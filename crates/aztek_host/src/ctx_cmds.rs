//! Tool contexts: named tools made by the Ctx commands, which `setToolTo` activates. Each context type maps to a tool the
//! views have; one with no tool yet is still made, and says so once when activated.

use std::cell::RefCell;
use std::collections::BTreeMap;

use aztek_ops::{fail, ArgType, Args, CmdResult, CommandDef, CommandError, FlagDef};
use aztek_scene::Session;
use serde_json::{json, Map, Value};

type Cmd = CommandDef<Session>;

#[derive(Clone, Debug)]
struct Context {
    kind: String,
    props: Map<String, Value>,
}

#[derive(Default)]
struct Contexts {
    all: BTreeMap<String, Context>,
    current: String,
    counters: BTreeMap<String, u32>,
    ready: bool,
    edit_mode: bool,
}

thread_local! {
    static CTX: RefCell<Contexts> = RefCell::new(Contexts::default());
}

/// Built in contexts, as Maya has them from the start: (name, type).
const BUILT_IN: &[(&str, &str)] = &[
    ("selectSuperContext", "selectSuperContext"),
    ("moveSuperContext", "moveSuperContext"),
    ("RotateSuperContext", "RotateSuperContext"),
    ("scaleSuperContext", "scaleSuperContext"),
    ("artAttrSkinContext", "artAttrSkinContext"),
    ("Move", "manipMoveContext"),
    ("Rotate", "manipRotateContext"),
    ("Scale", "manipScaleContext"),
    ("lassoContext", "lassoContext"),
    ("artSelectContext", "artSelectCtx"),
    ("selectContext", "selectSuperContext"),
    ("ShowManips", "selectSuperContext"),
    ("tumbleContext", "tumbleCtx"),
    ("trackContext", "trackCtx"),
    ("dollyContext", "dollyCtx"),
    ("rollContext", "rollCtx"),
    ("jointContext", "jointCtx"),
    ("ikHandleContext", "ikHandleCtx"),
    ("ikSplineHandleContext", "ikSplineHandleCtx"),
    ("curveContext", "curveCVCtx"),
    ("curveCVContext", "curveCVCtx"),
    ("curveEPContext", "curveEPCtx"),
    ("insertJointContext", "insertJointCtx"),
    ("polyCutContext", "polyCutCtx"),
    ("polySplitContext", "polySplitCtx"),
    ("polyCreateFacetContext", "polyCreateFacetCtx"),
    ("polyAppendFacetContext", "polyAppendFacetCtx"),
    ("targetWeldContext", "targetWeldCtx"),
    ("polySelectEditContext", "polySelectEditCtx"),
    ("polyShortestPathContext", "polyShortestPathCtx"),
];

/// A context type to the view's tool name; None when the views have no such tool yet.
fn tool_of(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "selectSuperContext" | "selectContext" | "polySelectEditCtx" => "select",
        "moveSuperContext" | "manipMoveContext" => "move",
        "RotateSuperContext" | "manipRotateContext" => "rotate",
        "scaleSuperContext" | "manipScaleContext" => "scale",
        "artAttrSkinContext" | "artAttrSkinPaintCtx" => "paintSkin",
        "lassoContext" => "lasso",
        "artSelectCtx" => "paintSelect",
        "tumbleCtx" => "camTumble",
        "trackCtx" => "camTrack",
        "dollyCtx" => "camDolly",
        "rollCtx" => "camRoll",
        "boxZoomCtx" => "camZoom",
        "jointCtx" => "joint",
        "curveCVCtx" => "curveCv",
        "curveEPCtx" => "curveEp",
        "insertJointCtx" => "insertJoint",
        "polySplitCtx" | "polySplitCtx2" => "multiCut",
        "polyCreateFacetCtx" => "createPolygon",
        "polyAppendFacetCtx" => "appendPolygon",
        "targetWeldCtx" => "targetWeld",
        "polyShortestPathCtx" => "shortestPath",
        "sculptMeshCacheCtx" => "sculpt",
        _ => return None,
    })
}

/// The shell's tool to Maya's context of that tool, for when the user picks a tool by hand.
fn context_of_tool(tool: &str) -> &'static str {
    match tool {
        "move" => "moveSuperContext",
        "rotate" => "RotateSuperContext",
        "scale" => "scaleSuperContext",
        "paintSkin" => "artAttrSkinContext",
        "lasso" => "lassoContext",
        "paintSelect" => "artSelectContext",
        "camTumble" => "tumbleContext",
        "camTrack" => "trackContext",
        "camDolly" => "dollyContext",
        "camRoll" => "rollContext",
        "joint" => "jointContext",
        "curveCv" => "curveContext",
        "curveEp" => "curveEPContext",
        "insertJoint" => "insertJointContext",
        "multiCut" => "polySplitContext",
        "createPolygon" => "polyCreateFacetContext",
        "appendPolygon" => "polyAppendFacetContext",
        "targetWeld" => "targetWeldContext",
        "shortestPath" => "polyShortestPathContext",
        _ => "selectSuperContext",
    }
}

fn with_ctx<R>(f: impl FnOnce(&mut Contexts) -> R) -> R {
    CTX.with(|c| {
        let mut c = c.borrow_mut();
        if !c.ready {
            c.ready = true;
            for (n, k) in BUILT_IN {
                c.all.insert(n.to_string(), Context { kind: k.to_string(), props: Map::new() });
            }
            c.current = "selectSuperContext".into();
        }
        f(&mut c)
    })
}

pub fn current() -> String {
    with_ctx(|c| c.current.clone())
}

/// The shell changed tool: the current context follows, unless it already means that tool.
pub fn shell_tool_changed(tool: &str) {
    with_ctx(|c| {
        let already = c.all.get(&c.current).and_then(|x| tool_of(&x.kind)) == Some(tool);
        if !already {
            c.current = context_of_tool(tool).to_string();
        }
    });
}

fn f(name: &str, short: &str, ty: ArgType) -> FlagDef {
    FlagDef::new(name, short, ty, Value::Null, "")
}

const CTX_FLAGS: &str = "image1:i1:a image2:i2:a image3:i3:a title:t:a exists:ex:b query:q:b edit:e:b queried::a name:n:a history:ch:b dragSlider:ds:a";

/// Flags beyond the ones every context has.
const TYPES: &[(&str, &str)] = &[
    ("manipMoveContext", "mode:m:a snap:s:b snapValue:sv:a snapRelative:sr:b snapComponentsRelative:scr:b activeHandle:ah:a position:p:b orientAxes:oa:a orientObject:oo:a orientJoint:oj:a orientJointEnabled:oje:b orientTowards:ot:a interactiveUpdate:iu:b editPivotMode:epm:b editPivotPosition:epp:b xformConstraint:xc:a xformConstraintAlongNormal:xcn:a preventNegativeScale:pns:b reflection:rfl:b reflectionAboutAxis:rab:b reflectionAboutBBox:rbb:b reflectionAboutOrigin:rao:b reflectionAboutX:rax:b reflectionAboutY:ray:b reflectionAboutZ:raz:b tweakMode:twk:b translate:tr:a lastMode:lm:b alignAlong:aa:a pinFaces:pf:b"),
    ("manipRotateContext", "mode:m:a snap:s:b snapValue:sv:a snapRelative:sr:b activeHandle:ah:a position:p:b rotate:ro:a objectSpace:os:b orientAxes:oa:a orientObject:oo:a orientJoint:oj:a orientJointEnabled:oje:b tweakMode:twk:b interactiveUpdate:iu:b editPivotMode:epm:b editPivotPosition:epp:b xformConstraint:xc:a modifyTranslation:mt:b preserveUV:puv:b lastMode:lm:b reflection:rfl:b centerTrackball:ctb:b alignAlong:aa:a"),
    ("manipScaleContext", "mode:m:a snap:s:b snapValue:sv:a snapRelative:sr:b activeHandle:ah:a position:p:b scale:sc:a orientAxes:oa:a orientObject:oo:a orientJoint:oj:a orientJointEnabled:oje:b tweakMode:twk:b interactiveUpdate:iu:b editPivotMode:epm:b editPivotPosition:epp:b xformConstraint:xc:a preserveUV:puv:b preventNegativeScale:pns:b useManipPivot:ump:b useObjectPivot:uop:b lastMode:lm:b reflection:rfl:b alignAlong:aa:a"),
    ("jointCtx", "autoJointOrient:ajo:a jointAutoLimits:jal:b jointOrientationPlane:jop:a secondaryAxisOrient:sao:a orientJoint:oj:a scaleJointOrient:sjo:b scaleOrientation:so:b scaleCompensate:sc:b degreeOfFreedomJ:dfj:a largeBoneRadius:lbr:a largeJointRadius:ljr:a smallBoneRadius:sbr:a smallJointRadius:sjr:a variableBoneSize:vbs:b createIKHandle:ikh:b ikSolverType:ikt:a poleVectorType:pvt:a snapHandleH:shh:b symmetry:sym:b symmetryAxis:sa:a"),
    ("ikHandleCtx", "autoPriorityH:apH:b createCurve:ccv:b createRootAxis:cra:b forceSolverH:fsH:b ikFKSwitching:ikf:b numSpans:ns:a parentCurve:pcv:b poWeightH:pwH:a priorityH:pH:a rootOnCurve:roc:b rootTwistMode:rtm:b simplifyCurve:scv:b snapCurve:snc:b snapHandleH:snH:b solverTypeH:stH:a stickyH:sH:a twistType:tws:a weightH:wH:a"),
    ("ikSplineHandleCtx", "autoPriorityH:apH:b createCurve:ccv:b createRootAxis:cra:b forceSolverH:fsH:b numSpans:ns:a parentCurve:pcv:b poWeightH:pwH:a priorityH:pH:a rootOnCurve:roc:b rootTwistMode:rtm:b simplifyCurve:scv:b snapCurve:snc:b snapHandleH:snH:b solverTypeH:stH:a stickyH:sH:a twistType:tws:a weightH:wH:a"),
    ("curveCVCtx", "degree:d:a multEndKnots:me:b preserveShape:ps:b rational:r:b refit:rf:b uniform:un:b bezier:bez:b"),
    ("curveEPCtx", "degree:d:a multEndKnots:me:b preserveShape:ps:b rational:r:b refit:rf:b uniform:un:b bezier:bez:b"),
    ("curveSketchCtx", "degree:d:a"),
    ("insertJointCtx", ""),
    ("polyCutCtx", "deleteFaces:df:b extractFaces:ef:a extractOffset:eo:a"),
    ("polySplitCtx", "precsnap:ps:a enablesnap:es:b magnetsnap:ms:a snaptoedge:ste:b smoothingangle:sma:a subdivision:s:a"),
    ("polyCreateFacetCtx", "append:ap:b exists2:ex2:b maximumNumberOfPoints:mnp:a planarConstraint:pc:b subdivision:s:a texture:tx:a autoUV? "),
    ("polyAppendFacetCtx", "append:ap:b maximumNumberOfPoints:mnp:a planarConstraint:pc:b rotate:ro:a subdivision:s:a texture:tx:a twist:tw:b"),
    ("targetWeldCtx", "mergeToCenter:mtc:b preserveUV:puv:b"),
    ("polySelectEditCtx", "mode:m:a insertWithEdgeFlow:ief:b insertWithEdgeFlowEdge:ife:b relativeInsert:r:b splitType:stp:a divisions:div:a absoluteOffset:ao:a autoComplete:ac:b fixQuads:fq:b smoothingAngle:sma:a useEqualMultiplier:uem:b deleteEdge:de:b adjustEdgeFlow:aef:a"),
    ("polyShortestPathCtx", "pathOnEdge:poe:b"),
    ("artAttrCtx", "attrSelected:asl:a selectedattroper:sao:a value:val:a radius:r:a lowerradius:lr:a opacity:op:a stampProfile:stP:a accopacity:aco:b minvalue:mnv:a maxvalue:mxv:a clamp:cl:a clamplower:cll:a clampupper:clu:a colorfeedback:cf:b useColorRamp:ucr:b mappressure:mp:a reflection:rfl:b reflectionaxis:ra:a importfileload:ifl:a exportfilesave:efs:a flood:fl:b floodselect:fs:a outline:o:b tablet:tab:b"),
    ("artSelectCtx", "selectop:sop:a radius:r:a lowerradius:lr:a opacity:op:a stampProfile:stP:a addselection:ads:b unselectall:ual:b selectall:sal:b outline:o:b tablet:tab:b reflection:rfl:b reflectionaxis:ra:a"),
    ("lassoContext", "drawClosed:dc:b"),
    ("skinBindCtx", "about:a:a axis:ax:a colorRamp:cr:a currentInfluence:ci:a displayInactiveMode:dim:a displayNormalized:dn:b falloffCurve:fc:a symmetry:sym:b tolerance:tol:a"),
    ("softModContext", "falloffMode:fm:a falloffRadius:fr:a"),
    ("setKeyCtx", "breakdown:bd:b"),
    ("moveKeyCtx", "moveFunction:mf:a option:o:a"),
    ("selectKeyCtx", ""),
    ("insertKeyCtx", "breakdown:bd:b"),
    ("scaleKeyCtx", "scaleSpecifiedKeys:ssk:b type:typ:a"),
    ("tumbleCtx", "autoOrthoConstrain:aoc:b autoSetPivot:asp:b localTumble:lt:a objectTumble:ot:b orthoLock:ol:b orthoStep:os:a tumbleScale:ts:a alternateContext:ac:b"),
    ("trackCtx", "trackScale:ts:a trackGeometry:tg:b alternateContext:ac:b"),
    ("dollyCtx", "scale:s:a localDolly:ld:b centerOfInterestDolly:cd:b boxDollyType:bdt:a dollyTowardsCenter:dtc:b alternateContext:ac:b toolName:tn:a"),
    ("rollCtx", "rollScale:rs:a alternateContext:ac:b"),
    ("boxZoomCtx", "zoomScale:zs:a"),
];

fn parse_flags(mut c: Cmd, spec: &str) -> Cmd {
    for t in spec.split_whitespace() {
        let mut it = t.split(':');
        let (Some(long), Some(short), Some(ty)) = (it.next(), it.next(), it.next()) else { continue };
        if c.flags.iter().any(|x| x.name == long) {
            continue;
        }
        c = c.flag(f(long, short, if ty == "b" { ArgType::Bool } else { ArgType::Any }));
    }
    c
}

fn asked(a: &Args) -> Option<String> {
    a.get("queried").and_then(Value::as_array).and_then(|q| q.first()).and_then(Value::as_str).map(str::to_string).or_else(|| a.flags.keys().find(|k| !matches!(k.as_str(), "query" | "queried")).cloned())
}

/// `manipMoveContext -m`: Maya's numbers to the axis names the views use.
fn axis_name(kind: &str, mode: i64) -> &'static str {
    if kind == "manipRotateContext" {
        return match mode {
            0 => "object",
            1 => "world",
            2 => "gimbal",
            _ => "object",
        };
    }
    match mode {
        0 => "object",
        1 => "parent",
        2 => "world",
        3 | 9 => "normal",
        _ => "object",
    }
}

fn view_all(key: &str, value: Value) {
    crate::core::push_ui_request(format!("viewAll|{}", json!({"key": key, "value": value})));
}

fn apply_to_views(kind: &str, flags: &Map<String, Value>) {
    if !matches!(kind, "manipMoveContext" | "manipRotateContext" | "manipScaleContext") {
        return;
    }
    if let Some(m) = flags.get("mode").and_then(Value::as_i64) {
        view_all("axisOrientation", json!(axis_name(kind, m)));
    }
    let step = match kind {
        "manipMoveContext" => "stepMove",
        "manipRotateContext" => "stepRotate",
        _ => "stepScale",
    };
    let on = flags.get("snap").map(|v| v.as_bool().unwrap_or(v.as_i64().is_some_and(|i| i != 0)));
    if let Some(v) = flags.get("snapValue").and_then(Value::as_f64) {
        if on != Some(false) {
            view_all(step, json!(v));
        }
    } else if on == Some(false) {
        view_all(step, json!(0.0));
    }
}

fn default_prop(kind: &str, key: &str) -> Value {
    match (kind, key) {
        ("manipMoveContext" | "manipScaleContext", "mode") => json!(2),
        ("manipRotateContext", "mode") => json!(0),
        ("manipMoveContext", "snapValue") => json!(1.0),
        ("manipRotateContext", "snapValue") => json!(15.0),
        ("manipScaleContext", "snapValue") => json!(1.0),
        (_, "activeHandle") => json!(3),
        (_, "position") => json!([0.0, 0.0, 0.0]),
        (_, "degree") => json!(3),
        (_, "title" | "image1" | "image2" | "image3") => json!(""),
        _ => Value::Null,
    }
}

fn context_command(kind: &'static str, s: &mut Session, a: &Args) -> CmdResult {
    let name = a.pos_str(0).map(str::to_string);
    if a.bool("exists") && !a.bool("query") {
        return Ok(json!(name.is_some_and(|n| with_ctx(|c| c.all.contains_key(&n)))));
    }
    if a.bool("query") {
        let Some(q) = asked(a) else { fail!("{kind}: name what to ask about.") };
        let Some(n) = name else { fail!("{kind}: name a context.") };
        if q == "exists" {
            return Ok(json!(with_ctx(|c| c.all.contains_key(&n))));
        }
        if q == "position" && kind.starts_with("manip") {
            return manip_position(s);
        }
        return with_ctx(|c| {
            let Some(x) = c.all.get(&n) else { fail!("{kind}: Object '{n}' not found.") };
            Ok(x.props.get(&q).cloned().unwrap_or_else(|| default_prop(&x.kind, &q)))
        });
    }
    let mut flags = a.flags.clone();
    flags.retain(|k, v| !matches!(k.as_str(), "query" | "edit" | "queried" | "exists" | "history") && !v.is_null());
    if a.bool("edit") {
        let Some(n) = name else { fail!("{kind}: name a context.") };
        let k = with_ctx(|c| -> Result<String, String> {
            let Some(x) = c.all.get_mut(&n) else { return Err(format!("{kind}: Object '{n}' not found.")) };
            for (fl, v) in &flags {
                x.props.insert(fl.clone(), v.clone());
            }
            Ok(x.kind.clone())
        })
        .map_err(CommandError::new)?;
        apply_to_views(&k, &flags);
        return Ok(json!(n));
    }
    let n = with_ctx(|c| -> Result<String, String> {
        let n = match name.or_else(|| flags.get("name").and_then(Value::as_str).map(str::to_string)) {
            Some(n) => {
                if c.all.contains_key(&n) {
                    return Err(format!("{kind}: Object's name '{n}' is not unique."));
                }
                n
            }
            None => {
                let e = c.counters.entry(kind.to_string()).or_insert(0);
                loop {
                    *e += 1;
                    let n = format!("{kind}{e}");
                    if !c.all.contains_key(&n) {
                        break n;
                    }
                }
            }
        };
        c.all.insert(n.clone(), Context { kind: kind.to_string(), props: flags.clone() });
        Ok(n)
    })
    .map_err(CommandError::new)?;
    Ok(json!(n))
}

/// The manipulator's position: the middle of what is selected.
fn manip_position(s: &mut Session) -> CmdResult {
    let names: Vec<String> = s.graph.selection().iter().map(|&i| s.graph.path(i)).collect();
    if names.is_empty() {
        return Ok(json!([0.0, 0.0, 0.0]));
    }
    let v = s.run("exactWorldBoundingBox", json!({"args": names}))?;
    let b: Vec<f64> = v.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
    if b.len() < 6 {
        return Ok(json!([0.0, 0.0, 0.0]));
    }
    Ok(json!([(b[0] + b[3]) * 0.5, (b[1] + b[4]) * 0.5, (b[2] + b[5]) * 0.5]))
}

fn set_tool_to(name: &str) -> CmdResult {
    let kind = with_ctx(|c| c.all.get(name).map(|x| x.kind.clone()));
    let Some(kind) = kind else { fail!("setToolTo: Object '{name}' not found.") };
    with_ctx(|c| c.current = name.to_string());
    match tool_of(&kind) {
        Some(t) => crate::core::push_ui_request(format!("setTool|{t}")),
        None => {
            crate::script_jobs::warn_once(&kind, &format!("{kind}: this tool has no interaction in the views yet; the context is current but the views stay on the select tool."));
            crate::core::push_ui_request("setTool|select".to_string());
        }
    }
    Ok(json!(name))
}

pub fn register(s: &mut Session) {
    for (kind, spec) in TYPES {
        let k: &'static str = kind;
        // A command the scene has, such as the paint tool's, stays.
        if s.commands.find(k).is_some() {
            continue;
        }
        let mut c = Cmd::new(k, &format!("A {k} tool context: create by name, -e and -q its settings. setToolTo makes it current."), move |s, a| context_command(k, s, a)).positional("[name]").undoable(false);
        c = parse_flags(c, spec);
        c = parse_flags(c, CTX_FLAGS);
        s.register(c);
    }

    s.register(
        Cmd::new("setToolTo", "Make a tool context current, by name.", |_s, a| {
            let Some(n) = a.pos_str(0) else { fail!("setToolTo: name a context.") };
            set_tool_to(n)
        })
        .positional("context")
        .undoable(false),
    );

    s.register(
        Cmd::new("currentCtx", "The name of the current tool context.", |_s, _a| Ok(json!(current())))
            .undoable(false)
            .flag(f("query", "q", ArgType::Bool))
            .flag(f("edit", "e", ArgType::Bool)),
    );

    s.register(
        Cmd::new("resetTool", "Put a tool context back to its default settings.", |_s, a| {
            let n = a.pos_str(0).map(str::to_string).unwrap_or_else(current);
            with_ctx(|c| {
                let Some(x) = c.all.get_mut(&n) else { fail!("resetTool: Object '{n}' not found.") };
                x.props.clear();
                Ok(Value::Null)
            })
        })
        .positional("[context]")
        .undoable(false),
    );

    s.register(
        Cmd::new("ctxEditMode", "Toggle the current tool's edit mode: the pivot, for the move, rotate and scale tools. -q reads it.", |_s, a| {
            if a.bool("query") {
                return Ok(json!(with_ctx(|c| c.edit_mode)));
            }
            let on = with_ctx(|c| {
                c.edit_mode = if a.has("buttonDown") || a.has("buttonUp") { a.bool("buttonDown") } else { !c.edit_mode };
                c.edit_mode
            });
            view_all("pivotMode", json!(on));
            Ok(Value::Null)
        })
        .undoable(false)
        .flag(f("query", "q", ArgType::Bool))
        .flag(f("buttonDown", "bd", ArgType::Bool))
        .flag(f("buttonUp", "bu", ArgType::Bool)),
    );

    s.register(
        Cmd::new("contextInfo", "About a tool context: -ex whether it exists, -c its class, -t its title.", |_s, a| {
            let Some(n) = a.pos_str(0) else { fail!("contextInfo: name a context.") };
            with_ctx(|c| {
                let x = c.all.get(n);
                if a.bool("exists") {
                    return Ok(json!(x.is_some()));
                }
                let Some(x) = x else { fail!("contextInfo: Object '{n}' not found.") };
                if a.bool("title") {
                    return Ok(x.props.get("title").cloned().unwrap_or(json!(n)));
                }
                if a.bool("image1") || a.bool("image2") || a.bool("image3") {
                    return Ok(json!(""));
                }
                Ok(json!(x.kind))
            })
        })
        .positional("context")
        .undoable(false)
        .flag(f("exists", "ex", ArgType::Bool))
        .flag(f("class", "c", ArgType::Bool))
        .flag(f("title", "t", ArgType::Bool))
        .flag(f("image1", "i1", ArgType::Bool))
        .flag(f("image2", "i2", ArgType::Bool))
        .flag(f("image3", "i3", ArgType::Bool)),
    );
}
