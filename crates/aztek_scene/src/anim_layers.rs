//! Animation layers as Maya builds them: an `animLayer` node per layer with BaseAnimation as the root, and for each
//! channel in a layer a blend node (`animBlendNodeAdditiveDL`, `...Rotation`, `...Scale`, `animBlendNodeBoolean`, ...) that
//! adds that layer's curve to everything below it. The blends of a channel are a chain: the output of one is `inputA`
//! of the next, the base curve at the bottom, the channel at the top.

use std::any::Any;
use std::sync::Arc;

use aztek_ops::{fail, ArgType, Args, CmdResult, Commands};
use glam::{DMat3, DQuat, DVec3};
use serde_json::{json, Value};

use crate::anim::{self, f, query_flags, queried, Cmd, Key, Keys, OUTPUT, POST, PRE};
use crate::graph::Graph;
use crate::node::{types, vec3_attr, EvalContext, NodeBehavior, NodeData, NodeType};
use crate::plug::{flags, AttrDef, NodeId, PlugRef, PlugType, PlugValue, NO_NODE};
use crate::session::Session;
use crate::transform::{euler_near, euler_to_matrix, RotateOrder, Transform};

pub const BASE_LAYER: &str = "BaseAnimation";

// Blend node attrs.
pub const IN_A: usize = 0;
pub const IN_B: usize = 1;
pub const WEIGHT_A: usize = 2;
pub const WEIGHT_B: usize = 3;
pub const BLEND_OUT: usize = 4;
pub const ACCUMULATE: usize = 5;
pub const BLEND_ROT_ORDER: usize = 6;

// Layer attrs.
const L_MUTE: usize = 0;
const L_LOCK: usize = 1;
const L_SOLO: usize = 2;
const L_OVERRIDE: usize = 3;
const L_WEIGHT: usize = 4;
const L_FG: usize = 5;
const L_BG: usize = 6;
const L_PARENT: usize = 7;
const L_RAM: usize = 8;
const L_SAM: usize = 9;
const L_SELECTED: usize = 10;
const L_PREFERRED: usize = 11;
const L_SOLO_MUTED: usize = 12;
const L_PASSTHROUGH: usize = 13;
const L_ORDER: usize = 14;

#[derive(Clone, Copy, PartialEq)]
enum BlendKind {
    Add,
    Scale,
    Switch,
    Rotation,
}

struct BlendNode {
    kind: BlendKind,
}

/// One blend's output from its inputs.
fn blend_scalar(kind: BlendKind, a: f64, b: f64, wa: f64, wb: f64, accumulate: i64) -> f64 {
    match kind {
        // Added: a layer value of 1 adds nothing; an override layer (wa < 1) fades to its own value.
        BlendKind::Scale if accumulate == 0 => {
            if wa < 1.0 {
                a * wa + b * wb
            } else {
                a + (b - 1.0) * wb
            }
        }
        BlendKind::Scale => {
            // Multiplied: an override layer (wa < 1) fades from the layers below to its own value.
            let base = a.abs().powf(wa) * if a < 0.0 && wa > 0.0 { -1.0 } else { 1.0 };
            base * b.abs().powf(wb) * if b < 0.0 && wb > 0.0 { -1.0 } else { 1.0 }
        }
        BlendKind::Switch => {
            if wb >= 0.5 {
                b
            } else {
                a
            }
        }
        _ => a * wa + b * wb,
    }
}

impl NodeBehavior for BlendNode {
    fn compute(&mut self, _ctx: &EvalContext, d: &mut NodeData) {
        let (wa, wb) = (d.float(WEIGHT_A), d.float(WEIGHT_B));
        let acc = d.int(ACCUMULATE);
        if self.kind == BlendKind::Rotation {
            let (a, b) = (d.vec3(IN_A), d.vec3(IN_B));
            let componentwise = a * wa + b * wb;
            let out = if acc == 1 {
                // By layer: rotations compose as quaternions.
                let order = RotateOrder::from_index(d.int(BLEND_ROT_ORDER));
                let (qa, qb) = (DQuat::from_mat4(&euler_to_matrix(a, order)), DQuat::from_mat4(&euler_to_matrix(b, order)));
                let q = if wa >= 1.0 { qa * DQuat::IDENTITY.slerp(qb, wb) } else { qa.slerp(qb, wb) };
                euler_near(&DMat3::from_quat(q.normalize()), order, componentwise)
            } else {
                componentwise
            };
            d.set_output(BLEND_OUT, PlugValue::Vec3(out));
        } else {
            d.set_output(BLEND_OUT, PlugValue::Float(blend_scalar(self.kind, d.float(IN_A), d.float(IN_B), wa, wb, acc)));
        }
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct LayerNode;

impl NodeBehavior for LayerNode {
    fn compute(&mut self, _ctx: &EvalContext, d: &mut NodeData) {
        let off = d.bool(L_MUTE) || d.bool(L_SOLO_MUTED);
        let fg = if off { 0.0 } else { d.float(L_WEIGHT) * d.float(L_PARENT) };
        // An override layer fades out what is under it.
        let bg = if d.bool(L_OVERRIDE) { 1.0 - fg } else { 1.0 };
        d.set_output(L_FG, PlugValue::Float(fg));
        d.set_output(L_BG, PlugValue::Float(bg));
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn float_in(name: &str, short: &str, def: f64, keyable: bool, doc: &str) -> AttrDef {
    AttrDef {
        name: name.into(),
        short_name: short.into(),
        ty: PlugType::Float,
        default_value: PlugValue::Float(def),
        flags: flags::INPUT | flags::STORABLE | if keyable { flags::KEYABLE } else { 0 },
        doc: doc.into(),
        ..Default::default()
    }
}

fn bool_in(name: &str, short: &str, def: bool, doc: &str) -> AttrDef {
    AttrDef {
        name: name.into(),
        short_name: short.into(),
        ty: PlugType::Bool,
        default_value: PlugValue::Bool(def),
        flags: flags::INPUT | flags::STORABLE,
        doc: doc.into(),
        ..Default::default()
    }
}

fn enum_in(name: &str, short: &str, names: &[&str], def: i64, doc: &str) -> AttrDef {
    AttrDef {
        name: name.into(),
        short_name: short.into(),
        ty: PlugType::Enum,
        default_value: PlugValue::Int(def),
        flags: flags::INPUT | flags::STORABLE,
        enum_names: names.iter().map(|s| s.to_string()).collect(),
        doc: doc.into(),
        ..Default::default()
    }
}

fn float_out(name: &str, short: &str, doc: &str) -> AttrDef {
    AttrDef {
        name: name.into(),
        short_name: short.into(),
        ty: PlugType::Float,
        default_value: PlugValue::Float(0.0),
        flags: flags::OUTPUT,
        doc: doc.into(),
        ..Default::default()
    }
}

pub fn register_types() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let blend = |name: &str, kind: BlendKind, def_b: f64, doc: &str| {
            let attrs = if kind == BlendKind::Rotation {
                vec![
                    vec3_attr("inputA", "ia", DVec3::ZERO, flags::INPUT | flags::STORABLE, "What the layers below come to."),
                    vec3_attr("inputB", "ib", DVec3::ZERO, flags::INPUT | flags::STORABLE, "This layer's value."),
                    float_in("weightA", "wa", 1.0, false, "Weight of the layers below."),
                    float_in("weightB", "wb", 1.0, false, "Weight of this layer."),
                    vec3_attr("output", "o", DVec3::ZERO, flags::OUTPUT, "The result."),
                    enum_in("accumulationMode", "am", &["component", "layer"], 0, "Add Euler values, or compose rotations."),
                    AttrDef { name: "rotateOrder".into(), short_name: "ro".into(), ty: PlugType::Int, default_value: PlugValue::Int(0), flags: flags::INPUT | flags::STORABLE, doc: "The channel's rotate order.".into(), ..Default::default() },
                ]
            } else {
                vec![
                    float_in("inputA", "ia", 0.0, false, "What the layers below come to."),
                    float_in("inputB", "ib", def_b, false, "This layer's value."),
                    float_in("weightA", "wa", 1.0, false, "Weight of the layers below."),
                    float_in("weightB", "wb", 1.0, false, "Weight of this layer."),
                    float_out("output", "o", "The result."),
                    enum_in("accumulationMode", "am", &["additive", "multiply"], 1, "Scale layers: add or multiply."),
                ]
            };
            types::register_type(NodeType {
                name: name.into(),
                icon: "chart-line".into(),
                doc: doc.into(),
                own_attrs: attrs,
                create: Some(Arc::new(move || Box::new(BlendNode { kind }))),
                ..Default::default()
            })
            .expect(name);
        };
        for (name, doc) in [
            ("animBlendNodeAdditive", "Adds a layer to what is below, doubles."),
            ("animBlendNodeAdditiveDL", "Adds a layer to what is below, a linear channel."),
            ("animBlendNodeAdditiveDA", "Adds a layer to what is below, an angle channel."),
            ("animBlendNodeAdditiveF", "Adds a layer to what is below, floats."),
            ("animBlendNodeAdditiveFL", "Adds a layer to what is below, a linear float channel."),
            ("animBlendNodeAdditiveFA", "Adds a layer to what is below, an angle float channel."),
            ("animBlendNodeAdditiveI16", "Adds a layer to what is below, short integers."),
            ("animBlendNodeAdditiveI32", "Adds a layer to what is below, integers."),
        ] {
            blend(name, BlendKind::Add, 0.0, doc);
        }
        blend("animBlendNodeAdditiveScale", BlendKind::Scale, 1.0, "Multiplies a layer into what is below: a scale channel.");
        blend("animBlendNodeAdditiveRotation", BlendKind::Rotation, 0.0, "Adds a layer to what is below: a whole rotate.");
        blend("animBlendNodeBoolean", BlendKind::Switch, 0.0, "Switches to a layer's value at half weight: a boolean channel.");
        blend("animBlendNodeEnum", BlendKind::Switch, 0.0, "Switches to a layer's value at half weight: an enum channel.");

        types::register_type(NodeType {
            name: "animLayer".into(),
            icon: "chart-line".into(),
            doc: "One animation layer: its curves add to the layers below, or override them.".into(),
            own_attrs: vec![
                bool_in("mute", "m", false, "The layer contributes nothing."),
                bool_in("lock", "l", false, "The layer can't be keyed."),
                bool_in("solo", "s", false, "Only soloed layers and the base play."),
                bool_in("override", "o", false, "Replaces what is below instead of adding to it."),
                float_in("weight", "w", 1.0, true, "How much of the layer shows, 0 to 1."),
                float_out("foregroundWeight", "fgw", "Weight of the layer's blends: its own under its parent's, 0 when muted."),
                float_out("backgroundWeight", "bgw", "Weight of what is below it."),
                float_in("parentWeight", "pw", 1.0, false, "The parent layer's foreground weight."),
                enum_in("rotationAccumulationMode", "ram", &["component", "layer"], 0, "Rotations add by Euler value, or compose."),
                enum_in("scaleAccumulationMode", "sam", &["additive", "multiply"], 1, "Scales add, or multiply."),
                bool_in("selected", "sel", false, "The layer is selected: keys go to the topmost selected one."),
                bool_in("preferred", "prf", false, "Preferred when the choice of layer is ambiguous."),
                bool_in("soloMuted", "sm", false, "Off because another layer is soloed."),
                bool_in("passthrough", "pth", true, "Lets lower layers through."),
                AttrDef { name: "layerOrder".into(), short_name: "lo".into(), ty: PlugType::Int, default_value: PlugValue::Int(0), flags: flags::INPUT | flags::STORABLE | flags::HIDDEN, doc: "Place among its siblings.".into(), ..Default::default() },
            ],
            create: Some(Arc::new(|| Box::new(LayerNode))),
            ..Default::default()
        })
        .expect("animLayer");
    });
}

// ---------------------------------------------------------------------------------------------------------------------
// Queries over the graph

pub fn is_layer(g: &Graph, id: NodeId) -> bool {
    g.node(id).is_some_and(|n| n.ty().name == "animLayer")
}

pub fn is_blend(g: &Graph, id: NodeId) -> bool {
    g.node(id).is_some_and(|n| n.ty().name.starts_with("animBlendNode"))
}

/// Blend or mute: nodes a channel's animation passes through.
pub fn is_routing(g: &Graph, id: NodeId) -> bool {
    is_blend(g, id) || crate::anim_filter::is_mute(g, id)
}

fn is_rotation(g: &Graph, blend: NodeId) -> bool {
    g.node(blend).is_some_and(|n| n.ty().name == "animBlendNodeAdditiveRotation")
}

/// A mute's source, or the mute itself when nothing feeds it.
fn through_mutes(g: &Graph, mut s: PlugRef) -> Option<PlugRef> {
    while crate::anim_filter::is_mute(g, s.node) {
        s = g.source(&PlugRef::new(s.node, crate::anim_filter::MUTE_INPUT as i32))?;
    }
    Some(s)
}

/// What drives a plug, whole vector or component, looking through mute nodes. None when nothing does.
pub fn driver(g: &Graph, p: &PlugRef) -> Option<PlugRef> {
    let s = g.source(p).or_else(|| if p.component >= 0 { g.source(&p.whole()) } else { None })?;
    through_mutes(g, s)
}

/// Where a new curve for a plug connects: the input of its mute node when it has one, else the plug.
pub fn curve_slot(g: &Graph, p: &PlugRef) -> PlugRef {
    match g.source(p) {
        Some(s) if crate::anim_filter::is_mute(g, s.node) => PlugRef::new(s.node, crate::anim_filter::MUTE_INPUT as i32),
        _ => *p,
    }
}

/// The plugs a connection into a routing node ends up driving, or the plug itself.
pub fn final_plugs(g: &Graph, d: PlugRef) -> Vec<PlugRef> {
    if crate::anim_filter::is_mute(g, d.node) {
        return g.destinations(&PlugRef::new(d.node, crate::anim_filter::MUTE_OUTPUT as i32)).into_iter().flat_map(|x| final_plugs(g, x)).collect();
    }
    if is_blend(g, d.node) && (d.attr == IN_A as i32 || d.attr == IN_B as i32) {
        let mut out = Vec::new();
        for x in g.destinations(&PlugRef::new(d.node, BLEND_OUT as i32)) {
            for y in final_plugs(g, x) {
                // A rotation blend drives the whole vector; a curve on one axis drives that axis.
                if d.component >= 0 && y.component < 0 && g.attr(&y).ty == PlugType::Vec3 {
                    out.push(PlugRef::component(y.node, y.attr, d.component));
                } else {
                    out.push(y);
                }
            }
        }
        return out;
    }
    vec![d]
}

/// The blends on a plug, topmost first.
pub fn chain(g: &Graph, p: &PlugRef) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut s = driver(g, p);
    while let Some(x) = s {
        if !is_blend(g, x.node) || out.contains(&x.node) {
            break;
        }
        out.push(x.node);
        let below = if is_rotation(g, x.node) { PlugRef::new(x.node, IN_A as i32) } else { input(g, x.node, IN_A, p) };
        s = g.source(&below).and_then(|y| through_mutes(g, y));
    }
    out
}

fn input(g: &Graph, blend: NodeId, which: usize, p: &PlugRef) -> PlugRef {
    if is_rotation(g, blend) {
        PlugRef::component(blend, which as i32, p.component.max(0))
    } else {
        PlugRef::new(blend, which as i32)
    }
}

pub fn blend_layer(g: &Graph, blend: NodeId) -> Option<NodeId> {
    g.source(&PlugRef::new(blend, WEIGHT_B as i32)).map(|s| s.node).filter(|&l| is_layer(g, l))
}

pub fn base_layer(g: &Graph) -> Option<NodeId> {
    let id = g.find(BASE_LAYER).ok().flatten()?;
    is_layer(g, id).then_some(id)
}

fn layer_parent(g: &Graph, l: NodeId) -> Option<NodeId> {
    g.source(&PlugRef::new(l, L_PARENT as i32)).map(|s| s.node).filter(|&x| is_layer(g, x))
}

fn layer_children(g: &Graph, l: NodeId) -> Vec<NodeId> {
    let mut v: Vec<NodeId> = g.destinations(&PlugRef::new(l, L_FG as i32)).into_iter().filter(|d| is_layer(g, d.node) && d.attr == L_PARENT as i32).map(|d| d.node).collect();
    v.sort_by_key(|&c| g.node(c).map(|n| n.data().int(L_ORDER)).unwrap_or(0));
    v
}

/// All layers bottom to top: the base, then each layer followed by its children.
pub fn ordered_layers(g: &Graph) -> Vec<NodeId> {
    fn walk(g: &Graph, l: NodeId, out: &mut Vec<NodeId>) {
        out.push(l);
        for c in layer_children(g, l) {
            walk(g, c, out);
        }
    }
    let mut out = Vec::new();
    if let Some(b) = base_layer(g) {
        walk(g, b, &mut out);
    }
    out
}

fn layer_flag(g: &Graph, l: NodeId, attr: usize) -> bool {
    g.node(l).is_some_and(|n| n.data().bool(attr))
}

/// The layer keys on this plug go to: the topmost selected and unlocked one it's in. None is the base.
fn best_layer(g: &Graph, p: &PlugRef) -> Option<NodeId> {
    let c = chain(g, p);
    for &b in &c {
        if let Some(l) = blend_layer(g, b) {
            if layer_flag(g, l, L_SELECTED) && !layer_flag(g, l, L_LOCK) {
                return Some(l);
            }
        }
    }
    for &b in &c {
        if let Some(l) = blend_layer(g, b) {
            if layer_flag(g, l, L_PREFERRED) && !layer_flag(g, l, L_LOCK) {
                return Some(l);
            }
        }
    }
    None
}

/// The input of a chain holding a layer's value: its blend's inputB, or the bottom blend's inputA for the base.
fn slot(g: &Graph, p: &PlugRef, layer: Option<NodeId>) -> Option<PlugRef> {
    let c = chain(g, p);
    match layer {
        Some(l) if Some(l) != base_layer(g) => c.iter().find(|&&b| blend_layer(g, b) == Some(l)).map(|&b| input(g, b, IN_B, p)),
        _ => c.last().map(|&b| input(g, b, IN_A, p)),
    }
}

fn curve_in(g: &Graph, slot: &PlugRef) -> Option<NodeId> {
    let s = g.source(slot).and_then(|x| through_mutes(g, x))?;
    (anim::is_curve(g, s.node) && s.attr == OUTPUT as i32).then_some(s.node)
}

/// The curve keys on a layered plug are read from.
pub fn best_curve(g: &Graph, p: &PlugRef) -> Option<NodeId> {
    curve_in(g, &slot(g, p, best_layer(g, p))?)
}

/// A curve's value at a frame, whatever the current time: keys only.
fn curve_at(g: &Graph, c: NodeId, t: f64) -> f64 {
    let (pre, post) = g.node(c).map(|n| (n.data().int(PRE), n.data().int(POST))).unwrap_or((0, 0));
    anim::keys_of(g, c).eval(t, anim::infinity(pre), anim::infinity(post))
}

/// A blend input's value at a frame: a curve, a lower blend, or what is stored.
fn input_at(g: &Graph, plug: &PlugRef, p: &PlugRef, t: f64) -> f64 {
    if let Some(s) = g.source(plug).or_else(|| if plug.component >= 0 { g.source(&plug.whole()) } else { None }).and_then(|x| through_mutes(g, x)) {
        if anim::is_curve(g, s.node) && s.attr == OUTPUT as i32 {
            return curve_at(g, s.node, t);
        }
        if is_blend(g, s.node) {
            return blend_at(g, s.node, p, t);
        }
    }
    match g.stored_value(plug) {
        PlugValue::Vec3(v) => v[plug.component.max(0) as usize],
        v => v.scalar().unwrap_or(0.0),
    }
}

fn weights(g: &Graph, blend: NodeId) -> (f64, f64, i64, BlendKind) {
    let d = g.node(blend).map(|n| n.data());
    let (wa, wb, acc) = d.map(|d| (d.float(WEIGHT_A), d.float(WEIGHT_B), d.int(ACCUMULATE))).unwrap_or((1.0, 1.0, 0));
    (wa, wb, acc, blend_kind(g, blend))
}

fn blend_kind(g: &Graph, blend: NodeId) -> BlendKind {
    match g.node(blend).map(|n| n.ty().name.clone()).unwrap_or_default().as_str() {
        "animBlendNodeAdditiveRotation" => BlendKind::Rotation,
        "animBlendNodeAdditiveScale" => BlendKind::Scale,
        "animBlendNodeBoolean" | "animBlendNodeEnum" => BlendKind::Switch,
        _ => BlendKind::Add,
    }
}

/// What a blend outputs for this plug at a frame.
fn blend_at(g: &Graph, blend: NodeId, p: &PlugRef, t: f64) -> f64 {
    let (wa, wb, acc, kind) = weights(g, blend);
    let a = input_at(g, &input(g, blend, IN_A, p), p, t);
    let b = input_at(g, &input(g, blend, IN_B, p), p, t);
    blend_scalar(kind, a, b, wa, wb, acc)
}

/// The value a blend's input A needs for its output to be `want`, given input B.
fn solve_a(kind: BlendKind, want: f64, b: f64, wa: f64, wb: f64, acc: i64) -> f64 {
    match kind {
        BlendKind::Scale if acc != 0 => {
            let div = b.abs().powf(wb) * if b < 0.0 && wb > 0.0 { -1.0 } else { 1.0 };
            let v = if div.abs() > 1e-12 { want / div } else { want };
            if wa > 1e-9 {
                v.abs().powf(1.0 / wa) * v.signum()
            } else {
                want
            }
        }
        BlendKind::Switch => want,
        BlendKind::Scale if wa >= 1.0 => want - (b - 1.0) * wb,
        _ if wa.abs() > 1e-9 => (want - b * wb) / wa,
        _ => want,
    }
}

/// The value a blend's input B needs for its output to be `want`, given input A.
fn solve_b(kind: BlendKind, want: f64, a: f64, wa: f64, wb: f64, acc: i64) -> f64 {
    match kind {
        BlendKind::Scale if acc != 0 => {
            let base = a.abs().powf(wa) * if a < 0.0 && wa > 0.0 { -1.0 } else { 1.0 };
            let v = if base.abs() > 1e-12 { want / base } else { want };
            if wb > 1e-9 {
                v.abs().powf(1.0 / wb) * v.signum()
            } else {
                want
            }
        }
        BlendKind::Scale if wa >= 1.0 => {
            if wb.abs() > 1e-9 {
                (want - a) / wb + 1.0
            } else {
                want
            }
        }
        BlendKind::Switch => want,
        _ if wb.abs() > 1e-9 => (want - a * wa) / wb,
        _ => want,
    }
}

/// The value a layer needs at frame t for the channel to come out at `want`, the layers above it staying as they are.
fn resolve(g: &Graph, p: &PlugRef, layer: Option<NodeId>, want: f64, t: f64) -> f64 {
    let base = layer.is_none() || layer == base_layer(g);
    let mut d = want;
    for &b in &chain(g, p) {
        let (wa, wb, acc, kind) = weights(g, b);
        if !base && blend_layer(g, b) == layer {
            let a = input_at(g, &input(g, b, IN_A, p), p, t);
            return solve_b(kind, d, a, wa, wb, acc);
        }
        let vb = input_at(g, &input(g, b, IN_B, p), p, t);
        d = solve_a(kind, d, vb, wa, wb, acc);
    }
    d
}

/// The curve to key for a plug in layers, made if need be, and the value to give it. None when the layer is locked.
pub fn layer_curve_for_key(g: &mut Graph, p: &PlugRef, layer: Option<NodeId>, value: f64, raw: bool) -> CmdResult<Option<(NodeId, f64)>> {
    let in_chain = |g: &Graph, l: NodeId| Some(l) == base_layer(g) || chain(g, p).iter().any(|&b| blend_layer(g, b) == Some(l));
    // A layer the channel isn't in is ignored, as with Maya's -al.
    let target = layer.filter(|&l| in_chain(g, l)).or_else(|| best_layer(g, p));
    if target.is_some_and(|l| layer_flag(g, l, L_LOCK)) {
        return Ok(None);
    }
    let Some(input_plug) = slot(g, p, target) else { return Ok(None) };
    let t = anim::frame(g);
    let v = if raw { value } else { resolve(g, p, target, value, t) };
    if let Some(c) = curve_in(g, &input_plug) {
        return Ok(Some((c, v)));
    }
    let node_name = g.node(p.node).map(|n| n.name.clone()).unwrap_or_default();
    let channel = anim::channel_name(g, p);
    let name = match target.filter(|&l| Some(l) != base_layer(g)) {
        Some(l) => format!("{node_name}_{channel}_{}_inputB", g.node(l).map(|n| n.name.clone()).unwrap_or_default()),
        None => format!("{node_name}_{channel}"),
    };
    let ty = anim::curve_type_for(g, p);
    let c = anim::new_curve(g, ty, &name, &input_plug)?;
    Ok(Some((c, v)))
}

// ---------------------------------------------------------------------------------------------------------------------
// Editing the chains

fn blend_type_for(g: &Graph, p: &PlugRef) -> &'static str {
    let a = g.attr(p);
    match (a.ty, a.name.as_str()) {
        (PlugType::Vec3, n) if n.starts_with("rotate") => "animBlendNodeAdditiveRotation",
        (PlugType::Vec3, n) if n.starts_with("scale") => "animBlendNodeAdditiveScale",
        (PlugType::Vec3, n) if n.starts_with("translate") => "animBlendNodeAdditiveDL",
        (PlugType::Bool, _) => "animBlendNodeBoolean",
        (PlugType::Enum, _) => "animBlendNodeEnum",
        (PlugType::Int, _) => "animBlendNodeAdditiveI32",
        _ => "animBlendNodeAdditive",
    }
}

/// Adds a channel to a layer. Rotates go in whole, as Maya's rotation blends do. Does nothing if already there.
pub fn add_plug(g: &mut Graph, layer: NodeId, p: &PlugRef) -> CmdResult<()> {
    let rotate = g.attr(p).ty == PlugType::Vec3 && g.attr(p).name.starts_with("rotate");
    let probe = if rotate && p.component < 0 { PlugRef::component(p.node, p.attr, 0) } else { *p };
    let c = chain(g, &probe);
    if c.iter().any(|&b| blend_layer(g, b) == Some(layer)) {
        return Ok(());
    }
    let order = ordered_layers(g);
    let rank = |l: Option<NodeId>| l.and_then(|l| order.iter().position(|x| *x == l)).unwrap_or(0);
    let mine = rank(Some(layer));
    // The chain runs top first: the new blend goes above those of lower layers.
    let above = c.iter().copied().filter(|&b| rank(blend_layer(g, b)) > mine).next_back();

    let node_name = g.node(p.node).map(|n| n.name.clone()).unwrap_or_default();
    let layer_name = g.node(layer).map(|n| n.name.clone()).unwrap_or_default();
    let target = if rotate { p.whole() } else { *p };
    let channel = if rotate { g.attr(p).name.clone() } else { anim::channel_name(g, p) };
    let ty = types::find(blend_type_for(g, p)).unwrap();
    let blend = g.create_node(&ty, &format!("{node_name}_{channel}_{layer_name}"), NO_NODE, true)?;
    let inp = |which: usize, c: i32| if rotate { PlugRef::component(blend, which as i32, c) } else { PlugRef::new(blend, which as i32) };

    g.connect(&PlugRef::new(layer, L_FG as i32), &PlugRef::new(blend, WEIGHT_B as i32), false)?;
    g.connect(&PlugRef::new(layer, L_BG as i32), &PlugRef::new(blend, WEIGHT_A as i32), false)?;
    let is_scale = blend_kind(g, blend) == BlendKind::Scale;
    if rotate {
        g.connect(&PlugRef::new(layer, L_RAM as i32), &PlugRef::new(blend, ACCUMULATE as i32), false)?;
        let order = g.node(p.node).map(|n| n.data().int(Transform::ROTATE_ORDER)).unwrap_or(0);
        g.set_value(PlugRef::new(blend, BLEND_ROT_ORDER as i32), PlugValue::Int(order))?;
    } else if is_scale {
        g.connect(&PlugRef::new(layer, L_SAM as i32), &PlugRef::new(blend, ACCUMULATE as i32), false)?;
    }

    let out = PlugRef::new(blend, BLEND_OUT as i32);
    if rotate {
        match above {
            Some(up) => {
                let ua = PlugRef::new(up, IN_A as i32);
                match g.source(&ua) {
                    Some(lower) => {
                        g.disconnect(&ua);
                        g.connect(&lower, &PlugRef::new(blend, IN_A as i32), false)?;
                    }
                    None => {
                        for ax in 0..3 {
                            let upc = PlugRef::component(up, IN_A as i32, ax);
                            move_input(g, &upc, &inp(IN_A, ax))?;
                        }
                    }
                }
                g.connect(&out, &ua, true)?;
            }
            None => match g.source(&target) {
                Some(whole) => {
                    g.disconnect(&target);
                    g.connect(&whole, &PlugRef::new(blend, IN_A as i32), false)?;
                    g.connect(&out, &target, false)?;
                }
                None => {
                    for ax in 0..3 {
                        let comp = PlugRef::component(target.node, target.attr, ax);
                        move_input(g, &comp, &inp(IN_A, ax))?;
                    }
                    g.connect(&out, &target, true)?;
                }
            },
        }
    } else {
        let dst = match above {
            Some(up) => PlugRef::new(up, IN_A as i32),
            None => curve_slot(g, &target),
        };
        move_input(g, &dst, &inp(IN_A, -1))?;
        g.connect(&out, &dst, true)?;
    }
    // A switch has no value of nothing to add: unkeyed, it shows what is under it, as an override layer does.
    if layer_flag(g, layer, L_OVERRIDE) || blend_kind(g, blend) == BlendKind::Switch {
        init_override_input(g, blend, rotate)?;
    }
    Ok(())
}

/// Moves what is on one input to another: its connection, or the value it holds.
fn move_input(g: &mut Graph, from: &PlugRef, to: &PlugRef) -> CmdResult<()> {
    match g.source(from) {
        Some(s) => {
            g.disconnect(from);
            g.connect(&s, to, false)?;
        }
        None => {
            let v = g.value(from)?.scalar().unwrap_or(0.0);
            g.set_value(*to, PlugValue::Float(v))?;
        }
    }
    Ok(())
}

/// An override layer with no keys yet holds what is under it instead of pulling the channel to zero.
fn init_override_input(g: &mut Graph, blend: NodeId, rotate: bool) -> CmdResult<()> {
    let plugs: Vec<(PlugRef, PlugRef)> = if rotate {
        (0..3).map(|c| (PlugRef::component(blend, IN_A as i32, c), PlugRef::component(blend, IN_B as i32, c))).collect()
    } else {
        vec![(PlugRef::new(blend, IN_A as i32), PlugRef::new(blend, IN_B as i32))]
    };
    for (a, b) in plugs {
        if g.source(&b).is_none() {
            let v = g.value(&a)?.scalar().unwrap_or(0.0);
            g.set_value(b, PlugValue::Float(v))?;
        }
    }
    Ok(())
}

/// The plugs a blend drives, in the end.
fn blend_plugs(g: &Graph, blend: NodeId) -> Vec<PlugRef> {
    g.destinations(&PlugRef::new(blend, BLEND_OUT as i32)).into_iter().flat_map(|d| final_plugs(g, d)).collect()
}

fn layer_blends(g: &Graph, layer: NodeId) -> Vec<NodeId> {
    g.destinations(&PlugRef::new(layer, L_FG as i32)).into_iter().filter(|d| is_blend(g, d.node) && d.attr == WEIGHT_B as i32).map(|d| d.node).collect()
}

/// Takes one blend out of its chain and deletes it, with the layer's curves on it.
fn remove_blend(g: &mut Graph, blend: NodeId) -> CmdResult<()> {
    let rotate = is_rotation(g, blend);
    let dests = g.destinations(&PlugRef::new(blend, BLEND_OUT as i32));
    let bs: Vec<PlugRef> = if rotate { (0..3).map(|c| PlugRef::component(blend, IN_B as i32, c)).collect() } else { vec![PlugRef::new(blend, IN_B as i32)] };
    let curves: Vec<NodeId> = bs.iter().filter_map(|b| curve_in(g, b)).collect();
    if rotate {
        let whole = g.source(&PlugRef::new(blend, IN_A as i32));
        for d in &dests {
            g.disconnect(d);
            match whole {
                Some(s) => {
                    g.disconnect(&PlugRef::new(blend, IN_A as i32));
                    g.connect(&s, d, false)?;
                }
                None => {
                    for ax in 0..3 {
                        move_input(g, &PlugRef::component(blend, IN_A as i32, ax), &PlugRef::component(d.node, d.attr, ax))?;
                    }
                }
            }
        }
    } else {
        for d in &dests {
            g.disconnect(d);
            move_input(g, &PlugRef::new(blend, IN_A as i32), d)?;
        }
    }
    g.delete_node(blend)?;
    for c in curves {
        g.delete_node(c)?;
    }
    Ok(())
}

/// A layer's channels and its child layers go with it; when no layers are left the base goes too.
pub fn before_delete(g: &mut Graph, id: NodeId) -> CmdResult<()> {
    if !is_layer(g, id) {
        return Ok(());
    }
    if Some(id) == base_layer(g) {
        if !layer_children(g, id).is_empty() {
            fail!("delete: {BASE_LAYER} goes when the last layer does");
        }
        return Ok(());
    }
    for b in layer_blends(g, id) {
        remove_blend(g, b)?;
    }
    let parent = layer_parent(g, id);
    for c in layer_children(g, id) {
        let pw = PlugRef::new(c, L_PARENT as i32);
        g.disconnect(&pw);
        if let Some(p) = parent {
            g.connect(&PlugRef::new(p, L_FG as i32), &pw, false)?;
        }
    }
    Ok(())
}

/// After deleting layers: the base goes with the last of them.
pub fn after_delete(g: &mut Graph) -> CmdResult<()> {
    if let Some(b) = base_layer(g) {
        if layer_children(g, b).is_empty() {
            g.delete_node(b)?;
        }
    }
    Ok(())
}

fn ensure_base(g: &mut Graph) -> CmdResult<NodeId> {
    if let Some(b) = base_layer(g) {
        return Ok(b);
    }
    let t = types::find("animLayer").unwrap();
    Ok(g.create_node(&t, BASE_LAYER, NO_NODE, true)?)
}

/// Recomputes who is off because of a solo.
fn refresh_solo(g: &mut Graph) -> CmdResult<()> {
    let layers = ordered_layers(g);
    let soloed: Vec<NodeId> = layers.iter().copied().filter(|&l| layer_flag(g, l, L_SOLO)).collect();
    let base = base_layer(g);
    for l in layers {
        if Some(l) == base {
            continue;
        }
        // A soloed layer brings its ancestors and descendants with it.
        let related = |g: &Graph, s: NodeId| {
            let up = |mut a: NodeId, b: NodeId| {
                while let Some(x) = layer_parent(g, a) {
                    if x == b {
                        return true;
                    }
                    a = x;
                }
                false
            };
            s == l || up(s, l) || up(l, s)
        };
        let off = !soloed.is_empty() && !soloed.iter().any(|&s| related(g, s));
        g.set_value(PlugRef::new(l, L_SOLO_MUTED as i32), PlugValue::Bool(off))?;
    }
    Ok(())
}

fn split_rotates(g: &Graph, plugs: Vec<PlugRef>) -> Vec<PlugRef> {
    plugs
        .into_iter()
        .flat_map(|p| {
            if p.component < 0 && g.attr(&p).ty == PlugType::Vec3 {
                (0..3).map(|c| PlugRef::component(p.node, p.attr, c)).collect::<Vec<_>>()
            } else {
                vec![p]
            }
        })
        .collect()
}

fn layer_arg(g: &Graph, a: &Args) -> CmdResult<Option<NodeId>> {
    for n in a.named() {
        if let Ok(Some(id)) = g.find(&n) {
            if is_layer(g, id) {
                return Ok(Some(id));
            }
        }
    }
    Ok(None)
}

fn excluded(g: &Graph, p: &PlugRef, a: &Args) -> bool {
    let attr = g.attr(p);
    (a.bool("excludeBoolean") && attr.ty == PlugType::Bool)
        || (a.bool("excludeEnum") && attr.ty == PlugType::Enum)
        || (a.bool("excludeRotate") && attr.name.starts_with("rotate"))
        || (a.bool("excludeScale") && attr.name.starts_with("scale"))
        || (a.bool("excludeTranslate") && attr.name.starts_with("translate"))
        || (a.bool("excludeVisibility") && attr.name == "visibility")
}

fn attr_list(a: &Args, flag: &str) -> Vec<String> {
    match a.get(flag) {
        Some(Value::String(x)) => vec![x.clone()],
        Some(Value::Array(v)) => v.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    }
}

fn apply_flags(s: &mut Session, layer: NodeId, a: &Args) -> CmdResult<()> {
    let g = &mut s.graph;
    for (flag, at) in [("mute", L_MUTE), ("lock", L_LOCK), ("solo", L_SOLO), ("selected", L_SELECTED), ("preferred", L_PREFERRED), ("passthrough", L_PASSTHROUGH)] {
        if let Some(b) = a.get(flag).and_then(Value::as_bool) {
            g.set_value(PlugRef::new(layer, at as i32), PlugValue::Bool(b))?;
        }
    }
    if let Some(w) = a.get("weight").and_then(Value::as_f64) {
        g.set_value(PlugRef::new(layer, L_WEIGHT as i32), PlugValue::Float(w))?;
    }
    if let Some(b) = a.get("override").and_then(Value::as_bool) {
        g.set_value(PlugRef::new(layer, L_OVERRIDE as i32), PlugValue::Bool(b))?;
        for blend in layer_blends(g, layer) {
            let rotate = is_rotation(g, blend);
            if b {
                init_override_input(g, blend, rotate)?;
            } else if blend_kind(g, blend) != BlendKind::Switch && g.source(&PlugRef::new(blend, IN_B as i32)).is_none() {
                // Back to adding: an unkeyed layer adds nothing.
                let rest = if blend_kind(g, blend) == BlendKind::Scale { 1.0 } else { 0.0 };
                if rotate {
                    for c in 0..3 {
                        let p = PlugRef::component(blend, IN_B as i32, c);
                        if g.source(&p).is_none() {
                            g.set_value(p, PlugValue::Float(rest))?;
                        }
                    }
                } else {
                    g.set_value(PlugRef::new(blend, IN_B as i32), PlugValue::Float(rest))?;
                }
            }
        }
    }
    if let Some(mode) = a.get("rotationAccumulationMode") {
        let v = mode.as_i64().or(mode.as_str().map(|x| i64::from(x == "layer"))).unwrap_or(0);
        g.set_value(PlugRef::new(layer, L_RAM as i32), PlugValue::Int(v))?;
    }
    if let Some(mode) = a.get("scaleAccumulationMode") {
        let v = mode.as_i64().or(mode.as_str().map(|x| i64::from(x == "multiply"))).unwrap_or(1);
        g.set_value(PlugRef::new(layer, L_SAM as i32), PlugValue::Int(v))?;
    }
    if let Some(n) = a.get("parent").and_then(Value::as_str) {
        let Some(p) = g.find(n)?.filter(|&x| is_layer(g, x)) else { fail!("animLayer: no layer named {n}") };
        set_parent(g, layer, p)?;
    }
    for (flag, after) in [("moveLayerAfter", true), ("moveLayerBefore", false)] {
        if let Some(n) = a.get(flag).and_then(Value::as_str) {
            let Some(o) = g.find(n)?.filter(|&x| is_layer(g, x)) else { fail!("animLayer: no layer named {n}") };
            move_layer(g, layer, o, after)?;
        }
    }
    if a.has("solo") {
        refresh_solo(g)?;
    }
    // Channels.
    let mut plugs: Vec<PlugRef> = Vec::new();
    for at in attr_list(a, "attribute") {
        plugs.extend(split_rotates(&s.graph, vec![s.graph.plug(&at)?]));
    }
    if a.bool("addSelectedObjects") {
        let sel: Vec<NodeId> = s.graph.selection().to_vec();
        for id in sel {
            let t = crate::dag::transform_of(&s.graph, id);
            plugs.extend(anim::keyable_plugs(&s.graph, t));
        }
    }
    plugs.retain(|p| !excluded(&s.graph, p, a));
    let g = &mut s.graph;
    for p in &plugs {
        add_plug(g, layer, p)?;
    }
    let mut gone: Vec<PlugRef> = Vec::new();
    for at in attr_list(a, "removeAttribute") {
        gone.extend(split_rotates(g, vec![g.plug(&at)?]));
    }
    for p in gone {
        if let Some(&b) = chain(g, &p).iter().find(|&&b| blend_layer(g, b) == Some(layer)) {
            remove_blend(g, b)?;
        }
    }
    if a.bool("removeAllAttributes") {
        for b in layer_blends(g, layer) {
            remove_blend(g, b)?;
        }
    }
    if a.bool("extractAnimation") {
        // The layer's channels' base curves become this layer's.
        for b in layer_blends(g, layer) {
            let plugs = blend_plugs(g, b);
            for p in split_rotates(g, plugs) {
                if let Some(c) = chain(g, &p).last().and_then(|&bot| curve_in(g, &input(g, bot, IN_A, &p))) {
                    let k = anim::keys_of(g, c);
                    if let Some(slot_plug) = slot(g, &p, Some(layer)) {
                        let target = match curve_in(g, &slot_plug) {
                            Some(x) => x,
                            None => {
                                let (node_name, channel, layer_name) = (g.node(p.node).map(|n| n.name.clone()).unwrap_or_default(), anim::channel_name(g, &p), g.node(layer).map(|n| n.name.clone()).unwrap_or_default());
                                let ty = anim::curve_type_for(g, &p);
                                anim::new_curve(g, ty, &format!("{node_name}_{channel}_{layer_name}_inputB"), &slot_plug)?
                            }
                        };
                        anim::set_keys(g, target, k)?;
                        let v = g.value(&PlugRef::new(c, OUTPUT as i32))?.scalar().unwrap_or(0.0);
                        let inp = input(g, *chain(g, &p).last().unwrap(), IN_A, &p);
                        g.delete_node(c)?;
                        g.set_value(inp, PlugValue::Float(v))?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn set_parent(g: &mut Graph, layer: NodeId, parent: NodeId) -> CmdResult<()> {
    let pw = PlugRef::new(layer, L_PARENT as i32);
    if g.source(&pw).is_some() {
        g.disconnect(&pw);
    }
    g.connect(&PlugRef::new(parent, L_FG as i32), &pw, false)?;
    let n = layer_children(g, parent).len() as i64;
    g.set_value(PlugRef::new(layer, L_ORDER as i32), PlugValue::Int(n))?;
    Ok(())
}

/// Puts a layer next to another in the stack, and rebuilds its channels' chains in that order.
fn move_layer(g: &mut Graph, layer: NodeId, other: NodeId, after: bool) -> CmdResult<()> {
    let Some(parent) = layer_parent(g, other) else { fail!("animLayer: can't move next to {BASE_LAYER}") };
    if layer_parent(g, layer) != Some(parent) {
        set_parent(g, layer, parent)?;
    }
    let mut sibs: Vec<NodeId> = layer_children(g, parent).into_iter().filter(|&x| x != layer).collect();
    let at = sibs.iter().position(|&x| x == other).unwrap_or(0) + usize::from(after);
    sibs.insert(at, layer);
    for (i, l) in sibs.iter().enumerate() {
        g.set_value(PlugRef::new(*l, L_ORDER as i32), PlugValue::Int(i as i64))?;
    }
    // Chains: take this layer's blends out and put them back in their new place, curves kept.
    for b in layer_blends(g, layer) {
        let plugs = blend_plugs(g, b);
        let rotate = is_rotation(g, b);
        let Some(p) = plugs.first().copied() else { continue };
        let plug = if rotate { p.whole() } else { p };
        let probes: Vec<PlugRef> = if rotate { (0..3).map(|c| PlugRef::component(plug.node, plug.attr, c)).collect() } else { vec![plug] };
        let saved: Vec<(usize, Option<PlugRef>, f64)> = probes
            .iter()
            .enumerate()
            .map(|(i, q)| {
                let inp = input(g, b, IN_B, q);
                let v = g.value(&inp).ok().and_then(|x| x.scalar()).unwrap_or(0.0);
                (i, g.source(&inp), v)
            })
            .collect();
        for (i, src, _) in &saved {
            if src.is_some() {
                g.disconnect(&input(g, b, IN_B, &probes[*i]));
            }
        }
        remove_blend(g, b)?;
        add_plug(g, layer, &plug)?;
        let new_b = *chain(g, &probes[0]).iter().find(|&&x| blend_layer(g, x) == Some(layer)).unwrap();
        for (i, src, v) in saved {
            let inp = input(g, new_b, IN_B, &probes[i]);
            match src {
                Some(s) => {
                    g.connect(&s, &inp, true)?;
                }
                None => g.set_value(inp, PlugValue::Float(v))?,
            }
        }
    }
    Ok(())
}

fn names(g: &Graph, ids: impl IntoIterator<Item = NodeId>) -> Value {
    json!(ids.into_iter().map(|i| g.path(i)).collect::<Vec<_>>())
}

fn layers_of_plug(g: &Graph, p: &PlugRef) -> Vec<NodeId> {
    let mut c: Vec<NodeId> = chain(g, p).iter().filter_map(|&b| blend_layer(g, b)).collect();
    if !chain(g, p).is_empty() {
        if let Some(b) = base_layer(g) {
            c.push(b);
        }
    }
    c.reverse();
    c
}

fn query(s: &Session, a: &Args) -> CmdResult {
    let g = &s.graph;
    let layer = layer_arg(g, a)?;
    let the = || -> CmdResult<NodeId> {
        match layer.or_else(|| base_layer(g)) {
            Some(l) => Ok(l),
            None => fail!("animLayer: no layers"),
        }
    };
    let plain_plugs = || -> CmdResult<Vec<PlugRef>> {
        let mut out = Vec::new();
        let named: Vec<String> = a.named().into_iter().filter(|n| g.find(n).ok().flatten().is_none_or(|id| !is_layer(g, id))).collect();
        let names = if named.is_empty() { g.selection().iter().map(|&i| g.path(i)).collect() } else { named };
        for n in names {
            if n.contains('.') {
                out.extend(split_rotates(g, vec![g.plug(&n)?]));
            } else if let Some(id) = g.find(&n)? {
                out.extend(anim::keyable_plugs(g, crate::dag::transform_of(g, id)));
            }
        }
        Ok(out)
    };
    if queried(a, &["exists"]) {
        return Ok(json!(layer.is_some()));
    }
    if queried(a, &["root"]) {
        return Ok(base_layer(g).map(|b| json!(g.path(b))).unwrap_or(Value::Null));
    }
    if queried(a, &["maxLayers"]) {
        return Ok(json!(ordered_layers(g).len()));
    }
    if queried(a, &["layeredHierarchy"]) {
        return Ok(names(g, ordered_layers(g)));
    }
    if queried(a, &["affectedLayers"]) {
        let mut out: Vec<NodeId> = Vec::new();
        for p in plain_plugs()? {
            for l in layers_of_plug(g, &p) {
                if !out.contains(&l) {
                    out.push(l);
                }
            }
        }
        return Ok(names(g, out));
    }
    if queried(a, &["bestLayer", "bestAnimLayer"]) {
        let plugs = plain_plugs()?;
        let best = plugs.first().and_then(|p| best_layer(g, p)).or_else(|| base_layer(g));
        return Ok(best.map(|b| json!(g.path(b))).unwrap_or(Value::Null));
    }
    if queried(a, &["findCurveForPlug"]) {
        // Asking for the curves of a plug: on the named layer or the best one.
        let plugs = match a.get("findCurveForPlug").and_then(Value::as_str) {
            Some(n) => split_rotates(g, vec![g.plug(n)?]),
            None => plain_plugs()?,
        };
        let mut out = Vec::new();
        for p in plugs {
            let l = layer.or_else(|| best_layer(g, &p));
            let c = match slot(g, &p, l) {
                Some(sl) => curve_in(g, &sl),
                None => anim::curve_of(g, &p),
            };
            out.extend(c);
        }
        return Ok(names(g, out));
    }
    let l = the()?;
    let flag = |at: usize| json!(layer_flag(g, l, at));
    let d = g.node(l).map(|n| n.data());
    Ok(if queried(a, &["mute"]) {
        flag(L_MUTE)
    } else if queried(a, &["lock"]) {
        flag(L_LOCK)
    } else if queried(a, &["solo"]) {
        flag(L_SOLO)
    } else if queried(a, &["override"]) {
        flag(L_OVERRIDE)
    } else if queried(a, &["selected"]) {
        flag(L_SELECTED)
    } else if queried(a, &["preferred"]) {
        flag(L_PREFERRED)
    } else if queried(a, &["passthrough"]) {
        flag(L_PASSTHROUGH)
    } else if queried(a, &["weight"]) {
        json!(d.map(|d| d.float(L_WEIGHT)).unwrap_or(1.0))
    } else if queried(a, &["rotationAccumulationMode"]) {
        json!(d.map(|d| d.int(L_RAM)).unwrap_or(0))
    } else if queried(a, &["scaleAccumulationMode"]) {
        json!(d.map(|d| d.int(L_SAM)).unwrap_or(1))
    } else if queried(a, &["parent"]) {
        layer_parent(g, l).map(|p| json!(g.path(p))).unwrap_or(Value::Null)
    } else if queried(a, &["children"]) {
        names(g, layer_children(g, l))
    } else if queried(a, &["attribute"]) {
        let mut out: Vec<String> = Vec::new();
        for b in layer_blends(g, l) {
            for p in split_rotates(g, blend_plugs(g, b)) {
                out.push(g.plug_name(&p));
            }
        }
        json!(out)
    } else if queried(a, &["animCurves"]) {
        let mut out: Vec<NodeId> = Vec::new();
        for b in layer_blends(g, l) {
            let inputs: Vec<PlugRef> = if is_rotation(g, b) { (0..3).map(|c| PlugRef::component(b, IN_B as i32, c)).collect() } else { vec![PlugRef::new(b, IN_B as i32)] };
            out.extend(inputs.iter().filter_map(|i| curve_in(g, i)));
        }
        names(g, out)
    } else if queried(a, &["baseAnimCurves"]) {
        let mut out: Vec<NodeId> = Vec::new();
        for b in layer_blends(g, l) {
            for p in split_rotates(g, blend_plugs(g, b)) {
                if let Some(c) = chain(g, &p).last().and_then(|&x| curve_in(g, &input(g, x, IN_A, &p))) {
                    if !out.contains(&c) {
                        out.push(c);
                    }
                }
            }
        }
        names(g, out)
    } else if queried(a, &["blendNodes"]) {
        names(g, layer_blends(g, l))
    } else {
        json!(g.path(l))
    })
}

fn anim_layer(s: &mut Session, a: &Args) -> CmdResult {
    if a.bool("query") {
        return query(s, a);
    }
    if a.bool("edit") {
        let layer = layer_arg(&s.graph, a)?;
        let Some(l) = layer else { fail!("animLayer: name the layer to edit") };
        apply_flags(s, l, a)?;
        return Ok(json!(s.graph.path(l)));
    }
    // Create. Maya's default name is AnimLayer1.
    let base = ensure_base(&mut s.graph)?;
    let name = a.named().first().cloned().unwrap_or_else(|| "AnimLayer1".into());
    let t = types::find("animLayer").unwrap();
    let wanted = a.get("copy").and_then(Value::as_str).map(str::to_string);
    let l = s.graph.create_node(&t, &name, NO_NODE, true)?;
    set_parent(&mut s.graph, l, base)?;
    apply_flags(s, l, a)?;
    if let Some(src) = wanted {
        let Some(from) = s.graph.find(&src)?.filter(|&x| is_layer(&s.graph, x)) else { fail!("animLayer: no layer named {src}") };
        for flag in [L_OVERRIDE, L_RAM, L_SAM] {
            let v = s.graph.node(from).map(|n| n.value(flag).clone());
            if let Some(v) = v {
                s.graph.set_value(PlugRef::new(l, flag as i32), v)?;
            }
        }
        let w = s.graph.node(from).map(|n| n.data().float(L_WEIGHT)).unwrap_or(1.0);
        s.graph.set_value(PlugRef::new(l, L_WEIGHT as i32), PlugValue::Float(w))?;
        // Its channels, and with -ca its keys.
        for b in layer_blends(&s.graph, from) {
            let plugs = split_rotates(&s.graph, blend_plugs(&s.graph, b));
            for p in plugs {
                add_plug(&mut s.graph, l, &p)?;
                if a.bool("copyAnimation") {
                    let g = &mut s.graph;
                    if let (Some(fs), Some(ts)) = (slot(g, &p, Some(from)), slot(g, &p, Some(l))) {
                        if let Some(c) = curve_in(g, &fs) {
                            let k = anim::keys_of(g, c);
                            let (node_name, channel, layer_name) = (g.node(p.node).map(|n| n.name.clone()).unwrap_or_default(), anim::channel_name(g, &p), g.node(l).map(|n| n.name.clone()).unwrap_or_default());
                            let ty = anim::curve_type_for(g, &p);
                            let n = anim::new_curve(g, ty, &format!("{node_name}_{channel}_{layer_name}_inputB"), &ts)?;
                            anim::set_keys(g, n, k)?;
                        }
                    }
                }
            }
        }
    }
    Ok(json!(s.graph.path(l)))
}

/// Merges layers into the lowest of them: its curves take the result, keyed every -sb frames (or every frame) over the
/// keys' range, then reduced to what keeps the result; the others go.
fn merge_layers(s: &mut Session, a: &Args) -> CmdResult {
    let g = &mut s.graph;
    let order = ordered_layers(g);
    let mut layers: Vec<NodeId> = Vec::new();
    let named = {
        let v = a.named();
        if v.is_empty() {
            order.iter().copied().filter(|&l| layer_flag(g, l, L_SELECTED) || Some(l) == base_layer(g)).map(|l| g.path(l)).collect()
        } else {
            v
        }
    };
    for n in named {
        let Some(id) = g.find(&n)?.filter(|&x| is_layer(g, x)) else { fail!("mergeAnimLayers: no layer named {n}") };
        layers.push(id);
    }
    layers.sort_by_key(|l| order.iter().position(|x| x == l).unwrap_or(0));
    layers.dedup();
    if layers.len() < 2 {
        fail!("mergeAnimLayers: name two or more layers");
    }
    let target = layers[0];
    let target_name = g.path(target);
    let upper: Vec<NodeId> = layers[1..].to_vec();
    let by = a.float("sampleBy", 1.0).max(1e-3);
    // Every channel in the layers going away.
    let mut plugs: Vec<PlugRef> = Vec::new();
    for &l in &upper {
        for b in layer_blends(g, l) {
            for p in split_rotates(g, blend_plugs(g, b)) {
                if !plugs.contains(&p) {
                    plugs.push(p);
                }
            }
        }
    }
    for &p_ in &plugs {
        let c = chain(g, &p_);
        // The merged result comes out of the topmost merged blend; layers above the merge stay as they are.
        let Some(&top) = c.iter().find(|&&b| blend_layer(g, b).is_some_and(|l| upper.contains(&l))) else { continue };
        let curve_list: Vec<NodeId> = c.iter().flat_map(|&b| [curve_in(g, &input(g, b, IN_A, &p_)), curve_in(g, &input(g, b, IN_B, &p_))]).flatten().collect();
        let times: Vec<f64> = curve_list.iter().flat_map(|&x| anim::keys_of(g, x).keys.into_iter().map(|k| k.t)).collect();
        if times.is_empty() {
            continue;
        }
        let (t0, t1) = (times.iter().copied().fold(f64::INFINITY, f64::min), times.iter().copied().fold(f64::NEG_INFINITY, f64::max));
        let mut samples = Vec::new();
        let n = ((t1 - t0) / by).ceil() as usize;
        for i in 0..=n {
            let t = (t0 + i as f64 * by).min(t1);
            let v = blend_at(g, top, &p_, t);
            samples.push((t, v));
        }
        // What the target layer has to hold for the same result, the merged layers having gone: their blends removed.
        let solved: Vec<(f64, f64)> = samples
            .iter()
            .map(|&(t, v)| {
                if Some(target) == base_layer(g) {
                    // Blends below the target don't exist; those between it and the top are going.
                    (t, v)
                } else {
                    let b = *c.iter().find(|&&b| blend_layer(g, b) == Some(target)).unwrap_or(&top);
                    let (wa, wb, acc, kind) = weights(g, b);
                    let inp_a = input_at(g, &input(g, b, IN_A, &p_), &p_, t);
                    (t, solve_b(kind, v, inp_a, wa, wb, acc))
                }
            })
            .collect();
        let r = anim::range();
        let mut baked = Keys { keys: solved.iter().map(|&(t, v)| Key::new(t, v, r.default_in, r.default_out)).collect(), weighted: false };
        baked.sort();
        baked = crate::anim_filter::reduce(&baked, 1e-4, 0.0, None);
        let in_target = c.iter().any(|&b| blend_layer(g, b) == Some(target)) || Some(target) == base_layer(g);
        if !in_target {
            add_plug(g, target, &p_)?;
        }
        let Some(sl) = slot(g, &p_, Some(target)) else { continue };
        let curve = match curve_in(g, &sl) {
            Some(x) => x,
            None => {
                let (node_name, channel) = (g.node(p_.node).map(|n| n.name.clone()).unwrap_or_default(), anim::channel_name(g, &p_));
                let name = if Some(target) == base_layer(g) { format!("{node_name}_{channel}") } else { format!("{node_name}_{channel}_{}_inputB", g.node(target).map(|n| n.name.clone()).unwrap_or_default()) };
                let ty = anim::curve_type_for(g, &p_);
                anim::new_curve(g, ty, &name, &sl)?
            }
        };
        anim::set_keys(g, curve, baked)?;
    }
    for l in upper {
        before_delete(g, l)?;
        g.delete_node(l)?;
    }
    after_delete(g)?;
    Ok(json!(target_name))
}

pub fn register(c: &mut Commands<Session>) {
    register_types();
    c.register(
        query_flags(Cmd::new(
            "animLayer",
            "Create (name optional), edit (-e layer) or query (-q layer) an animation layer. The first layer brings BaseAnimation with it. -aso or -at put \
             channels in the layer; setKeyframe -al keys them there. -q: -m -l -s -o -w -sel -p -c -r -at -anc -bac -bld -afl -bl -fcv -ex -lh.",
            anim_layer,
        ))
        .positional("the layer, or for -afl and -bl the objects or plugs")
        .flag(f("mute", "m", ArgType::Bool, "Mute the layer."))
        .flag(f("lock", "l", ArgType::Bool, "Lock the layer against keying."))
        .flag(f("solo", "s", ArgType::Bool, "Solo the layer."))
        .flag(f("override", "o", ArgType::Bool, "Override what is below instead of adding."))
        .flag(f("weight", "w", ArgType::Float, "The layer's weight, 0 to 1."))
        .flag(f("parent", "p", ArgType::String, "The parent layer."))
        .flag(f("children", "c", ArgType::String, "Query: the child layers."))
        .flag(f("root", "r", ArgType::String, "Query: the base layer."))
        .flag(f("selected", "sel", ArgType::Bool, "Keys go to the topmost selected layer."))
        .flag(f("preferred", "prf", ArgType::Bool, "Keys go to this layer when none is selected."))
        .flag(f("passthrough", "pth", ArgType::Bool, "Let lower layers through."))
        .flag(f("addSelectedObjects", "aso", ArgType::Bool, "Add the selected objects' keyable channels."))
        .flag(f("attribute", "at", ArgType::Any, "Plug(s) to add; query: the layer's plugs.").multi(1))
        .flag(f("removeAttribute", "ra", ArgType::Any, "Plug(s) to remove.").multi(1))
        .flag(f("removeAllAttributes", "raa", ArgType::Bool, "Remove every channel."))
        .flag(f("excludeBoolean", "ebl", ArgType::Bool, "With -aso: not boolean channels."))
        .flag(f("excludeEnum", "een", ArgType::Bool, "With -aso: not enum channels."))
        .flag(f("excludeRotate", "ert", ArgType::Bool, "With -aso: not rotate channels."))
        .flag(f("excludeScale", "esc", ArgType::Bool, "With -aso: not scale channels."))
        .flag(f("excludeTranslate", "etr", ArgType::Bool, "With -aso: not translate channels."))
        .flag(f("excludeVisibility", "evs", ArgType::Bool, "With -aso: not visibility."))
        .flag(f("excludeDynamic", "edn", ArgType::Bool, "Accepted for Maya scripts: -aso takes keyable channels."))
        .flag(f("affectedLayers", "afl", ArgType::Bool, "Query: the layers the objects or plugs are in."))
        .flag(f("animCurves", "anc", ArgType::Bool, "Query: the layer's curves."))
        .flag(f("baseAnimCurves", "bac", ArgType::Bool, "Query: the base curves of the layer's channels."))
        .flag(f("blendNodes", "bld", ArgType::Bool, "Query: the layer's blend nodes."))
        .flag(f("bestLayer", "bl", ArgType::Bool, "Query: where keys on the plugs go."))
        .flag(f("bestAnimLayer", "blr", ArgType::Bool, "Query: as -bl."))
        .flag(f("findCurveForPlug", "fcv", ArgType::String, "Query: the curve holding a plug's keys in the layer."))
        .flag(f("exists", "ex", ArgType::Bool, "Query: whether the layer exists."))
        .flag(f("layeredHierarchy", "lh", ArgType::Bool, "Query: all layers, bottom to top."))
        .flag(f("maxLayers", "mxl", ArgType::Bool, "Query: how many layers there are."))
        .flag(f("moveLayerAfter", "mva", ArgType::String, "Move the layer above this one."))
        .flag(f("moveLayerBefore", "mvb", ArgType::String, "Move the layer below this one."))
        .flag(f("copy", "cp", ArgType::String, "Create: copy this layer's settings and channels."))
        .flag(f("copyAnimation", "ca", ArgType::Bool, "With -cp: and its keys."))
        .flag(f("copyNoAnimation", "cna", ArgType::Bool, "With -cp: without keys (the default)."))
        .flag(f("extractAnimation", "xa", ArgType::Bool, "Move the base curves of the layer's channels onto the layer."))
        .flag(f("rotationAccumulationMode", "ram", ArgType::Any, "component (0) or layer (1)."))
        .flag(f("scaleAccumulationMode", "sam", ArgType::Any, "additive (0) or multiply (1)."))
        .flag(f("forceUIRebuild", "fur", ArgType::Bool, "Accepted for Maya scripts."))
        .flag(f("forceUIRefresh", "uir", ArgType::Bool, "Accepted for Maya scripts.")),
    );

    c.register(
        Cmd::new(
            "mergeAnimLayers",
            "Merge layers into the lowest of them (the selected layers and the base when none are named), keeping the result. -sb sets how often it samples. \
             Stands in for the scripts behind Maya's Merge Layers. Returns the layer merged into.",
            merge_layers,
        )
        .positional("the layers")
        .flag(f("sampleBy", "sb", ArgType::Float, "Frames between samples (1).")),
    );
}
