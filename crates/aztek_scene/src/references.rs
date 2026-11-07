//! File references: `file -r -ns x path.ma` brings a file's nodes in under a namespace, marked as referenced, and the
//! scene saves the path with its edits rather than the contents. Unload takes the nodes away and keeps the edits; reload
//! reads the file again and puts them back. `reference` and `referenceQuery` work on the same table.
//!
//! An edit here is an attribute value that differs from what the file gave, or a connection between a referenced node and
//! one of the scene's. Maya keeps those in the reference node's `.ed`; a `.ma` written here keeps them as plain setAttr and
//! connectAttr lines after the `file -r` statement. A reference inside a referenced file becomes a reference of its own at
//! the top level. Loading one flushes undo, as in Maya.

use std::collections::{BTreeMap, HashSet};

use aztek_ops::{fail, objects, ArgType, Args, CmdResult, CommandDef, CommandError, Commands};
use serde_json::{json, Value};

use crate::graph::Graph;
use crate::maya_core::{asked, f};
use crate::node::{types, NodeType, Plain};
use crate::plug::{flags, from_json, to_json, NodeId, PlugRef, PlugType, NO_NODE};
use crate::session::Session;
use crate::system_cmds::q_flags;

type Cmd = CommandDef<Session>;
use ArgType::{Bool as B, String as S};

pub const REF_NODE_TYPE: &str = "reference";

#[derive(Clone, Debug, Default)]
pub struct RefEntry {
    pub id: u32,
    pub path: String,
    pub namespace: String,
    /// The reference node.
    pub rfn: String,
    pub loaded: bool,
    /// Values the file gave, by plug.
    baseline: BTreeMap<String, Value>,
    /// Kept while unloaded and written to files: attribute values and connections with the scene.
    pub edits: Vec<(String, Value)>,
    pub connections: Vec<(String, String)>,
}

#[derive(Default)]
pub struct RefTable {
    pub entries: Vec<RefEntry>,
    next: u32,
    depth: u32,
}

/// The stored attributes an edit can be, as (plug, value).
fn plug_values(g: &Graph, id: NodeId) -> Vec<(String, Value)> {
    let Some(n) = g.node(id) else { return Vec::new() };
    let path = g.path(id);
    let mut out = Vec::new();
    for (i, a) in n.ty().attrs.iter().enumerate() {
        if !a.has(flags::INPUT) || !a.has(flags::STORABLE) || a.is_array() || matches!(a.ty, PlugType::Mesh | PlugType::Data | PlugType::Matrix) {
            continue;
        }
        if g.source(&PlugRef::new(id, i as i32)).is_some() {
            continue;
        }
        out.push((format!("{path}.{}", a.name), to_json(n.value(i))));
    }
    out
}

fn nodes_of(g: &Graph, id: u32) -> Vec<NodeId> {
    g.nodes().into_iter().filter(|&n| g.node(n).is_some_and(|x| x.reference_id() == id)).collect()
}

/// Reads a reference's file into the scene. The entry's edits go back on top.
fn load(s: &mut Session, at: usize) -> Result<Vec<String>, String> {
    if s.extras.references.depth > 8 {
        return Err("references nested too deep".to_string());
    }
    s.extras.references.depth += 1;
    let (id, path, ns, rfn) = {
        let e = &s.extras.references.entries[at];
        (e.id, e.path.clone(), e.namespace.clone(), e.rfn.clone())
    };
    let before: HashSet<NodeId> = s.graph.nodes().into_iter().collect();
    let sel: Vec<NodeId> = s.graph.selection().to_vec();
    let was = s.graph.undo_stack().enabled();
    s.graph.undo_stack_mut().set_enabled(false);
    let r = crate::ma_file::import_scene(s, &path);
    let result = r.map(|warnings| {
        let g = &mut s.graph;
        if !ns.is_empty() {
            g.set_namespace_known(&ns, true);
        }
        let new: Vec<NodeId> = g.nodes().into_iter().filter(|n| !before.contains(n) && g.node(*n).is_some_and(|x| x.reference_id() == 0)).collect();
        for &n in &new {
            let name = g.node(n).unwrap().name().to_string();
            if !ns.is_empty() {
                let _ = g.rename(n, &format!("{ns}:{name}"));
            }
            g.set_reference(n, id);
        }
        // The reference node, marked so it's written as the reference and not as a node of the scene.
        if g.find(&rfn).ok().flatten().is_none() {
            if let Some(t) = types::find(REF_NODE_TYPE) {
                if let Ok(n) = g.create_node(&t, &rfn, NO_NODE, false) {
                    g.set_reference(n, id);
                }
            }
        }
        g.evaluate();
        let mut baseline = BTreeMap::new();
        for &n in &new {
            baseline.extend(plug_values(g, n));
        }
        let (edits, conns) = {
            let e = &s.extras.references.entries[at];
            (e.edits.clone(), e.connections.clone())
        };
        apply_edits(g, &edits);
        reconnect(g, &conns);
        let e = &mut s.extras.references.entries[at];
        e.baseline = baseline;
        e.loaded = true;
        warnings
    });
    s.graph.select(&sel, crate::graph::SelectMode::Replace);
    s.graph.undo_stack_mut().set_enabled(was);
    s.extras.references.depth -= 1;
    result
}

fn apply_edits(g: &mut Graph, edits: &[(String, Value)]) {
    for (plug, v) in edits {
        let Ok(p) = g.plug(plug) else { continue };
        let def = g.attr(&p).clone();
        if let Ok(val) = from_json(&def, v) {
            let _ = g.set_value(p, val);
        }
    }
}

fn reconnect(g: &mut Graph, conns: &[(String, String)]) {
    for (a, b) in conns {
        if let (Ok(x), Ok(y)) = (g.plug(a), g.plug(b)) {
            if let Err(e) = g.connect(&x, &y, true) {
                log::warn!("{a} -> {b}: {e}");
            }
        }
    }
}

/// What has changed on a loaded reference since its file gave it.
fn gather_edits(s: &Session, at: usize) -> (Vec<(String, Value)>, Vec<(String, String)>) {
    let g = &s.graph;
    let e = &s.extras.references.entries[at];
    if !e.loaded {
        return (e.edits.clone(), e.connections.clone());
    }
    let ids: HashSet<NodeId> = nodes_of(g, e.id).into_iter().collect();
    let mut edits = Vec::new();
    for &n in &ids {
        for (plug, v) in plug_values(g, n) {
            if e.baseline.get(&plug).is_some_and(|b| *b != v) {
                edits.push((plug, v));
            }
        }
    }
    edits.sort_by(|x, y| x.0.cmp(&y.0));
    let mut conns: Vec<(String, String)> = Vec::new();
    for (dst, src) in g.connections() {
        if ids.contains(&dst.node) != ids.contains(&src.node) {
            conns.push((g.plug_name(&src), g.plug_name(&dst)));
        }
    }
    (edits, conns)
}

/// Takes a reference's nodes out of the scene, keeping its edits.
fn unload(s: &mut Session, at: usize) {
    let (edits, conns) = gather_edits(s, at);
    let id = s.extras.references.entries[at].id;
    let rfn = s.extras.references.entries[at].rfn.clone();
    let g = &mut s.graph;
    let was = g.undo_stack().enabled();
    g.undo_stack_mut().set_enabled(false);
    let mut ids = nodes_of(g, id);
    ids.retain(|&n| g.node(n).is_some_and(|x| x.name() != rfn));
    for n in ids {
        if g.node(n).is_some() {
            let _ = g.delete_node(n);
        }
    }
    g.undo_stack_mut().set_enabled(was);
    let e = &mut s.extras.references.entries[at];
    e.edits = edits;
    e.connections = conns;
    e.loaded = false;
    e.baseline.clear();
}

fn add_entry(s: &mut Session, path: &str, ns: &str, rfn: Option<&str>) -> usize {
    let t = &mut s.extras.references;
    t.next += 1;
    let id = t.next;
    let stem = std::path::Path::new(path).file_stem().and_then(|x| x.to_str()).unwrap_or("reference").to_string();
    let ns = if ns.is_empty() { stem.clone() } else { ns.to_string() };
    // A namespace already taken takes a number, as a second reference of a file does.
    let mut unique = ns.clone();
    let mut k = 1;
    while t.entries.iter().any(|e| e.namespace == unique) {
        unique = format!("{ns}{k}");
        k += 1;
    }
    let rfn = rfn.map(str::to_string).unwrap_or(format!("{unique}RN"));
    t.entries.push(RefEntry { id, path: path.to_string(), namespace: unique, rfn, loaded: false, ..Default::default() });
    t.entries.len() - 1
}

/// `file -r`: returns the path as Maya does, with a copy number from the second reference of a file on.
pub fn create(s: &mut Session, path: &str, ns: &str, rfn: Option<&str>, deferred: bool) -> Result<String, String> {
    let copies = s.extras.references.entries.iter().filter(|e| e.path == path).count();
    if !std::path::Path::new(path).exists() {
        return Err(format!("file: {path} not found"));
    }
    let at = add_entry(s, path, ns, rfn);
    if !deferred {
        if let Err(e) = load(s, at) {
            s.extras.references.entries.remove(at);
            return Err(e);
        }
    }
    s.graph.undo_stack_mut().clear();
    Ok(if copies == 0 { path.to_string() } else { format!("{path}{{{copies}}}") })
}

/// Takes a path with its copy number, a reference node's name or a namespace, to the entry.
fn entry_of(s: &Session, name: &str) -> Option<usize> {
    let t = &s.extras.references.entries;
    if let Some((p, n)) = name.strip_suffix('}').and_then(|x| x.rsplit_once('{')) {
        let k: usize = n.parse().ok()?;
        return t.iter().enumerate().filter(|(_, e)| e.path == p).nth(k).map(|(i, _)| i);
    }
    t.iter().position(|e| e.rfn == name || e.path == name || e.namespace == name.trim_start_matches(':'))
}

/// The entry a node belongs to, from its name or its reference id.
fn entry_of_node(s: &Session, name: &str) -> Option<usize> {
    let g = &s.graph;
    let obj = name.split('.').next().unwrap_or(name);
    let id = g.find(obj).ok().flatten()?;
    let rid = g.node(id)?.reference_id();
    if rid == 0 {
        return None;
    }
    s.extras.references.entries.iter().position(|e| e.id == rid)
}

fn path_with_copy(s: &Session, at: usize) -> String {
    let t = &s.extras.references.entries;
    let path = &t[at].path;
    let k = t[..at].iter().filter(|e| e.path == *path).count();
    if k == 0 {
        path.clone()
    } else {
        format!("{path}{{{k}}}")
    }
}

/// Load, unload or remove by the flags of `file` and `reference`. None when none was asked for.
pub fn file_flags(s: &mut Session, a: &Args) -> Option<CmdResult> {
    let positional = a.pos_str(0).map(str::to_string);
    let named = |s: &Session, flag: &str| -> Option<CmdResult<usize>> {
        let v = a.get(flag)?;
        let name = v.as_str().map(str::to_string).filter(|n| !n.is_empty()).or_else(|| a.get("referenceNode").and_then(Value::as_str).map(str::to_string)).or_else(|| positional.clone()).unwrap_or_default();
        Some(entry_of(s, &name).ok_or_else(|| CommandError::new(format!("no reference {name}"))))
    };
    if let Some(at) = named(s, "unloadReference") {
        return Some(at.map(|at| {
            unload(s, at);
            s.graph.undo_stack_mut().clear();
            json!(path_with_copy(s, at))
        }));
    }
    if let Some(at) = named(s, "loadReference") {
        return Some(at.and_then(|at| {
            if s.extras.references.entries[at].loaded {
                unload(s, at);
            }
            load(s, at).map_err(CommandError::new)?;
            s.graph.undo_stack_mut().clear();
            Ok(json!(path_with_copy(s, at)))
        }));
    }
    if a.bool("removeReference") {
        let name = positional.or_else(|| a.get("referenceNode").and_then(Value::as_str).map(str::to_string)).unwrap_or_default();
        return Some(match entry_of(s, &name) {
            Some(at) => {
                unload(s, at);
                let e = s.extras.references.entries.remove(at);
                let rid = e.id;
                let g = &mut s.graph;
                let was = g.undo_stack().enabled();
                g.undo_stack_mut().set_enabled(false);
                for n in nodes_of(g, rid) {
                    let _ = g.delete_node(n);
                }
                g.set_namespace_known(&e.namespace, false);
                g.undo_stack_mut().set_enabled(was);
                g.undo_stack_mut().clear();
                Ok(json!(e.path))
            }
            None => Err(CommandError::new(format!("no reference {name}"))),
        });
    }
    None
}

/// `file -q -r`, the paths; `file -q -rfn path`, the reference node.
pub fn file_query(s: &Session, a: &Args, path: Option<&str>) -> Option<Value> {
    if asked(a, "reference") {
        return Some(Value::Array((0..s.extras.references.entries.len()).map(|i| json!(path_with_copy(s, i))).collect()));
    }
    if asked(a, "referenceNode") {
        let at = path.and_then(|p| entry_of(s, p))?;
        return Some(json!(s.extras.references.entries[at].rfn));
    }
    if asked(a, "namespace") {
        let at = path.and_then(|p| entry_of(s, p))?;
        return Some(json!(s.extras.references.entries[at].namespace));
    }
    None
}

fn reference_query(s: &mut Session, a: &Args) -> CmdResult {
    let name = objects(s, a).into_iter().next().unwrap_or_default();
    let by_node = entry_of_node(s, &name);
    if a.bool("isNodeReferenced") {
        // The reference node itself belongs to the scene.
        let is_rfn = s.extras.references.entries.iter().any(|e| e.rfn == name);
        return Ok(json!(by_node.is_some() && !is_rfn));
    }
    let Some(at) = by_node.or_else(|| entry_of(s, &name)) else { fail!("referenceQuery: {name} is not part of a reference") };
    let e = s.extras.references.entries[at].clone();
    if a.bool("filename") {
        if a.bool("shortName") {
            return Ok(json!(std::path::Path::new(&e.path).file_name().and_then(|x| x.to_str()).unwrap_or("")));
        }
        return Ok(json!(if a.bool("withoutCopyNumber") { e.path } else { path_with_copy(s, at) }));
    }
    if a.bool("nodes") {
        let g = &s.graph;
        let list: Vec<Value> = nodes_of(g, e.id).into_iter().filter(|&n| g.node(n).is_some_and(|x| x.name() != e.rfn)).map(|n| json!(if a.bool("dagPath") { g.full_path(n) } else { g.path(n) })).collect();
        return Ok(Value::Array(list));
    }
    if a.bool("namespace") {
        return Ok(json!(if a.bool("shortName") { e.namespace } else { format!(":{}", e.namespace) }));
    }
    if a.bool("referenceNode") {
        return Ok(json!(e.rfn));
    }
    if a.bool("isLoaded") {
        return Ok(json!(e.loaded));
    }
    if a.bool("parent") {
        return Ok(Value::Null);
    }
    if a.bool("topReference") {
        return Ok(json!(e.rfn));
    }
    if a.bool("editStrings") || a.bool("editNodes") || a.bool("editAttrs") {
        let (edits, _) = gather_edits(s, at);
        return Ok(Value::Array(edits.iter().map(|(p, v)| json!(format!("setAttr {p} {v}"))).collect()));
    }
    fail!("referenceQuery: ask for something: -f, -n, -ns, -rfn, -il or -inr")
}

/// A saved reference, for scene files.
pub fn to_doc(s: &Session) -> Vec<Value> {
    (0..s.extras.references.entries.len())
        .map(|at| {
            let (edits, conns) = gather_edits(s, at);
            let e = &s.extras.references.entries[at];
            json!({"path": e.path, "namespace": e.namespace, "rfn": e.rfn, "loaded": e.loaded,
                   "edits": edits.iter().map(|(p, v)| json!([p, v])).collect::<Vec<_>>(),
                   "connections": conns.iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>()})
        })
        .collect()
}

/// What to bring back after a scene's own nodes are read.
pub fn restore(s: &mut Session, list: &[Value]) -> Vec<String> {
    let mut warnings = Vec::new();
    for r in list {
        let (path, ns, rfn) = (r["path"].as_str().unwrap_or(""), r["namespace"].as_str().unwrap_or(""), r["rfn"].as_str());
        let at = add_entry(s, path, ns, rfn);
        let e = &mut s.extras.references.entries[at];
        e.edits = r["edits"].as_array().into_iter().flatten().filter_map(|x| Some((x[0].as_str()?.to_string(), x[1].clone()))).collect();
        e.connections = r["connections"].as_array().into_iter().flatten().filter_map(|x| Some((x[0].as_str()?.to_string(), x[1].as_str()?.to_string()))).collect();
        if r["loaded"].as_bool().unwrap_or(true) {
            if let Err(w) = load(s, at) {
                warnings.push(w);
                let g = &mut s.graph;
                if let Some(t) = types::find(REF_NODE_TYPE) {
                    let _ = g.without_undo(|g| g.create_node(&t, rfn.unwrap_or("referenceRN"), NO_NODE, false).map(|n| g.set_reference(n, 0)));
                }
            }
        }
    }
    warnings
}

/// The `.ma` lines for a scene's references, after the header: Maya's `file -rdi` and `file -r`, then the edits.
pub fn ma_statements(s: &Session) -> (String, String) {
    let mut head = String::new();
    let mut tail = String::new();
    for at in 0..s.extras.references.entries.len() {
        let (edits, conns) = gather_edits(s, at);
        let e = &s.extras.references.entries[at];
        let dr = if e.loaded { "" } else { " -dr 1" };
        head.push_str(&format!("file -rdi 1 -ns \"{ns}\" -rfn \"{rfn}\" \"{p}\";\n", ns = e.namespace, rfn = e.rfn, p = e.path));
        head.push_str(&format!("file -r -ns \"{ns}\"{dr} -rfn \"{rfn}\" \"{p}\";\n", ns = e.namespace, rfn = e.rfn, p = e.path));
        for (plug, v) in &edits {
            let Some((node, attr)) = plug.rsplit_once('.') else { continue };
            let vals = match v {
                Value::Array(l) => l.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" "),
                Value::Bool(b) => (if *b { "yes" } else { "no" }).to_string(),
                x => x.to_string(),
            };
            let ty = match v {
                Value::Array(l) if l.len() == 3 => " -type \"double3\"",
                Value::String(_) => " -type \"string\"",
                _ => "",
            };
            tail.push_str(&format!("select -ne \"{node}\";\n\tsetAttr \".{attr}\"{ty} {vals};\n"));
        }
        for (a, b) in &conns {
            tail.push_str(&format!("connectAttr \"{a}\" \"{b}\" -f;\n"));
        }
    }
    (head, tail)
}

/// `file -r` from a `.ma` while it's read: an entry to load now, with edits arriving as the file's later lines.
pub fn from_ma(s: &mut Session, path: &str, ns: &str, rfn: Option<&str>, deferred: bool) -> Result<(), String> {
    let at = add_entry(s, path, ns, rfn);
    if !std::path::Path::new(path).exists() {
        return Err("no such file".to_string());
    }
    if deferred {
        return Ok(());
    }
    load(s, at).map(|_| ())
}

/// Re-baselines a reference after its .ma's edits were applied: they stay edits, so the file's values are the baseline's.
pub fn register_types() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = types::register_type(NodeType {
            name: REF_NODE_TYPE.into(),
            doc: "A file reference: the scene's record of where its nodes come from.".into(),
            create: Some(std::sync::Arc::new(|| Box::new(Plain))),
            ..Default::default()
        });
    });
}

pub fn register(c: &mut Commands<Session>) {
    register_types();
    c.register(
        q_flags(Cmd::new(
            "reference",
            "File references: -f path -ns namespace brings one in; -ur, -lr, -rr unload, load and remove; -q -f lists the files.",
            |s, a| {
                if a.bool("query") {
                    return Ok(file_query(s, a, a.pos_str(0)).unwrap_or(Value::Null));
                }
                if let Some(r) = file_flags(s, a) {
                    return r;
                }
                let path = a.get("filename").and_then(Value::as_str).or(a.pos_str(0)).map(str::to_string);
                let Some(path) = path else { fail!("reference: give a file") };
                let r = create(s, &path, &a.string("namespace", ""), a.get("referenceNode").and_then(Value::as_str), a.bool("deferReference")).map_err(CommandError::new)?;
                Ok(json!(r))
            },
        ))
        .positional("file")
        .flag(f("filename", "f", S, "The file to reference."))
        .flag(f("reference", "r", B, "With -q: the referenced files."))
        .flag(f("namespace", "ns", S, "The namespace to put it under."))
        .flag(f("referenceNode", "rfn", S, "The reference node's name."))
        .flag(f("deferReference", "dr", B, "Add it unloaded."))
        .flag(f("loadReference", "lr", S, "Load this one."))
        .flag(f("unloadReference", "ur", S, "Unload this one."))
        .flag(f("removeReference", "rr", B, "Remove this one."))
        .undoable(false),
    );
    c.register(
        Cmd::new(
            "referenceQuery",
            "Ask about references, given a node, a reference node or a file: -inr isNodeReferenced, -f the file, -n nodes, -ns namespace, -rfn reference node, -il loaded.",
            reference_query,
        )
        .positional("node, reference node or file")
        .flag(f("isNodeReferenced", "inr", B, "Whether the node comes from a reference."))
        .flag(f("filename", "f", B, "The file."))
        .flag(f("nodes", "n", B, "The nodes it brought in."))
        .flag(f("namespace", "ns", B, "Its namespace."))
        .flag(f("referenceNode", "rfn", B, "Its reference node."))
        .flag(f("isLoaded", "il", B, "Whether it's loaded."))
        .flag(f("shortName", "shn", B, "The file's name without its folder, or the namespace without its colon."))
        .flag(f("withoutCopyNumber", "wcn", B, "The file without its {n}."))
        .flag(f("dagPath", "dp", B, "Full paths from -n."))
        .flag(f("parent", "p", B, "The reference above it: none, since references are flat here."))
        .flag(f("topReference", "tr", B, "The top reference: itself, since references are flat here."))
        .flag(f("editStrings", "es", B, "Its edits, as text."))
        .flag(f("editNodes", "en", B, "Its edits, as text."))
        .flag(f("editAttrs", "ea", B, "Its edits, as text."))
        .undoable(false),
    );
}
