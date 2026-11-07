//! HumanIK-style characters: a standard skeleton in T-pose, character definitions mapping joints to named slots (automatic
//! from common naming, or set by hand), a control rig (FK spine and head, IK hands and feet with pole vectors), retargeting
//! one character onto another through constraints, and baking. A character is a HIKCharacterNode holding its slots.
//! This is a working subset; the full-body solver (reach and pinning) is not here.

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Arc;

use aztek_ops::{fail, ArgType, Args, CmdResult, CommandDef, Commands, FlagDef};
use glam::DVec3;
use serde_json::{json, Value};

use crate::graph::Graph;
use crate::node::{types, EvalContext, NodeBehavior, NodeData, NodeType};
use crate::plug::{NodeId, NO_NODE};
use crate::rig::{data_attr, is_joint, json_of, set_json, world};
use crate::session::Session;

pub const TYPE: &str = "HIKCharacterNode";
const SLOTS: usize = 0;

/// Slot names with the ids `setCharacterObject` takes.
pub const SLOT_IDS: &[(&str, i64)] = &[
    ("Reference", 0), ("Hips", 1), ("LeftUpLeg", 2), ("LeftLeg", 3), ("LeftFoot", 4), ("RightUpLeg", 5), ("RightLeg", 6), ("RightFoot", 7),
    ("Spine", 8), ("LeftArm", 9), ("LeftForeArm", 10), ("LeftHand", 11), ("RightArm", 12), ("RightForeArm", 13), ("RightHand", 14), ("Head", 15),
    ("LeftToeBase", 16), ("RightToeBase", 17), ("LeftShoulder", 18), ("RightShoulder", 19), ("Neck", 20), ("Spine1", 23), ("Spine2", 24), ("Spine3", 25),
    ("Spine4", 26), ("Spine5", 27), ("Neck1", 32),
];

struct CharacterNode;

impl NodeBehavior for CharacterNode {
    fn compute(&mut self, _c: &EvalContext, _d: &mut NodeData) {}
    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn register_types() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        types::register_type(NodeType {
            name: TYPE.into(),
            icon: "bone".into(),
            doc: "A character definition: joints by slot.".into(),
            own_attrs: vec![data_attr("slots", "sl", "Slot name to joint name, and the source character.")],
            create: Some(Arc::new(|| Box::new(CharacterNode))),
            ..Default::default()
        })
        .expect(TYPE);
    });
}

pub(crate) fn slots_of(g: &Graph, c: NodeId) -> BTreeMap<String, String> {
    let v = g.node(c).map(|n| json_of(n.data(), SLOTS)).unwrap_or(Value::Null);
    v.get("slots").and_then(Value::as_object).map(|o| o.iter().filter_map(|(k, x)| Some((k.clone(), x.as_str()?.to_string()))).collect()).unwrap_or_default()
}

fn set_slots(g: &mut Graph, c: NodeId, slots: &BTreeMap<String, String>) -> CmdResult<()> {
    let mut v = g.node(c).map(|n| json_of(n.data(), SLOTS)).unwrap_or(json!({}));
    if !v.is_object() {
        v = json!({});
    }
    v["slots"] = json!(slots);
    set_json(g, c, SLOTS, v)
}

pub(crate) fn character(g: &Graph, name: &str, cmd: &str) -> CmdResult<NodeId> {
    match g.find(name)? {
        Some(id) if g.node(id).is_some_and(|n| n.ty().name == TYPE) => Ok(id),
        _ => fail!("{cmd}: {name} is not a character"),
    }
}

/// The slot a joint's name means, whatever rig it came from: Character1_LeftArm, mixamorig:LeftArm, L_UpperArm, l_arm...
pub fn slot_for_name(name: &str) -> Option<String> {
    let short = name.rsplit(['|', ':']).next().unwrap_or(name);
    let bare = short.rsplit('_').next().unwrap_or(short);
    // The exact slot names first, with any prefix off.
    if let Some((s, _)) = SLOT_IDS.iter().find(|(s, _)| *s == bare || s.eq_ignore_ascii_case(short)) {
        return Some(s.to_string());
    }
    let low = short.to_lowercase();
    let side = if low.starts_with("l_") || low.ends_with("_l") || low.contains("left") {
        Some("Left")
    } else if low.starts_with("r_") || low.ends_with("_r") || low.contains("right") {
        Some("Right")
    } else {
        None
    };
    let part = [
        ("upperarm", "Arm"), ("forearm", "ForeArm"), ("lowerarm", "ForeArm"), ("elbow", "ForeArm"), ("clavicle", "Shoulder"), ("shoulder", "Shoulder"), ("hand", "Hand"), ("wrist", "Hand"), ("arm", "Arm"),
        ("thigh", "UpLeg"), ("upleg", "UpLeg"), ("calf", "Leg"), ("shin", "Leg"), ("knee", "Leg"), ("lowerleg", "Leg"), ("ankle", "Foot"), ("foot", "Foot"), ("toe", "ToeBase"), ("ball", "ToeBase"), ("leg", "Leg"),
    ];
    if let Some(sd) = side {
        return part.iter().find(|(k, _)| low.contains(*k)).map(|(_, p)| format!("{sd}{p}"));
    }
    if low.contains("hips") || low.contains("pelvis") {
        return Some("Hips".into());
    }
    if low.contains("head") {
        return Some("Head".into());
    }
    if low.contains("neck") {
        return Some("Neck".into());
    }
    if low.contains("spine") || low.contains("chest") {
        let n: String = low.chars().filter(char::is_ascii_digit).collect();
        let n: u32 = n.parse().unwrap_or(0);
        return Some(if n == 0 { "Spine".into() } else { format!("Spine{n}") });
    }
    None
}

type Cmd = CommandDef<Session>;

fn f(name: &str, short: &str, ty: ArgType, doc: &str) -> FlagDef {
    FlagDef::new(name, short, ty, Value::Null, doc)
}

fn slot_arg(a: &Args, i: usize) -> Option<String> {
    let v = a.positional.get(i)?;
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    let id = v.as_i64()?;
    SLOT_IDS.iter().find(|x| x.1 == id).map(|x| x.0.to_string())
}

/// A joint's world place.
fn at(g: &mut Graph, name: &str) -> Option<DVec3> {
    g.evaluate();
    let id = g.find(name).ok().flatten()?;
    Some(world(g, id).w_axis.truncate())
}

fn make_skeleton(s: &mut Session, a: &Args) -> CmdResult {
    let name = a.string("name", "Character1");
    let h = a.float("height", 170.0) / 170.0;
    let spines = a.int("spineCount", 3).clamp(1, 6);
    let mut out: Vec<String> = Vec::new();
    let mut slots: BTreeMap<String, String> = BTreeMap::new();
    let mut joint = |s: &mut Session, slot: &str, parent: Option<&str>, p: [f64; 3]| -> CmdResult<String> {
        match parent {
            Some(x) => s.run("select", json!({"args": [x]}))?,
            None => s.run("select", json!({"clear": true}))?,
        };
        let n = format!("{name}_{slot}");
        let r = s.run("joint", json!({"position": [p[0] * h, p[1] * h, p[2] * h], "name": n, "radius": 3.0 * h}))?;
        let made = r.as_str().unwrap_or(&n).to_string();
        slots.insert(slot.to_string(), made.clone());
        out.push(made.clone());
        Ok(made)
    };
    let hips = joint(s, "Hips", None, [0.0, 95.0, 0.0])?;
    let mut last = hips.clone();
    for i in 0..spines {
        let slot = if i == 0 { "Spine".to_string() } else { format!("Spine{i}") };
        last = joint(s, &slot, Some(&last), [0.0, 95.0 + 45.0 * (i + 1) as f64 / spines as f64, 0.0])?;
    }
    let chest = last.clone();
    let neck = joint(s, "Neck", Some(&chest), [0.0, 148.0, 0.0])?;
    joint(s, "Head", Some(&neck), [0.0, 158.0, 0.0])?;
    for (side, x) in [("Left", 1.0), ("Right", -1.0)] {
        let sh = joint(s, &format!("{side}Shoulder"), Some(&chest), [x * 6.0, 142.0, 0.0])?;
        let arm = joint(s, &format!("{side}Arm"), Some(&sh), [x * 18.0, 142.0, 0.0])?;
        let fore = joint(s, &format!("{side}ForeArm"), Some(&arm), [x * 45.0, 142.0, -1.0])?;
        let hand = joint(s, &format!("{side}Hand"), Some(&fore), [x * 70.0, 142.0, 0.0])?;
        if a.bool("fingers") {
            for (k, fname) in ["Thumb", "Index", "Middle", "Ring", "Pinky"].iter().enumerate() {
                let z = 3.0 - k as f64 * 1.5;
                let mut prev = hand.clone();
                for seg in 1..=3 {
                    prev = joint(s, &format!("{side}Hand{fname}{seg}"), Some(&prev), [x * (72.0 + seg as f64 * 2.5), 142.0, z])?;
                }
            }
        }
        let up = joint(s, &format!("{side}UpLeg"), Some(&hips), [x * 9.0, 90.0, 0.0])?;
        let leg = joint(s, &format!("{side}Leg"), Some(&up), [x * 9.0, 50.0, 1.0])?;
        let foot = joint(s, &format!("{side}Foot"), Some(&leg), [x * 9.0, 8.0, 0.0])?;
        joint(s, &format!("{side}ToeBase"), Some(&foot), [x * 9.0, 2.0, 12.0])?;
    }
    s.run("select", json!({"args": [hips]}))?;
    if a.bool("character") {
        let t = types::find(TYPE).unwrap();
        let c = s.graph.create_node(&t, &name, NO_NODE, true)?;
        set_slots(&mut s.graph, c, &slots)?;
    }
    Ok(json!(out))
}

/// Adds a control: a locator named as Maya names them, at a place, in the rig's group.
fn control(s: &mut Session, name: &str, at_p: DVec3, group: &str) -> CmdResult<String> {
    let r = s.run("spaceLocator", json!({"name": name}))?;
    let n = r.get(0).and_then(Value::as_str).unwrap_or(name).to_string();
    s.run("move", json!({"args": [n.clone()], "to": at_p.to_array()}))?;
    s.run("parent", json!({"args": [n.clone(), group]}))?;
    Ok(n)
}

fn create_control_rig(s: &mut Session, a: &Args) -> CmdResult {
    let Some(cname) = a.pos_str(0).map(str::to_string) else { fail!("hikCreateControlRig: name a character") };
    let c = character(&s.graph, &cname, "hikCreateControlRig")?;
    let slots = slots_of(&s.graph, c);
    if !slots.contains_key("Hips") {
        fail!("hikCreateControlRig: {cname} has no Hips");
    }
    let grp = format!("{cname}_Ctrl_Reference");
    s.run("group", json!({"empty": true, "name": grp}))?;
    let mut made = vec![grp.clone()];
    // FK down the middle: each control follows the one before and turns its joint.
    let mut parent_ctrl = grp.clone();
    let mut order = vec!["Hips".to_string(), "Spine".to_string()];
    for i in 1..=5 {
        order.push(format!("Spine{i}"));
    }
    order.push("Neck".into());
    order.push("Head".into());
    for slot in order {
        let Some(j) = slots.get(&slot).cloned() else { continue };
        let Some(p) = at(&mut s.graph, &j) else { continue };
        let ctrl = control(s, &format!("{cname}_Ctrl_{slot}"), p, &parent_ctrl)?;
        if slot == "Hips" {
            s.run("parentConstraint", json!({"args": [ctrl.clone(), j], "maintainOffset": true}))?;
        } else {
            s.run("orientConstraint", json!({"args": [ctrl.clone(), j], "maintainOffset": true}))?;
        }
        made.push(ctrl.clone());
        parent_ctrl = ctrl;
    }
    let hips_ctrl = format!("{cname}_Ctrl_Hips");
    // IK hands and feet, with pole vectors at the elbows and knees.
    for side in ["Left", "Right"] {
        for (root, mid, end, eff, pole_dir) in [("Arm", "ForeArm", "Hand", "HandEffector", DVec3::NEG_Z), ("UpLeg", "Leg", "Foot", "AnkleEffector", DVec3::Z)] {
            let (Some(r), Some(m), Some(e)) = (slots.get(&format!("{side}{root}")), slots.get(&format!("{side}{mid}")), slots.get(&format!("{side}{end}"))) else { continue };
            let (r, m, e) = (r.clone(), m.clone(), e.clone());
            let (Some(pe), Some(pm)) = (at(&mut s.graph, &e), at(&mut s.graph, &m)) else { continue };
            let ctrl = control(s, &format!("{cname}_Ctrl_{side}{eff}"), pe, &grp)?;
            let pole = control(s, &format!("{cname}_Ctrl_{side}{mid}Pole"), pm + pole_dir * 30.0, &grp)?;
            let h = s.run("ikHandle", json!({"startJoint": r, "endEffector": e, "solver": "ikRPsolver", "name": format!("{cname}_{side}{end}IK")}))?;
            let hn = h.get(0).and_then(Value::as_str).unwrap_or("").to_string();
            s.run("parent", json!({"args": [hn.clone(), ctrl.clone()]}))?;
            s.run("poleVectorConstraint", json!({"args": [pole.clone(), hn]}))?;
            // A hand follows the ribs, a foot stays on the ground: only the pole of an arm goes with the hips.
            if root == "Arm" {
                s.run("parent", json!({"args": [pole.clone(), hips_ctrl.clone()]}))?;
            }
            made.push(ctrl);
            made.push(pole);
        }
    }
    s.run("select", json!({"args": [grp]}))?;
    Ok(json!(made))
}

fn constraints_on(g: &Graph, joint: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    for (d, src) in g.connections() {
        if d.node == joint && crate::rig::CONSTRAINT_TYPES.contains(&g.node(src.node).map(|n| n.ty().name.as_str()).unwrap_or("")) && !out.contains(&src.node) {
            out.push(src.node);
        }
    }
    out
}

pub fn register(c: &mut Commands<Session>) {
    register_types();
    c.register(
        Cmd::new(
            "hikCreateSkeleton",
            "Make a standard humanoid skeleton in T-pose, joints named Character1_Hips and so on: -n the name, -h height, -sc spine joints, \
             -fg fingers, -ch a character definition with it. Returns the joints.",
            make_skeleton,
        )
        .flag(f("name", "n", ArgType::String, "Name, and the joints' prefix."))
        .flag(f("height", "h", ArgType::Float, "Height (170)."))
        .flag(f("spineCount", "sc", ArgType::Int, "Spine joints (3)."))
        .flag(f("fingers", "fg", ArgType::Bool, "Fingers too."))
        .flag(f("character", "ch", ArgType::Bool, "Make the character definition too.")),
    );
    c.register(
        Cmd::new(
            "hikCreateCharacter",
            "Define a character. Joints named (or selected, with everything below them) go to slots by their names; `setCharacterObject` \
             sets others. Returns the character.",
            |s, a| {
                let names = aztek_ops::objects(s, a);
                let (cname, joints) = match names.first() {
                    Some(n) if s.graph.find(n)?.is_none() => (n.clone(), names[1..].to_vec()),
                    _ => (a.string("name", "Character1"), names.clone()),
                };
                let g = &mut s.graph;
                let t = types::find(TYPE).unwrap();
                let c = g.create_node(&t, &cname, NO_NODE, true)?;
                let mut slots = BTreeMap::new();
                for n in joints {
                    let Some(id) = g.find(&n)? else { continue };
                    let mut all = vec![id];
                    all.extend(g.descendants(id));
                    for j in all {
                        if is_joint(g, j) {
                            let path = g.path(j);
                            if let Some(slot) = slot_for_name(&path) {
                                slots.entry(slot).or_insert(path);
                            }
                        }
                    }
                }
                set_slots(g, c, &slots)?;
                Ok(json!(g.path(c)))
            },
        )
        .positional("the new character's name, then joints (the selection when none)")
        .flag(f("name", "n", ArgType::String, "The character's name.")),
    );
    c.register(
        Cmd::new(
            "setCharacterObject",
            "Put a joint in a character's slot, as `setCharacterObject(joint, character, slot, 0)`; the slot by id or name.",
            |s, a| {
                let (Some(j), Some(cn), Some(slot)) = (a.pos_str(0).map(str::to_string), a.pos_str(1).map(str::to_string), slot_arg(a, 2)) else { fail!("setCharacterObject: joint, character, slot, 0") };
                let g = &mut s.graph;
                let c = character(g, &cn, "setCharacterObject")?;
                if g.find(&j)?.is_none() {
                    fail!("setCharacterObject: no object named {j}");
                }
                let mut m = slots_of(g, c);
                m.insert(slot, j);
                set_slots(g, c, &m)?;
                Ok(Value::Null)
            },
        )
        .positional("joint, character, slot, 0"),
    );
    c.register(
        Cmd::new(
            "hikGetSkNode",
            "The joint in a character's slot, by id or name; with only the character, every slot and joint in turn.",
            |s, a| {
                let Some(cn) = a.pos_str(0) else { fail!("hikGetSkNode: name a character") };
                let c = character(&s.graph, cn, "hikGetSkNode")?;
                let m = slots_of(&s.graph, c);
                match slot_arg(a, 1) {
                    Some(slot) => Ok(json!(m.get(&slot).cloned().unwrap_or_default())),
                    None => Ok(json!(m.into_iter().flat_map(|(k, v)| [k, v]).collect::<Vec<_>>())),
                }
            },
        )
        .undoable(false)
        .positional("character, slot"),
    );
    c.register(
        Cmd::new("hikCreateControlRig", "Build a control rig on a character: FK hips, spine, neck and head, IK hands and feet with pole vectors. Returns the controls.", |s, a| {
            let g = &mut s.graph;
            g.open_chunk("Control Rig");
            let r = create_control_rig(s, a);
            s.graph.close_chunk();
            r
        })
        .positional("the character"),
    );
    c.register(
        Cmd::new("hikDeleteControlRig", "Remove a character's control rig; the joints stay where they are.", |s, a| {
            let Some(cn) = a.pos_str(0).map(str::to_string) else { fail!("hikDeleteControlRig: name a character") };
            let c = character(&s.graph, &cn, "hikDeleteControlRig")?;
            let joints: Vec<String> = slots_of(&s.graph, c).into_values().collect();
            let mut doomed = Vec::new();
            for j in &joints {
                if let Some(id) = s.graph.find(j)? {
                    doomed.extend(constraints_on(&s.graph, id));
                }
            }
            for d in doomed {
                if s.graph.node(d).is_some() {
                    let p = s.graph.path(d);
                    s.run("delete", json!({"args": [p]}))?;
                }
            }
            let grp = format!("{cn}_Ctrl_Reference");
            if s.graph.find(&grp)?.is_some() {
                s.run("delete", json!({"args": [grp]}))?;
            }
            Ok(Value::Null)
        })
        .positional("the character"),
    );
    c.register(
        Cmd::new(
            "hikSetSource",
            "Drive one character from another, both in matching poses now: hikSetSource target source. Each slot both have follows in \
             rotation, relative to the pose they're in; the hips follow in place too, unscaled. With no source, stops.",
            |s, a| {
                let Some(tn) = a.pos_str(0).map(str::to_string) else { fail!("hikSetSource: target, source") };
                let target = character(&s.graph, &tn, "hikSetSource")?;
                let tm = slots_of(&s.graph, target);
                // What was there goes first.
                for j in tm.values() {
                    if let Some(id) = s.graph.find(j)? {
                        for c in constraints_on(&s.graph, id) {
                            if s.graph.node(c).is_some() {
                                let p = s.graph.path(c);
                                s.run("delete", json!({"args": [p]}))?;
                            }
                        }
                    }
                }
                let Some(sn) = a.pos_str(1).map(str::to_string) else { return Ok(Value::Null) };
                let source = character(&s.graph, &sn, "hikSetSource")?;
                let sm = slots_of(&s.graph, source);
                let mut n = 0;
                for (slot, tj) in &tm {
                    let Some(sj) = sm.get(slot) else { continue };
                    s.run("orientConstraint", json!({"args": [sj, tj], "maintainOffset": true}))?;
                    n += 1;
                    if slot == "Hips" {
                        s.run("pointConstraint", json!({"args": [sj, tj], "maintainOffset": true}))?;
                    }
                }
                Ok(json!(n))
            },
        )
        .positional("target, source"),
    );
    c.register(
        Cmd::new(
            "hikBake",
            "Bake a character's joints to keys over -t (the playback range by default), leaving them free of what drove them.",
            |s, a| {
                let Some(cn) = a.pos_str(0).map(str::to_string) else { fail!("hikBake: name a character") };
                let c = character(&s.graph, &cn, "hikBake")?;
                let joints: Vec<String> = slots_of(&s.graph, c).into_values().collect();
                let mut plugs = Vec::new();
                for j in &joints {
                    for at_name in ["rotateX", "rotateY", "rotateZ"] {
                        plugs.push(json!(format!("{j}.{at_name}")));
                    }
                }
                if let Some(h) = slots_of(&s.graph, c).get("Hips") {
                    for at_name in ["translateX", "translateY", "translateZ"] {
                        plugs.push(json!(format!("{h}.{at_name}")));
                    }
                }
                let mut call = json!({"args": plugs});
                if let Some(t) = a.get("time") {
                    call["time"] = t.clone();
                }
                let r = s.run("bakeResults", call)?;
                let doomed: Vec<NodeId> = joints.iter().filter_map(|j| s.graph.find(j).ok().flatten()).flat_map(|id| constraints_on(&s.graph, id)).collect();
                for d in doomed {
                    if s.graph.node(d).is_some() {
                        let p = s.graph.path(d);
                        s.run("delete", json!({"args": [p]}))?;
                    }
                }
                Ok(r)
            },
        )
        .positional("the character")
        .flag(f("time", "t", ArgType::Any, "The range: \"1:100\".")),
    );
}
