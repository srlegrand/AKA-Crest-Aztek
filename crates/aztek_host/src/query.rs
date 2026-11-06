//! Scene snapshots for the panels, as JSON: outliner, channel box, attribute editor, layers, state.

use aztek_scene::plug::PlugRef;
use aztek_scene::components::{self, ComponentMode};
use aztek_scene::graph::Graph;
use aztek_scene::plug::{flags, AttrDef, NodeId, PlugType, PlugValue, NO_NODE};
use aztek_scene::transform::DagNode;
use serde_json::{json, Value};

use crate::core::Core;

/// camelCase to "Camel Case", as Maya's nice names.
pub fn nice(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if i == 0 {
            out.extend(ch.to_uppercase());
        } else if ch.is_uppercase() && !out.ends_with(' ') {
            out.push(' ');
            out.push(ch);
        } else {
            out.push(ch);
        }
    }
    out
}

fn attr_nice(a: &AttrDef) -> String {
    if !a.nice_name.is_empty() && a.nice_name != "json" {
        a.nice_name.clone()
    } else {
        nice(&a.name)
    }
}

fn value_json(v: &PlugValue) -> Value {
    match v {
        PlugValue::Bool(b) => json!(b),
        PlugValue::Int(i) => json!(i),
        PlugValue::Float(f) => json!(f),
        PlugValue::Vec3(v) => json!([v.x, v.y, v.z]),
        PlugValue::String(s) => json!(s),
        PlugValue::Matrix(m) => json!(m.to_cols_array()),
        _ => Value::Null,
    }
}

fn type_name(t: PlugType) -> &'static str {
    match t {
        PlugType::Bool => "bool",
        PlugType::Int => "int",
        PlugType::Float => "float",
        PlugType::Vec3 => "vec3",
        PlugType::Matrix => "matrix",
        PlugType::String => "string",
        PlugType::Enum => "enum",
        PlugType::Mesh => "mesh",
        PlugType::Data => "data",
    }
}

fn is_shape(g: &Graph, id: NodeId) -> bool {
    g.node(id).is_some_and(|n| n.ty().is_a("shape"))
}

fn outliner_node(g: &Graph, id: NodeId, shapes: bool, sel: &[NodeId]) -> Option<Value> {
    let n = g.node(id)?;
    let shape = n.ty().is_a("shape");
    let intermediate = shape && n.bool(DagNode::INTERMEDIATE_OBJECT);
    if shape && !shapes {
        return None;
    }
    let kids: Vec<Value> = g.children(id).iter().filter_map(|&c| outliner_node(g, c, shapes, sel)).collect();
    // What the icon shows: a transform over a mesh is a mesh to the user.
    let icon = if n.ty().is_a("joint") {
        "joint".into()
    } else if n.ty().is_a("ikHandle") {
        "ik".into()
    } else if n.ty().is_a("deformHandle") {
        "deformHandle".into()
    } else if n.ty().is_a("transform") {
        match aztek_scene::dag::shapes(g, id, false).first().and_then(|&s| g.node(s)) {
            Some(s) => s.ty().name.clone(),
            None if g.children(id).is_empty() => "transform".into(),
            None => "group".into(),
        }
    } else {
        n.ty().name.clone()
    };
    Some(json!({
        "name": n.name(),
        "path": g.path(id),
        "type": n.ty().name,
        "icon": icon,
        "selected": sel.contains(&id),
        "visible": !n.ty().dag || n.bool(DagNode::VISIBILITY),
        "intermediate": intermediate,
        "children": kids,
    }))
}

pub fn outliner(core: &Core, shapes: bool) -> Value {
    let g = &core.s.graph;
    let sel = g.selection().to_vec();
    let mut out = Vec::new();
    for &r in g.roots() {
        if g.node(r).is_some_and(|n| n.ty().dag) {
            if let Some(v) = outliner_node(g, r, shapes, &sel) {
                out.push(v);
            }
        }
    }
    // Sets and layers at the end, as Maya lists its default sets.
    for id in g.nodes() {
        let Some(n) = g.node(id) else { continue };
        if n.ty().is_a("objectSet") {
            out.push(json!({"name": n.name(), "path": g.path(id), "type": n.ty().name, "icon": "set", "selected": sel.contains(&id), "visible": true, "intermediate": false, "children": []}));
        }
    }
    Value::Array(out)
}

/// How the channel box colours a channel: keyed, keyed on this frame, driven by another connection, locked, or "".
fn channel_state(g: &Graph, p: &PlugRef) -> &'static str {
    if g.source(p).is_some_and(|x| aztek_scene::anim_filter::is_mute(g, x.node)) {
        return "muted";
    }
    if let Some(c) = aztek_scene::anim::curve_of(g, p) {
        let now = aztek_scene::anim::frame(g);
        if g.is_held(p) {
            return "changed";
        }
        return if aztek_scene::anim::keys_of(g, c).keys.iter().any(|k| (k.t - now).abs() < 1e-6) { "keyOnFrame" } else { "keyed" };
    }
    if g.source(p).is_some() || (p.component >= 0 && g.source(&p.whole()).is_some()) {
        return "connected";
    }
    if g.is_locked(p) {
        return "locked";
    }
    ""
}

/// The animation curves on the selected objects, for the graph editor: keys with their actual slopes, and the curve
/// sampled over [start, end].
pub fn anim_curves(core: &Core, arg: &str) -> Value {
    use aztek_scene::anim;
    let g = &core.s.graph;
    let mut it = arg.split(':').filter_map(|x| x.parse::<f64>().ok());
    let (start, end, step) = (it.next().unwrap_or(0.0), it.next().unwrap_or(100.0), it.next().unwrap_or(1.0).max(1e-3));
    let mut out = Vec::new();
    let mut nodes: Vec<NodeId> = Vec::new();
    for &id in g.selection() {
        let id = if g.node(id).is_some_and(|n| n.ty().dag && !n.ty().is_a("transform")) { aztek_scene::dag::transform_of(g, id) } else { id };
        if !nodes.contains(&id) {
            nodes.push(id);
        }
    }
    for id in nodes {
        let mut curves = Vec::new();
        for (p, c) in anim::curves_on(g, id) {
            let k = anim::keys_of(g, c);
            let (pre, post) = (g.node(c).map(|n| n.data().int(anim::PRE)).unwrap_or(0), g.node(c).map(|n| n.data().int(anim::POST)).unwrap_or(0));
            let inf = |i: i64| match i {
                1 => anim::Infinity::Linear,
                3 => anim::Infinity::Cycle,
                4 => anim::Infinity::CycleRelative,
                5 => anim::Infinity::Oscillate,
                _ => anim::Infinity::Constant,
            };
            let keys: Vec<Value> = (0..k.keys.len())
                .map(|i| {
                    let (si, so) = k.slopes(i);
                    let key = &k.keys[i];
                    // The handles, in frames and value: a third of the tangents.
                    let f = anim::fps() / 3.0;
                    let (ix, iy) = k.tangent_xy(i, false);
                    let (ox, oy) = k.tangent_xy(i, true);
                    json!([key.t, key.v, si, so, key.itt.name(), key.ott.name(), key.breakdown, ix * f, iy / 3.0, ox * f, oy / 3.0, key.tan_lock, key.weight_lock])
                })
                .collect();
            let mut samples = Vec::new();
            let mut t = start;
            while t <= end + 1e-9 {
                samples.push(json!([t, k.eval(t, inf(pre), inf(post))]));
                t += step;
            }
            let a = g.attr(&p);
            let channel = if p.component >= 0 { a.child_names[p.component as usize].clone() } else { a.name.clone() };
            // What the buffer holds, sampled the same way.
            let mut buffer = Vec::new();
            if let Some(b) = core.s.buffer_curves.get(&c) {
                let mut held = b.clone();
                held.weighted = k.weighted;
                let mut t = start;
                while !held.keys.is_empty() && t <= end + 1e-9 {
                    buffer.push(json!([t, held.eval(t, inf(pre), inf(post))]));
                    t += step;
                }
            }
            curves.push(json!({"curve": g.path(c), "channel": channel, "plug": g.plug_name(&p), "component": p.component, "keys": keys, "samples": samples, "weighted": k.weighted, "buffer": buffer}));
        }
        if !curves.is_empty() {
            out.push(json!({"node": g.path(id), "curves": curves}));
        }
    }
    json!({"time": anim::frame(g), "objects": out})
}

/// Channels of a node as the channel box lists them: keyable scalar attributes, vectors split per axis.
fn channels(g: &Graph, id: NodeId, all_inputs: bool) -> Vec<Value> {
    let Some(n) = g.node(id) else { return Vec::new() };
    let path = g.path(id);
    let ty = n.ty();
    let mut out = Vec::new();
    for (i, a) in ty.attrs.iter().enumerate() {
        if a.flags & (flags::HIDDEN | flags::OUTPUT) != 0 {
            continue;
        }
        // Keyable as this node has it: setAttr -k hides and shows channels.
        let whole = PlugRef::new(id, i as i32);
        let vector = a.ty == PlugType::Vec3;
        let keyable = !vector && aztek_scene::anim::is_keyable(g, &whole);
        if !vector && !keyable && !(all_inputs && matches!(a.ty, PlugType::Bool | PlugType::Int | PlugType::Float | PlugType::Enum | PlugType::String)) {
            continue;
        }
        let v = n.value(i);
        match (a.ty, v) {
            (PlugType::Vec3, PlugValue::Vec3(v)) => {
                for k in 0..3 {
                    if !aztek_scene::anim::is_keyable(g, &PlugRef::component(id, i as i32, k as i32)) {
                        continue;
                    }
                    let child = if a.child_names[k].is_empty() { format!("{}{}", a.name, ["X", "Y", "Z"][k]) } else { a.child_names[k].clone() };
                    out.push(json!({
                        "attr": child,
                        "plug": format!("{path}.{child}"),
                        "nice": format!("{} {}", attr_nice(a), ["X", "Y", "Z"][k]),
                        "type": "float",
                        "value": v[k],
                        "connected": g.source(&aztek_scene::plug::PlugRef::new(id, i as i32)).is_some(),
                        "state": channel_state(g, &PlugRef::component(id, i as i32, k as i32)),
                    }));
                }
            }
            (t, v) if matches!(t, PlugType::Bool | PlugType::Int | PlugType::Float | PlugType::Enum | PlugType::String) => {
                out.push(json!({
                    "attr": a.name,
                    "plug": format!("{path}.{}", a.name),
                    "nice": attr_nice(a),
                    "type": type_name(t),
                    "value": value_json(v),
                    "enum": a.enum_names,
                    "connected": g.source(&aztek_scene::plug::PlugRef::new(id, i as i32)).is_some(),
                    "state": channel_state(g, &PlugRef::new(id, i as i32)),
                }));
            }
            _ => {}
        }
    }
    out
}

pub fn channel_box(core: &Core) -> Value {
    let g = &core.s.graph;
    let Some(&lead) = g.selection().last() else { return Value::Null };
    let Some(n) = g.node(lead) else { return Value::Null };
    let is_xf = n.ty().is_a("transform");
    let shapes: Vec<NodeId> = if is_xf { aztek_scene::dag::shapes(g, lead, false) } else if is_shape(g, lead) { vec![lead] } else { vec![] };
    let mut inputs = Vec::new();
    for &sh in &shapes {
        if g.node(sh).is_some_and(|n| n.ty().is_a("mesh")) {
            for h in aztek_scene::history::history_of(g, sh) {
                let Some(hn) = g.node(h) else { continue };
                if hn.ty().dag {
                    continue;
                }
                inputs.push(json!({"name": hn.name(), "path": g.path(h), "type": hn.ty().name, "channels": channels(g, h, true)}));
            }
        }
    }
    let shape_list: Vec<Value> = shapes
        .iter()
        .filter_map(|&s| g.node(s).map(|n| json!({"name": n.name(), "path": g.path(s), "type": n.ty().name, "channels": channels(g, s, false)})))
        .collect();
    json!({
        "name": n.name(),
        "path": g.path(lead),
        "type": n.ty().name,
        "channels": if is_xf || !is_shape(g, lead) { channels(g, lead, !n.ty().dag) } else { Vec::new() },
        "shapes": shape_list,
        "inputs": inputs,
        "count": g.selection().len(),
    })
}

/// Every attribute of a node, for the attribute editor.
fn node_attrs(g: &Graph, id: NodeId) -> Value {
    let Some(n) = g.node(id) else { return Value::Null };
    let path = g.path(id);
    let mut attrs = Vec::new();
    for (i, a) in n.ty().attrs.iter().enumerate() {
        if a.flags & flags::HIDDEN != 0 || matches!(a.ty, PlugType::Mesh | PlugType::Data) {
            continue;
        }
        attrs.push(json!({
            "attr": a.name,
            "plug": format!("{path}.{}", a.name),
            "nice": attr_nice(a),
            "type": type_name(a.ty),
            "value": value_json(n.value(i)),
            "enum": a.enum_names,
            "children": a.child_names,
            "output": a.flags & flags::OUTPUT != 0,
            "keyable": a.flags & flags::KEYABLE != 0,
            "category": a.category,
            "doc": a.doc,
            "min": a.soft_min.or(a.min_value),
            "max": a.soft_max.or(a.max_value),
            "connected": g.source(&aztek_scene::plug::PlugRef::new(id, i as i32)).is_some(),
        }));
    }
    json!({"name": n.name(), "path": path, "type": n.ty().name, "doc": n.ty().doc, "attrs": attrs})
}

/// The attribute editor's tabs for a node: the transform, its shape, and the shape's history, as Maya shows them.
pub fn attribute_editor(core: &Core, path: &str) -> Value {
    let g = &core.s.graph;
    let id = if path.is_empty() {
        match g.selection().last() {
            Some(&id) => id,
            None => return Value::Null,
        }
    } else {
        match g.find(path) {
            Ok(Some(id)) => id,
            _ => return Value::Null,
        }
    };
    let mut tabs = vec![id];
    let Some(n) = g.node(id) else { return Value::Null };
    if n.ty().is_a("transform") {
        for s in aztek_scene::dag::shapes(g, id, false) {
            tabs.push(s);
            if g.node(s).is_some_and(|n| n.ty().is_a("mesh")) {
                tabs.extend(aztek_scene::history::history_of(g, s).into_iter().filter(|&h| g.node(h).is_some_and(|n| !n.ty().dag)));
            }
        }
    } else if is_shape(g, id) {
        let xf = aztek_scene::dag::transform_of(g, id);
        if xf != id && xf != NO_NODE {
            tabs.insert(0, xf);
        }
    }
    json!({"focus": g.path(id), "tabs": tabs.iter().map(|&t| node_attrs(g, t)).collect::<Vec<_>>()})
}

pub fn layers(core: &Core) -> Value {
    let g = &core.s.graph;
    let mut out = Vec::new();
    for id in g.nodes() {
        let Some(n) = g.node(id) else { continue };
        if n.ty().name != aztek_scene::display_layers::TYPE_NAME || n.name() == aztek_scene::display_layers::DEFAULT_LAYER {
            continue;
        }
        let members: Vec<String> = aztek_scene::display_layers::members(g, id).iter().map(|&m| g.path(m)).collect();
        out.push(json!({
            "name": n.name(),
            "visible": n.bool(aztek_scene::display_layers::VISIBILITY),
            "displayType": n.int(aztek_scene::display_layers::DISPLAY_TYPE),
            "order": n.int(aztek_scene::display_layers::DISPLAY_ORDER),
            "members": members,
        }));
    }
    out.sort_by_key(|l| -l["order"].as_i64().unwrap_or(0));
    Value::Array(out)
}

pub fn state(core: &Core) -> Value {
    let g = &core.s.graph;
    let mode = match components::mode(g) {
        ComponentMode::Object => "object",
        ComponentMode::Vertex => "vertex",
        ComponentMode::Edge => "edge",
        ComponentMode::Face => "face",
    };
    let sel = components::component_strings(g, false);
    let sel = if sel.is_empty() { g.selection().iter().map(|&id| g.path(id)).collect() } else { sel };
    json!({
        "mode": mode,
        "selection": sel,
        "history": g.construction_history,
        "canUndo": g.undo_stack().can_undo(),
        "canRedo": g.undo_stack().can_redo(),
        "file": core.current_file,
        "version": core.version,
    })
}

pub fn commands(core: &Core) -> Value {
    let mut v: Vec<Value> = core
        .s
        .commands
        .all()
        .map(|c| json!({"name": c.name, "doc": c.doc, "flags": c.flags.iter().map(|f| json!({"name": f.name, "short": f.short_name, "doc": f.doc})).collect::<Vec<_>>()}))
        .collect();
    v.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Value::Array(v)
}

/// The UV editor's view of the selected and hilited meshes: maps, faces, borders, and the components selected.
pub fn uv_data(core: &mut Core) -> Value {
    let g = &mut core.s.graph;
    let mut meshes: Vec<NodeId> = Vec::new();
    for id in g.selection().to_vec() {
        if let Some(m) = components::as_mesh(g, id) {
            if !meshes.contains(&m) {
                meshes.push(m);
            }
        }
    }
    let mode = components::mode(g);
    for kind in [ComponentMode::Vertex, ComponentMode::Edge, ComponentMode::Face] {
        for (&node, _) in components::selected_all(g, kind).iter() {
            if let Some(m) = components::as_mesh(g, node) {
                if !meshes.contains(&m) {
                    meshes.push(m);
                }
            }
        }
    }
    let mut out = Vec::new();
    for m in meshes {
        let Some(mut d) = aztek_scene::uv_ops::editor_data(g, m) else { continue };
        let sel = |kind| -> Vec<u32> {
            let all = components::selected_all(g, kind);
            all.iter().filter(|(n, _)| components::as_mesh(g, **n) == Some(m)).flat_map(|(_, ids)| ids.iter().copied()).collect()
        };
        d["selFaces"] = json!(sel(ComponentMode::Face));
        d["selEdges"] = json!(sel(ComponentMode::Edge));
        d["selPoints"] = json!(sel(ComponentMode::Vertex));
        out.push(d);
    }
    let mode = match mode {
        ComponentMode::Object => "object",
        ComponentMode::Vertex => "vertex",
        ComponentMode::Edge => "edge",
        ComponentMode::Face => "face",
    };
    json!({"mode": mode, "meshes": out})
}
