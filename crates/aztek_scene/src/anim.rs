//! Keyframe animation as Maya does it: animCurve nodes (TL, TA, TU) driving channels, keyed with setKeyframe and
//! edited with keyframe, cutKey and keyTangent; the time range in playbackOptions; times in frames.

use std::any::Any;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use aztek_ops::{fail, objects, ArgType, CmdResult, CommandDef, Commands, FlagDef};
use serde_json::{json, Value};

use crate::graph::{DrivenEdit, Graph};
use crate::node::{types, EvalContext, NodeBehavior, NodeData, NodeType};
use crate::plug::{flags, AttrDef, NodeId, PlugRef, PlugType, PlugValue, NO_NODE};
use crate::session::Session;

// ---------------------------------------------------------------------------------------------------------------------
// Settings

static FPS_BITS: AtomicU64 = AtomicU64::new(0x4038_0000_0000_0000); // 24.0
static AUTO_KEY: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug)]
pub struct Range {
    pub min: f64,
    pub max: f64,
    pub anim_start: f64,
    pub anim_end: f64,
    pub loop_mode: String,
    pub speed: f64,
    pub default_in: Tangent,
    pub default_out: Tangent,
}

static RANGE: Mutex<Option<Range>> = Mutex::new(None);

fn default_range() -> Range {
    Range {
        min: 1.0,
        max: 120.0,
        anim_start: 1.0,
        anim_end: 200.0,
        loop_mode: "continuous".into(),
        speed: 1.0,
        default_in: Tangent::Auto,
        default_out: Tangent::Auto,
    }
}

pub fn range() -> Range {
    RANGE.lock().unwrap().clone().unwrap_or_else(default_range)
}

fn set_range(f: impl FnOnce(&mut Range)) {
    let mut r = RANGE.lock().unwrap();
    let mut v = r.clone().unwrap_or_else(default_range);
    f(&mut v);
    *r = Some(v);
}

pub fn fps() -> f64 {
    f64::from_bits(FPS_BITS.load(Ordering::Relaxed))
}

pub fn auto_key() -> bool {
    AUTO_KEY.load(Ordering::Relaxed)
}

/// The graph's time (seconds) as a frame.
pub fn frame(g: &Graph) -> f64 {
    let f = g.time() * fps();
    let r = f.round();
    if (f - r).abs() < 1e-6 {
        r
    } else {
        f
    }
}

/// A frame as JSON: whole frames print as integers, as Maya's results do.
pub fn frame_json(f: f64) -> Value {
    if f.fract() == 0.0 && f.abs() < 1e15 {
        json!(f as i64)
    } else {
        json!(f)
    }
}

pub fn set_frame(g: &mut Graph, f: f64) {
    g.set_time(f / fps());
}

// ---------------------------------------------------------------------------------------------------------------------
// Keys

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tangent {
    Auto,
    Spline,
    Linear,
    Flat,
    Step,
    StepNext,
    Clamped,
    Plateau,
    Fixed,
}

impl Tangent {
    pub const NAMES: &'static [&'static str] = &["auto", "spline", "linear", "flat", "step", "stepnext", "clamped", "plateau", "fixed"];

    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }

    pub fn parse(s: &str) -> Option<Tangent> {
        use Tangent::*;
        Some(match s {
            "auto" => Auto,
            "spline" | "smooth" => Spline,
            "linear" => Linear,
            "flat" => Flat,
            "step" => Step,
            "stepnext" => StepNext,
            "clamped" => Clamped,
            "plateau" => Plateau,
            "fixed" => Fixed,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    /// Frame.
    pub t: f64,
    pub v: f64,
    pub itt: Tangent,
    pub ott: Tangent,
    /// Slopes (value per frame) of fixed tangents.
    pub in_slope: f64,
    pub out_slope: f64,
    pub breakdown: bool,
}

impl Key {
    pub fn new(t: f64, v: f64, itt: Tangent, ott: Tangent) -> Key {
        Key { t, v, itt, ott, in_slope: 0.0, out_slope: 0.0, breakdown: false }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Keys {
    pub keys: Vec<Key>,
}

impl crate::plug::Data for Keys {
    fn kind(&self) -> &str {
        "animKeys"
    }
    fn to_json(&self) -> Value {
        let k: Vec<Value> = self
            .keys
            .iter()
            .map(|k| json!([k.t, k.v, k.itt.name(), k.ott.name(), k.in_slope, k.out_slope, k.breakdown]))
            .collect();
        json!({"kind": "animKeys", "keys": k})
    }
    fn bytes(&self) -> usize {
        self.keys.len() * std::mem::size_of::<Key>()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Keys {
    pub fn from_json(j: &Value) -> Keys {
        let mut keys = Vec::new();
        for e in j.get("keys").and_then(Value::as_array).into_iter().flatten() {
            let f = |i: usize| e.get(i).and_then(Value::as_f64).unwrap_or(0.0);
            let t = |i: usize| e.get(i).and_then(Value::as_str).and_then(Tangent::parse).unwrap_or(Tangent::Auto);
            keys.push(Key {
                t: f(0),
                v: f(1),
                itt: t(2),
                ott: t(3),
                in_slope: f(4),
                out_slope: f(5),
                breakdown: e.get(6).and_then(Value::as_bool).unwrap_or(false),
            });
        }
        Keys { keys }
    }

    pub fn sort(&mut self) {
        self.keys.sort_by(|a, b| a.t.total_cmp(&b.t));
        self.keys.dedup_by(|b, a| (a.t - b.t).abs() < 1e-9);
    }

    /// Sets or replaces the key at t.
    pub fn set(&mut self, k: Key) {
        match self.keys.iter_mut().find(|e| (e.t - k.t).abs() < 1e-6) {
            Some(e) => {
                e.v = k.v;
            }
            None => {
                self.keys.push(k);
                self.sort();
            }
        }
    }

    /// (in, out) slopes of key i, value per frame.
    pub fn slopes(&self, i: usize) -> (f64, f64) {
        let k = &self.keys[i];
        let prev = i.checked_sub(1).map(|j| &self.keys[j]);
        let next = self.keys.get(i + 1);
        let seg = |a: &Key, b: &Key| if (b.t - a.t).abs() < 1e-12 { 0.0 } else { (b.v - a.v) / (b.t - a.t) };
        let left = prev.map(|p| seg(p, k));
        let right = next.map(|n| seg(k, n));
        let smooth = match (prev, next) {
            (Some(p), Some(n)) => seg(p, n),
            _ => left.or(right).unwrap_or(0.0),
        };
        let auto = || -> f64 {
            let (Some(p), Some(n)) = (prev, next) else { return 0.0 };
            // Flat at peaks and valleys; never overshooting the neighbours otherwise.
            if (k.v - p.v) * (n.v - k.v) <= 0.0 {
                return 0.0;
            }
            let m = smooth;
            let lim = 3.0 * left.unwrap().abs().min(right.unwrap().abs());
            m.clamp(-lim, lim)
        };
        let clamped = || -> f64 {
            let near = |o: Option<&Key>| o.is_some_and(|o| (o.v - k.v).abs() <= 1e-3 * (1.0 + k.v.abs()));
            if near(prev) || near(next) {
                0.0
            } else {
                smooth
            }
        };
        let one = |t: Tangent, fixed: f64, side: Option<f64>| -> f64 {
            match t {
                Tangent::Auto | Tangent::Plateau => auto(),
                Tangent::Spline => smooth,
                Tangent::Clamped => clamped(),
                Tangent::Linear => side.unwrap_or(0.0),
                Tangent::Flat | Tangent::Step | Tangent::StepNext => 0.0,
                Tangent::Fixed => fixed,
            }
        };
        (one(k.itt, k.in_slope, left.or(right)), one(k.ott, k.out_slope, right.or(left)))
    }

    fn eval_inside(&self, t: f64) -> f64 {
        let ks = &self.keys;
        let i = ks.partition_point(|k| k.t <= t);
        if i == 0 {
            return ks[0].v;
        }
        if i >= ks.len() {
            return ks[ks.len() - 1].v;
        }
        let (a, b) = (&ks[i - 1], &ks[i]);
        if a.ott == Tangent::Step {
            return a.v;
        }
        if a.ott == Tangent::StepNext {
            return b.v;
        }
        let h = b.t - a.t;
        if h <= 1e-12 {
            return b.v;
        }
        let m0 = self.slopes(i - 1).1;
        let m1 = self.slopes(i).0;
        let s = (t - a.t) / h;
        let (s2, s3) = (s * s, s * s * s);
        (2.0 * s3 - 3.0 * s2 + 1.0) * a.v + (s3 - 2.0 * s2 + s) * h * m0 + (-2.0 * s3 + 3.0 * s2) * b.v + (s3 - s2) * h * m1
    }

    /// The value at frame t, with the curve's infinities before and after its keys.
    pub fn eval(&self, t: f64, pre: Infinity, post: Infinity) -> f64 {
        let ks = &self.keys;
        if ks.is_empty() {
            return 0.0;
        }
        let (t0, t1) = (ks[0].t, ks[ks.len() - 1].t);
        let span = t1 - t0;
        if t >= t0 && t <= t1 || span <= 1e-9 {
            return self.eval_inside(t.clamp(t0, t1));
        }
        let (inf, before) = if t < t0 { (pre, true) } else { (post, false) };
        let (v0, v1) = (ks[0].v, ks[ks.len() - 1].v);
        match inf {
            Infinity::Constant => {
                if before {
                    v0
                } else {
                    v1
                }
            }
            Infinity::Linear => {
                if before {
                    v0 + self.slopes(0).0 * (t - t0)
                } else {
                    v1 + self.slopes(ks.len() - 1).1 * (t - t1)
                }
            }
            Infinity::Cycle | Infinity::CycleRelative | Infinity::Oscillate => {
                let n = ((t - t0) / span).floor();
                let mut local = t - n * span;
                if inf == Infinity::Oscillate && (n as i64).rem_euclid(2) == 1 {
                    local = t1 - (local - t0);
                }
                let v = self.eval_inside(local);
                if inf == Infinity::CycleRelative {
                    v + n * (v1 - v0)
                } else {
                    v
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Infinity {
    Constant,
    Linear,
    Cycle,
    CycleRelative,
    Oscillate,
}

pub const INFINITY_NAMES: &[&str] = &["constant", "linear", "constant", "cycle", "cycleRelative", "oscillate"];

fn infinity(i: i64) -> Infinity {
    match i {
        1 => Infinity::Linear,
        3 => Infinity::Cycle,
        4 => Infinity::CycleRelative,
        5 => Infinity::Oscillate,
        _ => Infinity::Constant,
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Curve nodes

pub const CURVE_TYPES: &[&str] = &["animCurveTL", "animCurveTA", "animCurveTU"];
pub const KEYS: usize = 0;
pub const PRE: usize = 1;
pub const POST: usize = 2;
pub const OUTPUT: usize = 3;

pub struct CurveNode;

impl NodeBehavior for CurveNode {
    fn compute(&mut self, ctx: &EvalContext, d: &mut NodeData) {
        let v = match d.value(KEYS) {
            PlugValue::Data(Some(k)) => k.as_any().downcast_ref::<Keys>().map(|k| {
                let f = ctx.time * fps();
                let r = f.round();
                let f = if (f - r).abs() < 1e-6 { r } else { f };
                k.eval(f, infinity(d.int(PRE)), infinity(d.int(POST)))
            }),
            _ => None,
        };
        d.set_output(OUTPUT, PlugValue::Float(v.unwrap_or(0.0)));
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub fn register_types() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(register_types_now);
}

fn register_types_now() {
    crate::plug::data::register_kind("animKeys", Arc::new(|j| Ok(Arc::new(Keys::from_json(j)) as Arc<dyn crate::plug::Data>)));
    let inf = |name: &str, short: &str, doc: &str| AttrDef {
        name: name.into(),
        short_name: short.into(),
        ty: PlugType::Enum,
        default_value: PlugValue::Int(0),
        flags: flags::INPUT | flags::STORABLE,
        enum_names: INFINITY_NAMES.iter().map(|s| s.to_string()).collect(),
        doc: doc.into(),
        ..Default::default()
    };
    let base = NodeType {
        name: "animCurve".into(),
        icon: "chart-line".into(),
        doc: "Keys over time driving one channel.".into(),
        time_dependent: true,
        own_attrs: vec![
            AttrDef {
                name: "keys".into(),
                short_name: "ktv".into(),
                ty: PlugType::Data,
                default_value: PlugValue::Data(None),
                flags: flags::INPUT | flags::STORABLE | flags::HIDDEN,
                doc: "The keys: time, value and tangents.".into(),
                ..Default::default()
            },
            inf("preInfinity", "pre", "What the curve does before its first key."),
            inf("postInfinity", "pst", "What the curve does after its last key."),
            AttrDef {
                name: "output".into(),
                short_name: "o".into(),
                ty: PlugType::Float,
                default_value: PlugValue::Float(0.0),
                flags: flags::OUTPUT,
                doc: "The value at the current time.".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let base = types::register_type(base).expect("animCurve");
    for (name, doc) in [
        ("animCurveTL", "Keys on a distance channel (translate)."),
        ("animCurveTA", "Keys on an angle channel (rotate), in degrees."),
        ("animCurveTU", "Keys on a unitless channel (scale, visibility, custom)."),
    ] {
        let t = NodeType {
            name: name.into(),
            base: Some(base.clone()),
            time_dependent: true,
            doc: doc.into(),
            create: Some(Arc::new(|| Box::new(CurveNode))),
            ..Default::default()
        };
        types::register_type(t).expect(name);
    }
}

pub fn is_curve(g: &Graph, id: NodeId) -> bool {
    g.node(id).is_some_and(|n| n.ty().is_a("animCurve"))
}

pub fn keys_of(g: &Graph, curve: NodeId) -> Keys {
    match g.node(curve).map(|n| n.data().value(KEYS)) {
        Some(PlugValue::Data(Some(k))) => k.as_any().downcast_ref::<Keys>().cloned().unwrap_or_default(),
        _ => Keys::default(),
    }
}

pub fn set_keys(g: &mut Graph, curve: NodeId, k: Keys) -> CmdResult<()> {
    g.set_value(PlugRef::new(curve, KEYS as i32), PlugValue::Data(Some(Arc::new(k))))?;
    Ok(())
}

/// The curve driving a plug, if any.
pub fn curve_of(g: &Graph, p: &PlugRef) -> Option<NodeId> {
    let s = g.source(p)?;
    (is_curve(g, s.node) && s.attr == OUTPUT as i32).then_some(s.node)
}

/// The channels a curve drives.
pub fn driven_by(g: &Graph, curve: NodeId) -> Vec<PlugRef> {
    g.destinations(&PlugRef::new(curve, OUTPUT as i32))
}

/// Every (plug, curve) pair animating a node.
pub fn curves_on(g: &Graph, node: NodeId) -> Vec<(PlugRef, NodeId)> {
    let mut out: Vec<(PlugRef, NodeId)> = g
        .connections()
        .into_iter()
        .filter(|(d, s)| d.node == node && is_curve(g, s.node) && s.attr == OUTPUT as i32)
        .map(|(d, s)| (d, s.node))
        .collect();
    out.sort_by_key(|(d, _)| (d.attr, d.component));
    out
}

/// A transform's keyable channels, as Maya keys them with S.
pub fn keyable_plugs(g: &Graph, node: NodeId) -> Vec<PlugRef> {
    let Some(n) = g.node(node) else { return Vec::new() };
    let mut out = Vec::new();
    for (i, a) in n.ty().attrs.iter().enumerate() {
        if !a.has(flags::KEYABLE) || !a.has(flags::INPUT) || a.has(flags::HIDDEN) || a.is_array() {
            continue;
        }
        match a.ty {
            PlugType::Vec3 => (0..3).for_each(|c| out.push(PlugRef::component(node, i as i32, c))),
            PlugType::Float | PlugType::Int | PlugType::Bool | PlugType::Enum => out.push(PlugRef::new(node, i as i32)),
            _ => {}
        }
    }
    out
}

fn curve_type_for(g: &Graph, p: &PlugRef) -> &'static str {
    let a = g.attr(p);
    if a.name == "rotate" || a.name.starts_with("rotate") && a.ty == PlugType::Float {
        "animCurveTA"
    } else if a.name == "translate" {
        "animCurveTL"
    } else {
        "animCurveTU"
    }
}

fn channel_name(g: &Graph, p: &PlugRef) -> String {
    let a = g.attr(p);
    if p.component >= 0 && a.ty == PlugType::Vec3 {
        a.child_names[p.component as usize].clone()
    } else {
        a.name.clone()
    }
}

fn plug_scalar(g: &mut Graph, p: &PlugRef) -> f64 {
    match g.value(p) {
        Ok(v) => v.scalar().unwrap_or(0.0),
        Err(_) => 0.0,
    }
}

fn steps_only(g: &Graph, p: &PlugRef) -> bool {
    matches!(g.attr(p).ty, PlugType::Bool | PlugType::Enum) && p.component < 0
}

/// Sets a key on a plug at frame t (value: the plug's current one when None). Makes and connects the curve if
/// needed. Returns false when the plug can't be keyed (locked, or driven by something else).
pub fn key_plug(g: &mut Graph, p: &PlugRef, t: f64, value: Option<f64>, itt: Option<Tangent>, ott: Option<Tangent>) -> CmdResult<bool> {
    if g.is_locked(p) {
        return Ok(false);
    }
    let v = match value {
        Some(v) => v,
        None => plug_scalar(g, p),
    };
    let r = range();
    let stepped = steps_only(g, p);
    let itt = itt.unwrap_or(if stepped { Tangent::Step } else { r.default_in });
    let ott = ott.unwrap_or(if stepped { Tangent::Step } else { r.default_out });
    let curve = match curve_of(g, p) {
        Some(c) => c,
        None => {
            if g.source(p).is_some() || (p.component >= 0 && g.source(&p.whole()).is_some()) {
                return Ok(false);
            }
            let ty = types::find(curve_type_for(g, p)).unwrap();
            let name = format!("{}_{}", g.node(p.node).map(|n| n.name.clone()).unwrap_or_default(), channel_name(g, p));
            let c = g.create_node(&ty, &name, NO_NODE, true)?;
            g.connect(&PlugRef::new(c, OUTPUT as i32), p, false)?;
            c
        }
    };
    let mut k = keys_of(g, curve);
    match k.keys.iter_mut().find(|e| (e.t - t).abs() < 1e-6) {
        Some(e) => e.v = v,
        None => k.set(Key::new(t, v, itt, ott)),
    }
    set_keys(g, curve, k)?;
    Ok(true)
}

/// Turns the plug arguments of a key command into plugs: "node.attr" as named, a node as its keyable channels (or the
/// -attribute ones).
fn targets(s: &Session, a: &aztek_ops::Args, cmd: &str) -> CmdResult<Vec<PlugRef>> {
    let g = &s.graph;
    let attrs: Vec<String> = match a.get("attribute") {
        Some(Value::String(x)) => vec![x.clone()],
        Some(Value::Array(v)) => v.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    };
    let mut out = Vec::new();
    for name in objects(s, a) {
        if name.contains('.') && !name.ends_with(']') {
            out.push(g.plug(&name)?);
            continue;
        }
        let base = name.split('.').next().unwrap_or(&name);
        let Some(id) = g.find(base)? else { fail!("{cmd}: no object named {name}") };
        // Components and shapes key their transform.
        let id = if g.node(id).is_some_and(|n| n.ty().dag && !n.ty().is_a("transform")) { crate::dag::transform_of(g, id) } else { id };
        if attrs.is_empty() {
            out.extend(keyable_plugs(g, id));
        } else {
            for at in &attrs {
                let path = format!("{}.{}", g.path(id), at);
                match g.plug(&path) {
                    Ok(p) => {
                        if g.attr(&p).ty == PlugType::Vec3 && p.component < 0 {
                            (0..3).for_each(|c| out.push(PlugRef::component(p.node, p.attr, c)));
                        } else {
                            out.push(p);
                        }
                    }
                    Err(e) => fail!("{cmd}: {e}"),
                }
            }
        }
    }
    Ok(out)
}

/// A -time flag: a frame, "10:20", "10:" or ":20", or [start, end]. None means all time.
fn time_range(a: &aztek_ops::Args) -> CmdResult<Option<(f64, f64)>> {
    let Some(v) = a.get("time") else { return Ok(None) };
    let parse = |s: &str| -> CmdResult<(f64, f64)> {
        let s = s.trim().trim_matches(|c| c == '"' || c == '(' || c == ')');
        if let Some((x, y)) = s.split_once(':') {
            let x = if x.trim().is_empty() { f64::NEG_INFINITY } else { x.trim().parse::<f64>().map_err(|_| aztek_ops::CommandError::new(format!("bad time {s}")))? };
            let y = if y.trim().is_empty() { f64::INFINITY } else { y.trim().parse::<f64>().map_err(|_| aztek_ops::CommandError::new(format!("bad time {s}")))? };
            Ok((x, y))
        } else {
            let t = s.parse::<f64>().map_err(|_| aztek_ops::CommandError::new(format!("bad time {s}")))?;
            Ok((t, t))
        }
    };
    Ok(Some(match v {
        Value::Number(n) => {
            let t = n.as_f64().unwrap_or(0.0);
            (t, t)
        }
        Value::String(s) => parse(s)?,
        Value::Array(x) if x.len() == 2 && x.iter().all(Value::is_number) => (x[0].as_f64().unwrap(), x[1].as_f64().unwrap()),
        Value::Array(x) if x.len() == 1 => match &x[0] {
            Value::String(s) => parse(s)?,
            Value::Array(y) if y.len() == 2 => (y[0].as_f64().unwrap_or(0.0), y[1].as_f64().unwrap_or(0.0)),
            Value::Number(n) => (n.as_f64().unwrap_or(0.0), n.as_f64().unwrap_or(0.0)),
            _ => fail!("bad time {v}"),
        },
        _ => fail!("bad time {v}"),
    }))
}

fn in_range(r: Option<(f64, f64)>, t: f64) -> bool {
    r.is_none_or(|(a, b)| t >= a - 1e-6 && t <= b + 1e-6)
}

fn queried(a: &aztek_ops::Args, names: &[&str]) -> bool {
    let q: Vec<&str> = a.get("queried").and_then(Value::as_array).map(|v| v.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    names.iter().any(|n| q.contains(n) || a.get(n).is_some_and(|v| v == &Value::Bool(true)))
}

fn tangent_flag(a: &aztek_ops::Args, name: &str) -> CmdResult<Option<Tangent>> {
    match a.get(name).and_then(Value::as_str) {
        None => Ok(None),
        Some(s) => match Tangent::parse(s) {
            Some(t) => Ok(Some(t)),
            None => fail!("unknown tangent type '{s}'"),
        },
    }
}

/// The curves of the targets, each with the channel it drives.
fn target_curves(s: &Session, a: &aztek_ops::Args, cmd: &str) -> CmdResult<Vec<(PlugRef, NodeId)>> {
    let g = &s.graph;
    let mut out = Vec::new();
    // Curves named directly.
    let names = objects(s, a);
    if names.iter().all(|n| g.find(n).ok().flatten().is_some_and(|id| is_curve(g, id))) && !names.is_empty() {
        for n in names {
            let id = g.find(&n)?.unwrap();
            let p = driven_by(g, id).first().copied().unwrap_or(PlugRef::new(id, OUTPUT as i32));
            out.push((p, id));
        }
        return Ok(out);
    }
    for p in targets(s, a, cmd)? {
        if let Some(c) = curve_of(g, &p) {
            out.push((p, c));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------------------------
// Hooks

/// A set on an animated channel: autokey keys it; otherwise the value holds until the time changes, as in Maya.
pub fn install(g: &mut Graph) {
    set_frame(g, range().min);
    g.set_driven_set_hook(Arc::new(|g: &mut Graph, dst: &PlugRef, src: &PlugRef, v: f64| {
        if let Some(edit) = crate::rig::driven_scalar_set(g, dst, src, v) {
            return edit;
        }
        if !is_curve(g, src.node) {
            return DrivenEdit::Reject;
        }
        if auto_key() {
            let t = frame(g);
            let mut k = keys_of(g, src.node);
            match k.keys.iter_mut().find(|e| (e.t - t).abs() < 1e-6) {
                Some(e) => e.v = v,
                None => {
                    let r = range();
                    k.set(Key::new(t, v, r.default_in, r.default_out));
                }
            }
            if set_keys(g, src.node, k).is_ok() {
                let _ = dst;
                return DrivenEdit::Handled;
            }
        }
        DrivenEdit::Hold
    }));
}

/// Curves left driving nothing after their channels' nodes were deleted, as Maya removes them.
pub fn orphans(g: &Graph, doomed: &[NodeId]) -> Vec<NodeId> {
    let mut out = Vec::new();
    for (d, s) in g.connections() {
        if doomed.contains(&d.node) && is_curve(g, s.node) && !doomed.contains(&s.node) && !out.contains(&s.node) {
            if driven_by(g, s.node).iter().all(|p| doomed.contains(&p.node)) {
                out.push(s.node);
            }
        }
    }
    out
}

/// Key times on these nodes' curves, sorted and unique.
pub fn key_times(g: &Graph, nodes: &[NodeId]) -> Vec<f64> {
    let mut t: Vec<f64> = Vec::new();
    for &n in nodes {
        for (_, c) in curves_on(g, n) {
            t.extend(keys_of(g, c).keys.iter().map(|k| k.t));
        }
    }
    t.sort_by(f64::total_cmp);
    t.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    t
}

// ---------------------------------------------------------------------------------------------------------------------
// Commands

type Cmd = CommandDef<Session>;

fn f(name: &str, short: &str, ty: ArgType, doc: &str) -> FlagDef {
    FlagDef::new(name, short, ty, Value::Null, doc)
}

fn query_flags(c: Cmd) -> Cmd {
    c.flag(f("query", "q", ArgType::Bool, "Return the value of the flags named instead of setting them."))
        .flag(f("edit", "e", ArgType::Bool, "Change the flags named."))
        .flag(f("queried", "", ArgType::List, "The flags asked about in query mode (filled in from q=True calls)."))
}

pub fn register(c: &mut Commands<Session>) {
    register_types();
    register_edit_keys(c);

    c.register(
        query_flags(Cmd::new(
            "currentTime",
            "Set the current frame, or query it. Animated channels move to their values there.",
            |s, a| {
                if !a.bool("query") {
                    let t = match a.positional.first().and_then(Value::as_f64).or_else(|| a.get("time").and_then(Value::as_f64)) {
                        Some(t) => t,
                        None => fail!("currentTime: expected a frame"),
                    };
                    set_frame(&mut s.graph, t);
                }
                Ok(frame_json(frame(&s.graph)))
            },
        ))
        .positional("frame")
        .flag(f("time", "t", ArgType::Float, "The frame to go to."))
        .flag(f("update", "u", ArgType::Bool, "Accepted for Maya scripts; the scene always updates."))
        .undoable(false),
    );

    c.register(
        query_flags(Cmd::new(
            "playbackOptions",
            "The playback range (min/max), the whole animation range (ast/aet), looping and speed. In query mode returns the \
             flag asked about.",
            |_s, a| {
                if a.bool("query") {
                    let r = range();
                    return Ok(if queried(a, &["maxTime"]) {
                        frame_json(r.max)
                    } else if queried(a, &["animationStartTime"]) {
                        frame_json(r.anim_start)
                    } else if queried(a, &["animationEndTime"]) {
                        frame_json(r.anim_end)
                    } else if queried(a, &["loop"]) {
                        json!(r.loop_mode)
                    } else if queried(a, &["playbackSpeed"]) {
                        json!(r.speed)
                    } else if queried(a, &["framesPerSecond"]) {
                        json!(fps())
                    } else {
                        frame_json(r.min)
                    });
                }
                set_range(|r| {
                    if let Some(v) = a.get("minTime").and_then(Value::as_f64) {
                        r.min = v;
                    }
                    if let Some(v) = a.get("maxTime").and_then(Value::as_f64) {
                        r.max = v;
                    }
                    if let Some(v) = a.get("animationStartTime").and_then(Value::as_f64) {
                        r.anim_start = v;
                    }
                    if let Some(v) = a.get("animationEndTime").and_then(Value::as_f64) {
                        r.anim_end = v;
                    }
                    if let Some(v) = a.get("loop").and_then(Value::as_str) {
                        r.loop_mode = v.to_string();
                    }
                    if let Some(v) = a.get("playbackSpeed").and_then(Value::as_f64) {
                        r.speed = v;
                    }
                    r.anim_start = r.anim_start.min(r.min);
                    r.anim_end = r.anim_end.max(r.max);
                });
                if let Some(v) = a.get("framesPerSecond").and_then(Value::as_f64) {
                    if v > 0.0 {
                        FPS_BITS.store(v.to_bits(), Ordering::Relaxed);
                    }
                }
                Ok(Value::Null)
            },
        ))
        .flag(f("minTime", "min", ArgType::Float, "First frame of the playback range."))
        .flag(f("maxTime", "max", ArgType::Float, "Last frame of the playback range."))
        .flag(f("animationStartTime", "ast", ArgType::Float, "First frame of the whole animation."))
        .flag(f("animationEndTime", "aet", ArgType::Float, "Last frame of the whole animation."))
        .flag(f("loop", "l", ArgType::String, "continuous, once or oscillate.").choices(&["continuous", "once", "oscillate"]))
        .flag(f("playbackSpeed", "ps", ArgType::Float, "1 for real time, 0 for every frame."))
        .flag(f("framesPerSecond", "fps", ArgType::Float, "Frames per second (24 by default, film)."))
        .undoable(false),
    );

    c.register(
        query_flags(Cmd::new(
            "autoKeyframe",
            "Auto key: changing an animated channel keys it at the current frame. Returns whether it is on.",
            |_s, a| {
                if !a.bool("query") {
                    if let Some(v) = a.get("state").and_then(Value::as_bool) {
                        AUTO_KEY.store(v, Ordering::Relaxed);
                    }
                }
                Ok(json!(auto_key()))
            },
        ))
        .flag(f("state", "st", ArgType::Bool, "On or off."))
        .undoable(false),
    );

    c.register(
        Cmd::new(
            "setKeyframe",
            "Key channels at the current frame (or -t). Objects key all their keyable channels unless -at names some. Returns \
             the number of keys set.",
            |s, a| {
                let plugs = targets(s, a, "setKeyframe")?;
                let times: Vec<f64> = match a.get("time") {
                    Some(Value::Array(v)) => v.iter().filter_map(Value::as_f64).collect(),
                    Some(v) => v.as_f64().into_iter().collect(),
                    None => vec![frame(&s.graph)],
                };
                let value = a.get("value").and_then(Value::as_f64);
                let (itt, ott) = (tangent_flag(a, "inTangentType")?, tangent_flag(a, "outTangentType")?);
                let mut n = 0;
                for p in plugs {
                    for &t in &times {
                        // A held (changed but unkeyed) value is what gets keyed at the current frame.
                        if key_plug(&mut s.graph, &p, t, value, itt, ott)? {
                            n += 1;
                        }
                    }
                }
                Ok(json!(n))
            },
        )
        .positional("objects or object.attribute plugs (the selection when none)")
        .flag(f("attribute", "at", ArgType::Any, "Channel name(s) to key: tx, rotateY..."))
        .flag(f("time", "t", ArgType::Any, "Frame(s) to key at."))
        .flag(f("value", "v", ArgType::Float, "Value to key instead of the current one."))
        .flag(f("inTangentType", "itt", ArgType::String, "Tangent into the key.").choices(Tangent::NAMES))
        .flag(f("outTangentType", "ott", ArgType::String, "Tangent out of the key.").choices(Tangent::NAMES))
        .flag(f("breakdown", "bd", ArgType::Bool, "Accepted for Maya scripts."))
        .flag(f("hierarchy", "hi", ArgType::String, "Accepted for Maya scripts."))
        .flag(f("controlPoints", "cp", ArgType::Bool, "Accepted for Maya scripts."))
        .flag(f("shape", "s", ArgType::Bool, "Accepted for Maya scripts.")),
    );

    c.register(
        query_flags(Cmd::new(
            "keyframe",
            "Query or edit keys. -q -kc counts them, -q -tc lists their frames, -q -vc their values, -q -n the curves. -tc/-vc \
             (with -r for relative) move the keys in -t (or all).",
            |s, a| {
                let curves = target_curves(s, a, "keyframe")?;
                let range = time_range(a)?;
                let index: Option<Vec<usize>> = match a.get("index") {
                    Some(Value::Array(v)) => Some(v.iter().filter_map(Value::as_u64).map(|i| i as usize).collect()),
                    Some(Value::Number(n)) => Some(vec![n.as_u64().unwrap_or(0) as usize]),
                    _ => None,
                };
                let pick = |i: usize, k: &Key| in_range(range, k.t) && index.as_ref().is_none_or(|ix| ix.contains(&i));
                if a.bool("query") {
                    if queried(a, &["name"]) {
                        return Ok(json!(curves.iter().map(|(_, c)| s.graph.path(*c)).collect::<Vec<_>>()));
                    }
                    let mut count = 0;
                    let mut times = Vec::new();
                    let mut values = Vec::new();
                    let mut both = Vec::new();
                    for (_, c) in &curves {
                        for (i, k) in keys_of(&s.graph, *c).keys.iter().enumerate() {
                            if pick(i, k) {
                                count += 1;
                                times.push(json!(k.t));
                                values.push(json!(k.v));
                                both.push(json!(k.t));
                                both.push(json!(k.v));
                            }
                        }
                    }
                    if queried(a, &["keyframeCount"]) {
                        return Ok(json!(count));
                    }
                    if queried(a, &["valueChange"]) && queried(a, &["timeChange"]) {
                        return Ok(Value::Array(both));
                    }
                    if queried(a, &["valueChange"]) {
                        return Ok(Value::Array(values));
                    }
                    if queried(a, &["eval"]) {
                        let t = frame(&s.graph);
                        let v: Vec<Value> = curves.iter().map(|(_, c)| json!(keys_of(&s.graph, *c).eval(t, Infinity::Constant, Infinity::Constant))).collect();
                        return Ok(Value::Array(v));
                    }
                    return Ok(Value::Array(times));
                }
                let rel = a.bool("relative");
                let tc = a.get("timeChange").and_then(Value::as_f64);
                let vc = a.get("valueChange").and_then(Value::as_f64);
                let mut n = 0;
                for (_, c) in &curves {
                    let mut k = keys_of(&s.graph, *c);
                    for (i, key) in k.keys.iter_mut().enumerate() {
                        if !pick(i, key) {
                            continue;
                        }
                        if let Some(t) = tc {
                            key.t = if rel { key.t + t } else { t };
                        }
                        if let Some(v) = vc {
                            key.v = if rel { key.v + v } else { v };
                        }
                        n += 1;
                    }
                    k.keys.sort_by(|a, b| a.t.total_cmp(&b.t));
                    set_keys(&mut s.graph, *c, k)?;
                }
                Ok(json!(n))
            },
        ))
        .positional("objects, plugs or curves (the selection when none)")
        .flag(f("attribute", "at", ArgType::Any, "Channel name(s)."))
        .flag(f("time", "t", ArgType::Any, "A frame or range \"10:20\"."))
        .flag(f("index", "in", ArgType::Any, "Key index(es) on each curve."))
        .flag(f("keyframeCount", "kc", ArgType::Bool, "Query: how many keys."))
        .flag(f("timeChange", "tc", ArgType::Float, "Query: the keys' frames. Edit: their new frame (or offset with -r)."))
        .flag(f("valueChange", "vc", ArgType::Float, "Query: the keys' values. Edit: their new value (or offset with -r)."))
        .flag(f("relative", "r", ArgType::Bool, "Edits are offsets."))
        .flag(f("absolute", "a", ArgType::Bool, "Edits are new values (the default)."))
        .flag(f("option", "o", ArgType::String, "Accepted for Maya scripts."))
        .flag(f("name", "n", ArgType::Bool, "Query: the curves' names."))
        .flag(f("eval", "ev", ArgType::Bool, "Query: the curves' values at the current frame."))
        .flag(f("selected", "sl", ArgType::Bool, "Accepted for Maya scripts.")),
    );

    c.register(
        Cmd::new(
            "cutKey",
            "Delete keys in -t (or all of them) from objects or channels. Curves left with no keys are deleted, leaving the \
             channel at its value.",
            |s, a| {
                let curves = target_curves(s, a, "cutKey")?;
                let range = time_range(a)?;
                let mut n = 0;
                for (p, c) in curves {
                    let mut k = keys_of(&s.graph, c);
                    let before = k.keys.len();
                    k.keys.retain(|key| !in_range(range, key.t));
                    n += before - k.keys.len();
                    if k.keys.is_empty() {
                        let v = plug_scalar(&mut s.graph, &p);
                        s.graph.delete_node(c)?;
                        if p.node != c {
                            let _ = s.graph.set_value(p, PlugValue::Float(v));
                        }
                    } else {
                        set_keys(&mut s.graph, c, k)?;
                    }
                }
                Ok(json!(n))
            },
        )
        .positional("objects, plugs or curves (the selection when none)")
        .flag(f("attribute", "at", ArgType::Any, "Channel name(s)."))
        .flag(f("time", "t", ArgType::Any, "A frame or range \"10:20\"."))
        .flag(f("clear", "cl", ArgType::Bool, "Delete without copying (the only way here)."))
        .flag(f("option", "o", ArgType::String, "Accepted for Maya scripts."))
        .flag(f("index", "in", ArgType::Any, "Accepted for Maya scripts.")),
    );

    c.register(
        query_flags(Cmd::new(
            "keyTangent",
            "Set the tangent types of keys (-itt, -ott; -t limits them), their angles (-ia, -oa, degrees), or with -g the \
             defaults new keys get.",
            |s, a| {
                let (itt, ott) = (tangent_flag(a, "inTangentType")?, tangent_flag(a, "outTangentType")?);
                if a.bool("global") {
                    if a.bool("query") {
                        let r = range();
                        return Ok(json!(if queried(a, &["outTangentType"]) { r.default_out.name() } else { r.default_in.name() }));
                    }
                    set_range(|r| {
                        if let Some(t) = itt {
                            r.default_in = t;
                        }
                        if let Some(t) = ott {
                            r.default_out = t;
                        }
                    });
                    return Ok(Value::Null);
                }
                let curves = target_curves(s, a, "keyTangent")?;
                let range = time_range(a)?;
                if a.bool("query") {
                    let mut out = Vec::new();
                    for (_, c) in &curves {
                        let k = keys_of(&s.graph, *c);
                        for (i, key) in k.keys.iter().enumerate() {
                            if !in_range(range, key.t) {
                                continue;
                            }
                            let (si, so) = k.slopes(i);
                            if queried(a, &["inAngle"]) {
                                out.push(json!(si.atan().to_degrees()));
                            } else if queried(a, &["outAngle"]) {
                                out.push(json!(so.atan().to_degrees()));
                            } else if queried(a, &["outTangentType"]) {
                                out.push(json!(key.ott.name()));
                            } else {
                                out.push(json!(key.itt.name()));
                            }
                        }
                    }
                    return Ok(Value::Array(out));
                }
                let ia = a.get("inAngle").and_then(Value::as_f64);
                let oa = a.get("outAngle").and_then(Value::as_f64);
                let mut n = 0;
                for (_, c) in &curves {
                    let mut k = keys_of(&s.graph, *c);
                    let slopes: Vec<(f64, f64)> = (0..k.keys.len()).map(|i| k.slopes(i)).collect();
                    for (i, key) in k.keys.iter_mut().enumerate() {
                        if !in_range(range, key.t) {
                            continue;
                        }
                        if let Some(t) = itt {
                            key.itt = t;
                        }
                        if let Some(t) = ott {
                            key.ott = t;
                        }
                        if ia.is_some() || oa.is_some() || a.bool("lock") {
                            key.in_slope = ia.map(|d| d.to_radians().tan()).unwrap_or(slopes[i].0);
                            key.out_slope = oa.map(|d| d.to_radians().tan()).unwrap_or(slopes[i].1);
                            if a.bool("lock") {
                                if ia.is_some() {
                                    key.out_slope = key.in_slope;
                                } else if oa.is_some() {
                                    key.in_slope = key.out_slope;
                                }
                            }
                            key.itt = Tangent::Fixed;
                            key.ott = Tangent::Fixed;
                        }
                        n += 1;
                    }
                    set_keys(&mut s.graph, *c, k)?;
                }
                Ok(json!(n))
            },
        ))
        .positional("objects, plugs or curves (the selection when none)")
        .flag(f("attribute", "at", ArgType::Any, "Channel name(s)."))
        .flag(f("time", "t", ArgType::Any, "A frame or range \"10:20\"."))
        .flag(f("inTangentType", "itt", ArgType::String, "Tangent into the keys.").choices(Tangent::NAMES))
        .flag(f("outTangentType", "ott", ArgType::String, "Tangent out of the keys.").choices(Tangent::NAMES))
        .flag(f("inAngle", "ia", ArgType::Float, "In tangent angle, degrees (makes it fixed)."))
        .flag(f("outAngle", "oa", ArgType::Float, "Out tangent angle, degrees (makes it fixed)."))
        .flag(f("lock", "l", ArgType::Bool, "Keep in and out tangents together."))
        .flag(f("global", "g", ArgType::Bool, "The default tangents for new keys.")),
    );

    c.register(
        Cmd::new(
            "findKeyframe",
            "The frame of the next, previous, first or last key on the objects (the selection when none).",
            |s, a| {
                let g = &s.graph;
                let mut nodes = Vec::new();
                for n in objects(s, a) {
                    if let Some(id) = g.find(n.split('.').next().unwrap_or(&n))? {
                        nodes.push(id);
                    }
                }
                let times = key_times(g, &nodes);
                let now = a.get("time").and_then(Value::as_f64).unwrap_or_else(|| frame(g));
                let which = a.string("which", "next");
                let r = match which.as_str() {
                    "first" => times.first().copied(),
                    "last" => times.last().copied(),
                    "previous" => times.iter().rev().find(|&&t| t < now - 1e-6).copied().or(times.last().copied()),
                    _ => times.iter().find(|&&t| t > now + 1e-6).copied().or(times.first().copied()),
                };
                Ok(frame_json(r.unwrap_or(now)))
            },
        )
        .positional("objects")
        .flag(f("which", "w", ArgType::String, "next, previous, first or last.").choices(&["next", "previous", "first", "last"]))
        .flag(f("time", "t", ArgType::Float, "Search from this frame instead of the current one."))
        .undoable(false),
    );

    c.register(
        Cmd::new(
            "keyTimes",
            "Frames that have keys on the objects (the selection when none), for the time slider.",
            |s, a| {
                let g = &s.graph;
                let mut nodes = Vec::new();
                for n in objects(s, a) {
                    if let Some(id) = g.find(n.split('.').next().unwrap_or(&n))? {
                        nodes.push(id);
                    }
                }
                Ok(json!(key_times(g, &nodes)))
            },
        )
        .positional("objects")
        .undoable(false),
    );
}

/// The graph editor's edits on picked keys: [[curve, index], ...] moved, set, retangented or deleted in one undo step.
fn register_edit_keys(c: &mut Commands<Session>) {
    c.register(
        Cmd::new(
            "editKeys",
            "Edit keys picked by curve and index: -dt/-dv move them, -t/-v set them, -itt/-ott change their tangents, -delete \
             removes them. Keys are [[curve, index], ...].",
            |s, a| {
                let Some(list) = a.get("keys").and_then(Value::as_array) else { fail!("editKeys: -keys [[curve, index], ...]") };
                let mut by_curve: Vec<(NodeId, Vec<usize>)> = Vec::new();
                for e in list {
                    let (Some(name), Some(i)) = (e.get(0).and_then(Value::as_str), e.get(1).and_then(Value::as_u64)) else {
                        fail!("editKeys: bad key {e}")
                    };
                    let Some(id) = s.graph.find(name)? else { fail!("editKeys: no curve named {name}") };
                    if !is_curve(&s.graph, id) {
                        fail!("editKeys: {name} is not an animation curve");
                    }
                    match by_curve.iter_mut().find(|(c, _)| *c == id) {
                        Some((_, v)) => v.push(i as usize),
                        None => by_curve.push((id, vec![i as usize])),
                    }
                }
                let dt = a.float("timeOffset", 0.0);
                let dv = a.float("valueOffset", 0.0);
                let (itt, ott) = (tangent_flag(a, "inTangentType")?, tangent_flag(a, "outTangentType")?);
                let mut n = 0;
                for (curve, idx) in by_curve {
                    let mut k = keys_of(&s.graph, curve);
                    if a.bool("delete") {
                        let mut i = 0;
                        k.keys.retain(|_| {
                            let keep = !idx.contains(&i);
                            i += 1;
                            keep
                        });
                        n += idx.len();
                        if k.keys.is_empty() {
                            let drives = driven_by(&s.graph, curve);
                            let vals: Vec<(PlugRef, f64)> = drives.iter().map(|p| (*p, plug_scalar(&mut s.graph, p))).collect();
                            s.graph.delete_node(curve)?;
                            for (p, v) in vals {
                                let _ = s.graph.set_value(p, PlugValue::Float(v));
                            }
                            continue;
                        }
                        set_keys(&mut s.graph, curve, k)?;
                        continue;
                    }
                    for &i in &idx {
                        let Some(key) = k.keys.get_mut(i) else { continue };
                        key.t = a.get("time").and_then(Value::as_f64).unwrap_or(key.t + dt);
                        key.v = a.get("value").and_then(Value::as_f64).unwrap_or(key.v + dv);
                        if let Some(t) = itt {
                            key.itt = t;
                        }
                        if let Some(t) = ott {
                            key.ott = t;
                        }
                        n += 1;
                    }
                    // Keys passing others replace them, as in Maya.
                    let moved: Vec<Key> = idx.iter().filter_map(|&i| k.keys.get(i).copied()).collect();
                    let mut rest: Vec<Key> = k.keys.iter().enumerate().filter(|(i, _)| !idx.contains(i)).map(|(_, k)| *k).collect();
                    rest.retain(|r| !moved.iter().any(|m| (m.t - r.t).abs() < 1e-6));
                    rest.extend(moved);
                    k.keys = rest;
                    k.keys.sort_by(|a, b| a.t.total_cmp(&b.t));
                    set_keys(&mut s.graph, curve, k)?;
                }
                Ok(json!(n))
            },
        )
        .flag(f("keys", "k", ArgType::List, "The keys: [[curve, index], ...]."))
        .flag(f("timeOffset", "dt", ArgType::Float, "Frames to move them by."))
        .flag(f("valueOffset", "dv", ArgType::Float, "Value to move them by."))
        .flag(f("time", "t", ArgType::Float, "Frame to put them at."))
        .flag(f("value", "v", ArgType::Float, "Value to give them."))
        .flag(f("inTangentType", "itt", ArgType::String, "Tangent into them.").choices(Tangent::NAMES))
        .flag(f("outTangentType", "ott", ArgType::String, "Tangent out of them.").choices(Tangent::NAMES))
        .flag(f("delete", "del", ArgType::Bool, "Remove them.")),
    );
}
