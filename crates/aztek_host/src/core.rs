//! The application core the UI talks to: the session, the script history and a change counter the panels poll.

use std::sync::Mutex;

use aztek_ops::{execute, CallSource};
use aztek_scene::Session;
use serde_json::{json, Map, Value};

use crate::mel;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LineKind {
    Echo = 0,
    Result = 1,
    Error = 2,
    Info = 3,
    Warning = 4,
}

#[derive(Clone)]
pub struct LogLine {
    pub text: String,
    pub kind: LineKind,
}

/// Warnings the scene code logs, turned into script history lines.
static CAPTURED: Mutex<Vec<(log::Level, String)>> = Mutex::new(Vec::new());

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Warn
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            if let Ok(mut c) = CAPTURED.lock() {
                c.push((r.level(), r.args().to_string()));
            }
            eprintln!("{}: {}", r.level(), r.args());
        }
    }
    fn flush(&self) {}
}

static LOGGER: Capture = Capture;

pub fn install_logger() {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Warn);
}

pub struct Core {
    pub s: Session,
    pub log: Vec<LogLine>,
    /// Lines the UI hasn't taken yet start here.
    pub log_read: usize,
    /// Bumped whenever anything may have changed; panels refresh when it moves.
    pub version: u64,
    /// The undo entry the last silent command made, which `retract_preview` may take back.
    preview: Vec<u64>,
    /// MEL's globals, procedures and top-level variables.
    pub mel_state: mel::MelState,
}

impl Core {
    pub fn new() -> Core {
        let mut c = Core { s: Session::new(), log: Vec::new(), log_read: 0, version: 1, preview: Vec::new(), mel_state: Default::default() };
        register_ui_commands(&mut c.s);
        crate::ui_cmds::register(&mut c.s);
        c.ensure_defaults();
        #[cfg(feature = "python")]
        crate::python::register_commands(&mut c.s);
        c
    }

    /// A core around a session moved here while Python is called back from inside a command: no UI commands of its own.
    #[cfg(feature = "python")]
    pub(crate) fn nested(s: Session) -> Core {
        Core { s, log: Vec::new(), log_read: 0, version: 1, preview: Vec::new(), mel_state: Default::default() }
    }

    /// A new scene has the default cameras and sets, as Maya's does. Cheap when they're there.
    pub fn ensure_defaults(&mut self) {
        if !self.s.graph.undo_stack().can_undo() {
            aztek_scene::cameras::make_defaults(&mut self.s.graph);
        }
    }

    pub fn push(&mut self, text: impl Into<String>, kind: LineKind) {
        self.log.push(LogLine { text: text.into(), kind });
    }

    fn drain_captured(&mut self) {
        #[cfg(feature = "python")]
        for (t, k) in crate::python::take_deferred() {
            self.push(t, k);
        }
        let lines: Vec<(log::Level, String)> = CAPTURED.lock().map(|mut c| std::mem::take(&mut *c)).unwrap_or_default();
        for (lvl, t) in lines {
            let kind = if lvl == log::Level::Error { LineKind::Error } else { LineKind::Warning };
            let prefix = if kind == LineKind::Error { "Error" } else { "Warning" };
            self.push(format!("// {prefix}: {t} //"), kind);
        }
    }

    /// Runs a command, echoing it as MEL; the result or error goes to the history.
    pub fn exec(&mut self, name: &str, call: Value) -> Result<Value, String> {
        // Echoed as Maya does: short flag names.
        let mut shown = call.clone();
        if let (Some(def), Some(o)) = (self.s.commands.find(name), shown.as_object_mut()) {
            let keys: Vec<String> = o.keys().cloned().collect();
            for k in keys {
                if let Some(f) = def.flags.iter().find(|f| f.name == k && !f.short_name.is_empty()) {
                    let v = o.remove(&k).unwrap();
                    o.insert(f.short_name.clone(), v);
                }
            }
        }
        self.push(mel::echo(name, &shown), LineKind::Echo);
        self.exec_quiet(name, call, CallSource::Ui)
    }

    pub fn exec_quiet(&mut self, name: &str, call: Value, src: CallSource) -> Result<Value, String> {
        let (name, call) = pivot_move(name, call);
        let r = execute(&mut self.s, &name, &call, src);
        self.ensure_defaults();
        self.version += 1;
        self.drain_captured();
        match r {
            Ok(v) => {
                if let Some(t) = mel::result(&v) {
                    self.push(t, LineKind::Result);
                }
                Ok(v)
            }
            Err(e) => {
                let msg = e.to_string();
                self.push(format!("// Error: {msg} //"), LineKind::Error);
                Err(msg)
            }
        }
    }

    /// Undoes the last silent command, if it recorded anything and nothing came after it: a drag's preview step.
    pub fn retract_preview(&mut self) {
        if let Some(id) = self.preview.pop() {
            if self.s.graph.undo_stack().newest_id() == id && self.s.graph.undo_stack().can_undo() {
                self.s.undo();
                self.version += 1;
            } else {
                self.preview.clear();
            }
        }
    }

    /// Takes back the last command (a manipulator drag replacing its own preview), dropping its history lines.
    pub fn retract(&mut self, lines: usize) {
        self.s.undo();
        let keep = self.log.len().saturating_sub(lines).max(self.log_read.min(self.log.len()));
        self.log.truncate(keep);
        self.version += 1;
    }

    /// Runs MEL source: echoes each statement and prints its result. An error goes to the history, ends the run and
    /// gives null. Otherwise returns the value of the last statement.
    pub fn mel(&mut self, src: &str) -> Value {
        mel::run(self, src).unwrap_or(Value::Null)
    }

    /// As `mel`, with the error back to the caller as well.
    pub fn mel_checked(&mut self, src: &str) -> Result<Value, String> {
        mel::run(self, src)
    }

    /// One command from MEL: no history lines; the interpreter reports the result or error as fits the statement.
    pub(crate) fn run_mel_command(&mut self, typed: &str, toks: &[mel::Tok]) -> Result<Value, String> {
        let (name, mut call) = mel::build(&self.s.commands, typed, toks)?;
        let qe = call.as_object_mut().and_then(|o| o.remove("_query").map(|a| (true, a)).or_else(|| o.remove("_edit").map(|a| (false, a))));
        if let Some((query, asked)) = qe {
            let o = call.as_object_mut().unwrap();
            let args: Vec<Value> = o.remove("args").and_then(|a| a.as_array().cloned()).unwrap_or_default();
            let mut flags = o.clone();
            for a in asked.as_array().into_iter().flatten() {
                if let Some(n) = a.as_str() {
                    flags.insert(n.to_string(), Value::Null);
                }
            }
            return self.query_edit(&name, args, flags, query);
        }
        let (name, call) = self.components_first(name, call);
        self.exec_plain(&name, call)
    }

    fn exec_plain(&mut self, name: &str, call: Value) -> Result<Value, String> {
        let (name, call) = pivot_move(name, call);
        let r = execute(&mut self.s, &name, &call, CallSource::Script);
        self.version += 1;
        self.drain_captured();
        r.map_err(|e| e.to_string())
    }

    /// Maya's component arguments (polyExtrudeFacet pCube1.f[2]): Aztek's component commands work on the selection,
    /// so select them first. move/rotate/scale/delete on components become their component commands.
    fn components_first(&mut self, mut name: String, mut call: Value) -> (String, Value) {
        let Some(args) = call.get("args").and_then(Value::as_array).cloned() else { return (name, call) };
        let (comps, rest): (Vec<Value>, Vec<Value>) = args.into_iter().partition(|a| a.as_str().is_some_and(mel::is_component));
        if comps.is_empty() {
            return (name, call);
        }
        let renamed = match name.as_str() {
            "move" => Some("moveComponents"),
            "rotate" => Some("rotateComponents"),
            "scale" => Some("scaleComponents"),
            "delete" => Some("deleteComponents"),
            _ => None,
        };
        if renamed.is_none() && !mel::takes_components(&name) {
            return (name, call);
        }
        if let Some(r) = renamed {
            name = r.to_string();
            if let Some(o) = call.as_object_mut() {
                // Components move relatively here.
                if let Some(to) = o.remove("to") {
                    o.insert("by".into(), to);
                }
                // The object form's switches mean nothing to the component commands.
                o.retain(|k, _| matches!(k.as_str(), "by" | "args" | "pivot"));
            }
        }
        let _ = self.exec_plain("select", json!({ "args": comps }));
        if let Some(o) = call.as_object_mut() {
            if rest.is_empty() {
                o.remove("args");
            } else {
                o.insert("args".into(), Value::Array(rest));
            }
        }
        (name, call)
    }

    /// A command run from Python: no history lines, errors back to the caller (who raises them).
    pub fn exec_silent(&mut self, name: &str, call: Value) -> Result<Value, String> {
        let before = self.s.graph.undo_stack().newest_id();
        let (name, call) = pivot_move(name, call);
        let r = execute(&mut self.s, &name, &call, CallSource::Script);
        self.ensure_defaults();
        let after = self.s.graph.undo_stack().newest_id();
        if after != before {
            self.preview.push(after);
            if self.preview.len() > 64 {
                self.preview.remove(0);
            }
        }
        self.version += 1;
        self.drain_captured();
        r.map_err(|e| e.to_string())
    }

    /// aztek.cmds.<name>(*args, **kwargs): Maya's spellings, short flags, query and edit modes.
    pub fn py_call(&mut self, typed: &str, args: Vec<Value>, mut kwargs: Map<String, Value>) -> Result<Value, String> {
        // optionVar(q='name'): the query flag carrying what is queried.
        let mut args = args;
        for k in ["q", "query"] {
            if let Some(Value::String(what)) = kwargs.get(k).cloned() {
                args.push(json!(what));
                kwargs.insert(k.to_string(), json!(true));
            }
        }
        let al = mel::alias(typed);
        let name = al.map(|a| a.target.to_string()).unwrap_or(typed.to_string());
        let def_flags: Vec<(String, String, aztek_ops::ArgType)> = match self.s.commands.find(&name) {
            Some(d) => d.flags.iter().map(|f| (f.name.clone(), f.short_name.clone(), f.ty)).collect(),
            None => return Err(format!("Cannot find procedure \"{typed}\".")),
        };
        let on = |v: &Value| v.as_bool().unwrap_or(v.as_i64().is_some_and(|i| i != 0));
        let query = kwargs.get("q").or(kwargs.get("query")).is_some_and(on);
        let edit = kwargs.get("e").or(kwargs.get("edit")).is_some_and(on);
        // Commands with their own query mode take q=True and the flags asked about.
        let own_query = def_flags.iter().any(|(n, _, _)| n == "query");
        if own_query && (query || edit) {
            let mut flags = Map::new();
            let mut asked = Vec::new();
            flags.insert(if query { "query" } else { "edit" }.into(), json!(true));
            for (k, v) in &kwargs {
                if matches!(k.as_str(), "q" | "query" | "e" | "edit") {
                    continue;
                }
                let Some((long, _, ty)) = def_flags.iter().find(|(n, s, _)| n == k || (!s.is_empty() && s == k)) else {
                    return Err(format!("{typed}: invalid flag '{k}'"));
                };
                if query && v == &Value::Bool(true) && *ty != aztek_ops::ArgType::Bool {
                    asked.push(json!(long));
                } else if query && v == &Value::Bool(true) {
                    asked.push(json!(long));
                    flags.insert(long.clone(), json!(true));
                } else {
                    flags.insert(long.clone(), v.clone());
                }
            }
            if def_flags.iter().any(|(n, _, _)| n == "queried") {
                flags.insert("queried".into(), Value::Array(asked));
            } else {
                for a in asked {
                    if let Some(n) = a.as_str() {
                        flags.entry(n.to_string()).or_insert(json!(true));
                    }
                }
            }
            if !args.is_empty() {
                flags.insert("args".into(), Value::Array(args));
            }
            return self.exec_silent(&name, Value::Object(flags));
        }
        let mut flags = Map::new();
        let mut relative = false;
        for (k, v) in &kwargs {
            if matches!(k.as_str(), "q" | "query" | "e" | "edit") {
                continue;
            }
            let mut key = k.as_str();
            if let Some(a) = al {
                if let Some((_, ours)) = a.flags.iter().find(|(m, _)| *m == key) {
                    key = ours;
                }
            }
            match def_flags.iter().find(|(n, s, _)| n == key || (!s.is_empty() && s == key)) {
                Some((long, _, ty)) => {
                    let v = if *ty == aztek_ops::ArgType::Bool && !v.is_boolean() { json!(on(v)) } else { v.clone() };
                    flags.insert(long.clone(), v);
                }
                None => {
                    if matches!(key, "r" | "relative") && matches!(name.as_str(), "move" | "rotate" | "scale") {
                        relative = on(v);
                    } else if query || edit {
                        // Attribute flags for query/edit on a node.
                        flags.insert(key.to_string(), v.clone());
                    } else if !matches!(key, "r" | "replace" | "ws" | "worldSpace" | "os" | "objectSpace" | "a" | "absolute" | "ch" | "constructionHistory" | "kft" | "keepFacesTogether" | "divisions") {
                        return Err(format!("{typed}: invalid flag '{k}'"));
                    }
                }
            }
        }
        let mut args = args;
        if query || edit {
            return self.query_edit(&name, args, flags, query);
        }
        if matches!(name.as_str(), "move" | "rotate" | "scale") && args.len() >= 3 && args[..3].iter().all(|a| a.is_number()) {
            let v: Vec<Value> = args.drain(..3).collect();
            flags.insert(if relative { "by" } else { "to" }.into(), Value::Array(v));
        }
        if !args.is_empty() {
            flags.insert("args".into(), Value::Array(args));
        }
        let (name, call) = self.components_first(name, Value::Object(flags));
        self.exec_silent(&name, call)
    }

    /// Maya's -q and -e on a node's settings: each flag is an attribute of the node named first (or selected), or of
    /// its shape's history (polyCube1 for pCube1).
    fn query_edit(&mut self, name: &str, args: Vec<Value>, flags: Map<String, Value>, query: bool) -> Result<Value, String> {
        let g = &self.s.graph;
        let target = match args.first().and_then(Value::as_str) {
            Some(t) => t.to_string(),
            None => match g.selection().last() {
                Some(&id) => g.path(id),
                None => return Err(format!("{name}: no object given or selected")),
            },
        };
        let mut nodes = vec![target.clone()];
        if let Ok(Some(id)) = g.find(&target) {
            if let Some(shape) = aztek_scene::components::as_mesh(g, id) {
                for h in aztek_scene::history::history_of(g, shape) {
                    nodes.push(g.path(h));
                }
            }
        }
        let mut out = Value::Null;
        for (attr, v) in flags {
            let plug = nodes.iter().map(|n| format!("{n}.{attr}")).find(|p| execute(&mut self.s, "getAttr", &json!({ "args": [p] }), CallSource::Script).is_ok());
            let Some(plug) = plug else { return Err(format!("{name}: {target} has no {attr}")) };
            if query {
                out = self.exec_silent("getAttr", json!({ "args": [plug] }))?;
            } else {
                self.exec_silent("setAttr", json!({ "args": [plug, v] }))?;
            }
        }
        Ok(out)
    }

    pub fn take_log(&mut self) -> Value {
        self.drain_captured();
        let start = self.log_read.min(self.log.len());
        let lines: Vec<Value> = self.log[start..].iter().map(|l| json!({"text": l.text, "kind": l.kind as i32})).collect();
        self.log_read = self.log.len();
        if self.log.len() > 20000 {
            self.log.drain(..10000);
            self.log_read = self.log.len();
        }
        Value::Array(lines)
    }

    /// Saves in the format the path's extension names: `.ma` or Aztek's own.
    pub fn save(&mut self, path: &str) -> Result<(), String> {
        aztek_scene::ma_file::save_scene(&mut self.s, path, aztek_scene::ma_file::Format::of_path(path))?;
        self.drain_captured();
        self.version += 1;
        self.push(format!("// Saved {path} //"), LineKind::Info);
        Ok(())
    }

    pub fn open(&mut self, path: &str) -> Result<(), String> {
        let warnings = aztek_scene::ma_file::open_scene(&mut self.s, path)?;
        // Files older than the default cameras, and .ma files, which leave them out, get them now.
        aztek_scene::cameras::make_defaults(&mut self.s.graph);
        // Warnings from a .ma file arrive through the log.
        self.drain_captured();
        if aztek_scene::ma_file::Format::of_path(path) != aztek_scene::ma_file::Format::MayaAscii {
            for w in warnings {
                self.push(format!("// Warning: {w} //"), LineKind::Warning);
            }
        }
        self.version += 1;
        self.push(format!("// Opened {path} //"), LineKind::Info);
        Ok(())
    }

    pub fn new_scene(&mut self) {
        let _ = self.exec("newScene", json!({}));
        self.s.scene_path = None;
    }
}

/// MEL's python("..."): runs Python and gives back the value of a lone expression.
#[cfg(feature = "python")]
pub(crate) fn python_from_mel(core: &mut Core, src: &str) -> Result<Value, String> {
    crate::python::eval_from_mel(core, src)
}

#[cfg(not(feature = "python"))]
pub(crate) fn python_from_mel(_core: &mut Core, _src: &str) -> Result<Value, String> {
    Err("python: this build has no Python.".into())
}

/// Windows and tools scripts open by Maya's runtime command names; the UI takes them from `take_ui_requests`.
static UI_REQUESTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub const UI_COMMANDS: &[(&str, &str)] = &[
    ("GraphEditor", "Open the Graph Editor."),
    ("NodeEditorWindow", "Open the Node Editor."),
    ("DopeSheetEditor", "Open the Dope Sheet."),
    ("SetDrivenKeyOptions", "Open the Set Driven Key window."),
    ("ScriptEditor", "Open the Script Editor."),
    ("OutlinerWindow", "Show the Outliner."),
    ("AttributeEditor", "Show the Attribute Editor."),
    ("TextureViewWindow", "Open the UV Editor."),
    ("UVEditor", "Open the UV Editor."),
    ("PreferencesWindow", "Open the Preferences."),
    ("ToggleChannelsLayers", "Show or hide the Channel Box / Layer Editor."),
    ("FrameSelected", "Frame the selection in the active view."),
    ("FrameAll", "Frame everything in the active view."),
    ("PlaybackToggle", "Start or stop playback."),
    ("JointTool", "The joint tool: click to place joints."),
    ("InsertJointTool", "The insert joint tool: drag from a joint to add one on the bone."),
    ("SculptMeshActivateBrushSculpt", "The sculpt brush."),
    ("MultiCutTool", "The Multi-Cut tool."),
    ("SplitEdgeRingTool", "The Insert Edge Loop tool."),
    ("OffsetEdgeLoopTool", "The Offset Edge Loop tool."),
    ("TargetWeldTool", "The Target Weld tool."),
    ("SlideEdgeTool", "The Slide Edge tool."),
    ("PolyCreaseTool", "The Crease tool."),
    ("CreatePolygonTool", "The Create Polygon tool."),
    ("AppendToPolygonTool", "The Append to Polygon tool."),
    ("QuadDrawTool", "The Quad Draw tool."),
    ("CVCurveTool", "The CV curve tool: click to place CVs, Enter to finish."),
    ("EPCurveTool", "The EP curve tool: click to place edit points, Enter to finish."),
    ("ArtPaintSkinWeightsTool", "The paint skin weights tool."),
    ("ArtPaintSkinWeightsToolOptions", "The paint skin weights tool and its settings."),
    ("ArtPaintAttrTool", "The paint attribute tool, on deformers' weights."),
    ("ArtPaintAttrToolOptions", "The paint attribute tool and its settings."),
    ("EditMembershipTool", "The edit membership tool: which points a deformer holds."),
    ("ShapeEditor", "The Shape Editor: blend shapes and their targets."),
    ("ToolSettingsWindow", "Show the Tool Settings."),
    ("SelectTool", "The select tool."),
    ("MoveTool", "The move tool."),
    ("RotateTool", "The rotate tool."),
    ("ScaleTool", "The scale tool."),
    ("FourViewLayout", "Four views: top, persp, front, side."),
    ("SingleViewLayout", "One view: the perspective."),
];

fn register_ui_commands(s: &mut Session) {
    s.register(
        aztek_ops::CommandDef::<Session>::new(
            "artSetToolAndSelectAttr",
            "Picks the paint attribute tool and what it paints: artSetToolAndSelectAttr artAttrCtx cluster.cluster1.weights.",
            |s, a| {
                let names = a.named();
                let Some(attr) = names.last().filter(|_| names.len() >= 2) else { aztek_ops::fail!("artSetToolAndSelectAttr: give the tool and nodeType.node.attribute") };
                aztek_scene::deform_paint::select_attr(s, attr)?;
                UI_REQUESTS.lock().unwrap().push("ArtPaintAttrTool".to_string());
                Ok(Value::Null)
            },
        )
        .positional("the tool, then nodeType.node.attribute")
        .undoable(false),
    );
    for (name, doc) in UI_COMMANDS {
        let n = name.to_string();
        s.register(
            aztek_ops::CommandDef::<Session>::new(name, doc, move |_s, _a| {
                UI_REQUESTS.lock().unwrap().push(n.clone());
                Ok(Value::Null)
            })
            .undoable(false),
        );
    }
}

/// Asks the shell to do something, by runtime command name or "name|payload".
pub fn push_ui_request(r: String) {
    UI_REQUESTS.lock().unwrap().push(r);
}

pub fn take_ui_requests() -> Value {
    json!(std::mem::take(&mut *UI_REQUESTS.lock().unwrap()))
}

/// Maya moves pivots with move on obj.scalePivot / obj.rotatePivot: that is movePivot here.
fn pivot_move(name: &str, call: Value) -> (String, Value) {
    if name != "move" {
        return (name.to_string(), call);
    }
    let Some(args) = call.get("args").and_then(Value::as_array) else { return (name.to_string(), call) };
    if !args.iter().any(|a| a.as_str().is_some_and(|s| s.ends_with(".rotatePivot") || s.ends_with(".scalePivot") || s.ends_with(".rp") || s.ends_with(".sp"))) {
        return (name.to_string(), call);
    }
    let mut objs: Vec<Value> = Vec::new();
    for a in args.iter().filter_map(Value::as_str) {
        let o = json!(a.split('.').next().unwrap_or(a));
        if !objs.contains(&o) {
            objs.push(o);
        }
    }
    let mut c = call.as_object().cloned().unwrap_or_default();
    c.insert("args".into(), Value::Array(objs));
    ("movePivot".into(), Value::Object(c))
}
