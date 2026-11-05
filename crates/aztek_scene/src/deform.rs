//! Deformers as Maya makes them from the Deform menu: nonlinear deformers (bend, twist, squash, flare, sine, wave)
//! with their handles, clusters, and blend shapes. Each sits in the mesh's history like a skinCluster.

use std::any::Any;
use std::sync::Arc;

use aztek_geo::Mesh;
use aztek_ops::{fail, objects, ArgType, CmdResult, CommandDef, Commands, FlagDef};
use glam::{DMat4, DVec3};
use serde_json::{json, Value};

use crate::attr_defs::{float_attr_short, mesh_attr};
use crate::components::{self, ComponentMode};
use crate::graph::{Graph, SelectMode};
use crate::node::{types, EvalContext, NodeBehavior, NodeData, NodeType};
use crate::plug::{flags, AttrDef, NodeId, PlugRef, PlugType, PlugValue, NO_NODE};
use crate::rig::{world, JsonData};
use crate::session::Session;
use crate::transform::{Transform, TransformBehavior};

fn plug(n: NodeId, a: usize) -> PlugRef {
    PlugRef::new(n, a as i32)
}

fn data_attr(name: &str, short: &str, doc: &str) -> AttrDef {
    AttrDef {
        name: name.into(),
        short_name: short.into(),
        ty: PlugType::Data,
        default_value: PlugValue::Data(None),
        flags: flags::INPUT | flags::STORABLE | flags::HIDDEN,
        doc: doc.into(),
        ..Default::default()
    }
}

fn matrix_in(name: &str, short: &str, doc: &str) -> AttrDef {
    AttrDef {
        name: name.into(),
        short_name: short.into(),
        ty: PlugType::Matrix,
        default_value: PlugValue::Matrix(DMat4::IDENTITY),
        flags: flags::INPUT | flags::HIDDEN,
        doc: doc.into(),
        ..Default::default()
    }
}

fn json_of(d: &NodeData, attr: usize) -> Value {
    match d.value(attr) {
        PlugValue::Data(Some(x)) => x.as_any().downcast_ref::<JsonData>().map(|j| j.0.clone()).unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

fn mat_json(m: &DMat4) -> Value {
    json!(m.to_cols_array().to_vec())
}

fn mat_from(v: &Value) -> DMat4 {
    let a: Vec<f64> = v.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
    if a.len() == 16 {
        DMat4::from_cols_array(&a.try_into().unwrap())
    } else {
        DMat4::IDENTITY
    }
}

// Shared deformer plugs.
const IN: usize = 0;
const OUT: usize = 1;
const ENVELOPE: usize = 2;
const DATA: usize = 3;
const HANDLE: usize = 4;
const BASE_COUNT: usize = 5;

pub const NONLINEAR: &[&str] = &["bend", "twist", "squash", "flare", "sine", "wave"];

pub fn is_deformer(type_name: &str) -> bool {
    NONLINEAR.contains(&type_name) || matches!(type_name, "cluster" | "blendShape")
}

/// The mesh's points through f in the handle's space: p -> G^-1 H f(H^-1 G p).
fn through_handle(m: &mut Mesh, geom: &DMat4, handle: &DMat4, env: f64, weights: Option<&[f64]>, f: impl Fn(DVec3) -> DVec3) {
    let to_h = handle.inverse() * *geom;
    let back = geom.inverse() * *handle;
    for p in 0..m.point_capacity() {
        if !m.point_valid(p) {
            continue;
        }
        let w = weights.map(|w| w.get(p as usize).copied().unwrap_or(0.0)).unwrap_or(1.0) * env;
        if w == 0.0 {
            continue;
        }
        let rest = m.position(p);
        let moved = back.transform_point3(f(to_h.transform_point3(rest)));
        m.set_position(p, rest + (moved - rest) * w);
    }
}

pub struct NonLinear {
    kind: usize,
}

// Nonlinear attributes after the shared ones.
const NL_A: usize = BASE_COUNT;
const NL_B: usize = BASE_COUNT + 1;
const NL_C: usize = BASE_COUNT + 2;
const NL_LOW: usize = BASE_COUNT + 3;
const NL_HIGH: usize = BASE_COUNT + 4;
const NL_D: usize = BASE_COUNT + 5;

fn nonlinear(kind: usize, a: f64, b: f64, c: f64, d: f64, lo: f64, hi: f64, p: DVec3) -> DVec3 {
    let span = (hi - lo).max(1e-9);
    match kind {
        0 => {
            // bend: curvature a (radians per unit) about Z.
            let curv = a;
            if curv.abs() < 1e-9 {
                return p;
            }
            let r = 1.0 / curv;
            let yc = p.y.clamp(lo, hi);
            let th = curv * yc;
            let (s, co) = th.sin_cos();
            let centre = DVec3::new(r - r * co, r * s, 0.0);
            let n = DVec3::new(co, -s, 0.0);
            let t = DVec3::new(s, co, 0.0);
            centre + n * p.x + t * (p.y - yc) + DVec3::Z * p.z
        }
        1 => {
            // twist: start angle a at the low bound to end angle b at the high one, degrees, about Y.
            let t = ((p.y - lo) / span).clamp(0.0, 1.0);
            let ang = (a + (b - a) * t).to_radians();
            let (s, co) = ang.sin_cos();
            DVec3::new(p.x * co + p.z * s, p.y, -p.x * s + p.z * co)
        }
        2 => {
            // squash: factor a along Y, bulging by expand b in XZ.
            let t = ((p.y - lo) / span).clamp(0.0, 1.0);
            let sy = 1.0 + a;
            let mid = (lo + hi) * 0.5;
            let y = if p.y >= lo && p.y <= hi { mid + (p.y - mid) * sy } else if p.y < lo { mid + (lo - mid) * sy + (p.y - lo) } else { mid + (hi - mid) * sy + (p.y - hi) };
            let bulge = 1.0 + (1.0 / sy.max(0.05).sqrt() - 1.0) * b * 4.0 * t * (1.0 - t);
            DVec3::new(p.x * bulge, y, p.z * bulge)
        }
        3 => {
            // flare: X scale from a to b, Z scale from c to d... here a/b are X start/end, c the curve.
            let t = ((p.y - lo) / span).clamp(0.0, 1.0);
            let curve = c * (std::f64::consts::PI * t).sin();
            let sx = a + (b - a) * t + curve;
            let sz = d + (b - a) * t + curve;
            DVec3::new(p.x * sx, p.y, p.z * sz)
        }
        4 => {
            // sine: amplitude a, wavelength b, offset c along Y, displacing X.
            if p.y < lo || p.y > hi {
                return p;
            }
            let wl = b.max(1e-6);
            DVec3::new(p.x + a * (std::f64::consts::TAU * p.y / wl + c).sin(), p.y, p.z)
        }
        _ => {
            // wave: rings in XZ, amplitude a, wavelength b, offset c, out to the high bound.
            let r = (p.x * p.x + p.z * p.z).sqrt();
            if r > hi.abs().max(1e-6) {
                return p;
            }
            let wl = b.max(1e-6);
            DVec3::new(p.x, p.y + a * (std::f64::consts::TAU * r / wl + c).sin(), p.z)
        }
    }
}

impl NodeBehavior for NonLinear {
    fn compute(&mut self, _ctx: &EvalContext, d: &mut NodeData) {
        let Some(input) = d.mesh(IN).cloned() else {
            d.set_output(OUT, PlugValue::Mesh(None));
            return;
        };
        let data = json_of(d, DATA);
        let geom = mat_from(data.get("geom").unwrap_or(&Value::Null));
        let handle = d.matrix(HANDLE);
        let (a, b, c, dd, lo, hi) = (d.float(NL_A), d.float(NL_B), d.float(NL_C), d.float(NL_D), d.float(NL_LOW), d.float(NL_HIGH));
        let kind = self.kind;
        let mut m = (*input).clone();
        through_handle(&mut m, &geom, &handle, d.float(ENVELOPE), None, |p| nonlinear(kind, a, b, c, dd, lo, hi, p));
        d.set_output(OUT, PlugValue::Mesh(Some(Arc::new(m))));
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub struct Cluster;

impl NodeBehavior for Cluster {
    fn compute(&mut self, _ctx: &EvalContext, d: &mut NodeData) {
        let Some(input) = d.mesh(IN).cloned() else {
            d.set_output(OUT, PlugValue::Mesh(None));
            return;
        };
        let data = json_of(d, DATA);
        let geom = mat_from(data.get("geom").unwrap_or(&Value::Null));
        let bind = mat_from(data.get("bindPre").unwrap_or(&Value::Null));
        let weights: Vec<f64> = data.get("weights").and_then(Value::as_array).map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect()).unwrap_or_default();
        let h = d.matrix(HANDLE) * bind;
        let mut m = (*input).clone();
        let gi = geom.inverse();
        let env = d.float(ENVELOPE);
        for p in 0..m.point_capacity() {
            let w = weights.get(p as usize).copied().unwrap_or(0.0) * env;
            if w == 0.0 || !m.point_valid(p) {
                continue;
            }
            let rest = m.position(p);
            let moved = gi.transform_point3(h.transform_point3(geom.transform_point3(rest)));
            m.set_position(p, rest + (moved - rest) * w);
        }
        d.set_output(OUT, PlugValue::Mesh(Some(Arc::new(m))));
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub struct BlendShape;

impl NodeBehavior for BlendShape {
    fn compute(&mut self, _ctx: &EvalContext, d: &mut NodeData) {
        let Some(input) = d.mesh(IN).cloned() else {
            d.set_output(OUT, PlugValue::Mesh(None));
            return;
        };
        let data = json_of(d, DATA);
        let targets = data.get("targets").and_then(Value::as_array).cloned().unwrap_or_default();
        let env = d.float(ENVELOPE);
        let mut m = (*input).clone();
        for (i, t) in targets.iter().enumerate() {
            let attr = BASE_COUNT + i;
            if attr >= d.ty().attrs.len() {
                break;
            }
            let w = d.float(attr) * env;
            if w == 0.0 {
                continue;
            }
            let deltas = t.get("deltas").and_then(Value::as_array).cloned().unwrap_or_default();
            for e in deltas {
                let f = |k: usize| e.get(k).and_then(Value::as_f64).unwrap_or(0.0);
                let p = f(0) as u32;
                if p < m.point_capacity() && m.point_valid(p) {
                    let at = m.position(p);
                    m.set_position(p, at + DVec3::new(f(1), f(2), f(3)) * w);
                }
            }
        }
        d.set_output(OUT, PlugValue::Mesh(Some(Arc::new(m))));
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// A deformer handle: a transform drawn as the deformer's frame.
pub struct DeformHandle;
impl TransformBehavior for DeformHandle {}
impl NodeBehavior for DeformHandle {
    fn compute(&mut self, _ctx: &EvalContext, d: &mut NodeData) {
        let local = Transform::local_matrix_of(self, d);
        let parent = d.matrix(Transform::PARENT_MATRIX);
        d.set_output(Transform::MATRIX, local);
        d.set_output(Transform::WORLD_MATRIX, parent * local);
    }
    fn as_transform(&self) -> Option<&dyn TransformBehavior> {
        Some(self)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub fn register_types() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let xf = types::find("transform").expect("transform");
        types::register_type(NodeType {
            name: "deformHandle".into(),
            base: Some(xf),
            dag: true,
            icon: "axis-y".into(),
            doc: "A deformer's handle: move, turn and scale it to place the deformation.".into(),
            own_attrs: vec![AttrDef {
                name: "deformerType".into(),
                short_name: "dft".into(),
                ty: PlugType::String,
                default_value: PlugValue::String(String::new()),
                flags: flags::INPUT | flags::STORABLE | flags::HIDDEN,
                doc: "What it handles: bend, cluster...".into(),
                ..Default::default()
            }],
            create: Some(Arc::new(|| Box::new(DeformHandle))),
            ..Default::default()
        })
        .expect("deformHandle");
        let shared = |extra: Vec<AttrDef>| {
            let mut v = vec![
                mesh_attr("inputPolymesh", "ip", flags::INPUT | flags::HIDDEN, "The mesh before the deformer."),
                mesh_attr("output", "out", flags::OUTPUT | flags::HIDDEN, "The deformed mesh."),
                float_attr_short("envelope", "en", 1.0, "How much of the deformation applies.", Some(0.0), Some(1.0)),
                data_attr("deformerData", "dd", "Bind matrices and weights."),
                matrix_in("handleMatrix", "hm", "The handle's world matrix."),
            ];
            v.extend(extra);
            v
        };
        let nl = types::register_type(NodeType { name: "nonLinear".into(), icon: "wave-sine".into(), doc: "A nonlinear deformer.".into(), own_attrs: shared(vec![]), ..Default::default() }).expect("nonLinear");
        let specs: [(&str, [(&str, &str, f64); 6]); 6] = [
            ("bend", [("curvature", "cur", 0.0), ("unusedB", "ub", 0.0), ("unusedC", "uc", 0.0), ("lowBound", "lb", -1.0), ("highBound", "hb", 1.0), ("unusedD", "ud", 0.0)]),
            ("twist", [("startAngle", "sa", 0.0), ("endAngle", "ea", 0.0), ("unusedC", "uc", 0.0), ("lowBound", "lb", -1.0), ("highBound", "hb", 1.0), ("unusedD", "ud", 0.0)]),
            ("squash", [("factor", "f", 0.0), ("expand", "exp", 1.0), ("unusedC", "uc", 0.0), ("lowBound", "lb", -1.0), ("highBound", "hb", 1.0), ("unusedD", "ud", 0.0)]),
            ("flare", [("startFlareX", "sfx", 1.0), ("endFlareX", "efx", 1.0), ("curve", "c", 0.0), ("lowBound", "lb", -1.0), ("highBound", "hb", 1.0), ("startFlareZ", "sfz", 1.0)]),
            ("sine", [("amplitude", "amp", 0.0), ("wavelength", "wav", 2.0), ("offset", "off", 0.0), ("lowBound", "lb", -1.0), ("highBound", "hb", 1.0), ("unusedD", "ud", 0.0)]),
            ("wave", [("amplitude", "amp", 0.0), ("wavelength", "wav", 1.0), ("offset", "off", 0.0), ("minRadius", "mnr", 0.0), ("maxRadius", "mxr", 1.0), ("unusedD", "ud", 0.0)]),
        ];
        for (k, (name, attrs)) in specs.iter().enumerate() {
            let own: Vec<AttrDef> = attrs
                .iter()
                .map(|(n, s, def)| {
                    let mut a = float_attr_short(n, s, *def, "", None, None);
                    if n.starts_with("unused") {
                        a.flags |= flags::HIDDEN;
                        a.flags &= !flags::KEYABLE;
                    }
                    a
                })
                .collect();
            types::register_type(NodeType {
                name: name.to_string(),
                base: Some(nl.clone()),
                icon: "wave-sine".into(),
                doc: format!("The {name} nonlinear deformer: the mesh {name}s in its handle's frame."),
                own_attrs: own,
                create: Some(Arc::new(move || Box::new(NonLinear { kind: k }))),
                ..Default::default()
            })
            .expect(name);
        }
        types::register_type(NodeType {
            name: "cluster".into(),
            icon: "circle-letter-c".into(),
            doc: "Points that follow a handle, each by its weight.".into(),
            own_attrs: shared(vec![]),
            create: Some(Arc::new(|| Box::new(Cluster))),
            ..Default::default()
        })
        .expect("cluster");
        types::register_type(NodeType {
            name: "blendShape".into(),
            icon: "shape".into(),
            doc: "Blends the mesh toward target shapes, one weight per target.".into(),
            own_attrs: shared(vec![]),
            create: Some(Arc::new(|| Box::new(BlendShape))),
            ..Default::default()
        })
        .expect("blendShape");
    });
}

/// Puts a deformer node at the bottom of the mesh's chain.
fn insert(g: &mut Graph, mesh: NodeId, ty: &str, name: &str) -> CmdResult<NodeId> {
    let t = types::find(ty).unwrap();
    let (base, src) = crate::history::upstream(g, mesh)?;
    let n = g.create_node(&t, name, NO_NODE, true)?;
    g.disconnect(&base);
    g.connect(&src, &plug(n, IN), false)?;
    g.connect(&plug(n, OUT), &base, false)?;
    Ok(n)
}

fn set_data(g: &mut Graph, n: NodeId, v: Value) -> CmdResult<()> {
    g.set_value(plug(n, DATA), PlugValue::Data(Some(Arc::new(JsonData(v)))))?;
    Ok(())
}

fn make_handle(g: &mut Graph, name: &str, kind: &str, at: DVec3, scale: DVec3) -> CmdResult<NodeId> {
    let t = types::find("deformHandle").unwrap();
    let h = g.create_node(&t, name, NO_NODE, true)?;
    g.set_value(plug(h, Transform::TRANSLATE), PlugValue::Vec3(at))?;
    g.set_value(plug(h, Transform::SCALE), PlugValue::Vec3(scale))?;
    let attr = g.node(h).unwrap().ty().attr_index("deformerType").unwrap();
    g.set_value(plug(h, attr), PlugValue::String(kind.into()))?;
    Ok(h)
}

/// Meshes from the arguments or selection, with the points picked on each (None for all).
fn targets(s: &mut Session, a: &aztek_ops::Args) -> CmdResult<Vec<(NodeId, Option<Vec<u32>>)>> {
    let g = &mut s.graph;
    let mut out: Vec<(NodeId, Option<Vec<u32>>)> = Vec::new();
    let names = a.named();
    if names.is_empty() {
        let mode = components::mode(g);
        if mode != ComponentMode::Object {
            for (node, ids) in components::selected_all(g, mode).clone() {
                let Some(m) = components::as_mesh(g, node) else { continue };
                let Some(cage) = crate::modifiers::cage(g, m) else { continue };
                out.push((m, Some(components::convert(&cage, mode, &ids, ComponentMode::Vertex, false))));
            }
            if !out.is_empty() {
                return Ok(out);
            }
        }
        for id in g.selection().to_vec() {
            if let Some(m) = components::as_mesh(g, id) {
                out.push((m, None));
            }
        }
        return Ok(out);
    }
    for n in names {
        let obj = n.split('.').next().unwrap_or(&n);
        let Some(id) = g.find(obj)? else { fail!("no object named {obj}") };
        let Some(m) = components::as_mesh(g, id) else { fail!("{obj} is not a mesh") };
        out.push((m, None));
    }
    Ok(out)
}

fn world_bounds(g: &mut Graph, mesh: NodeId) -> (DVec3, DVec3) {
    let cage = crate::modifiers::cage(g, mesh);
    let w = world(g, crate::dag::transform_of(g, mesh));
    let mut lo = DVec3::splat(f64::MAX);
    let mut hi = DVec3::splat(f64::MIN);
    if let Some(m) = cage {
        for p in 0..m.point_capacity() {
            if m.point_valid(p) {
                let q = w.transform_point3(m.position(p));
                lo = lo.min(q);
                hi = hi.max(q);
            }
        }
    }
    if lo.x > hi.x {
        return (DVec3::ZERO, DVec3::ZERO);
    }
    (lo, hi)
}

type Cmd = CommandDef<Session>;

fn f(name: &str, short: &str, ty: ArgType, doc: &str) -> FlagDef {
    FlagDef::new(name, short, ty, Value::Null, doc)
}

pub fn register(c: &mut Commands<Session>) {
    register_types();
    c.register(
        Cmd::new(
            "nonLinear",
            "Add a nonlinear deformer (-type bend, twist, squash, flare, sine or wave) to the selected meshes, with a handle \
             fitted to them. Returns [deformer, handle].",
            |s, a| {
                let kind = a.string("type", "bend");
                if !NONLINEAR.contains(&kind.as_str()) {
                    fail!("nonLinear: type must be one of {}", NONLINEAR.join(", "));
                }
                let meshes = targets(s, a)?;
                if meshes.is_empty() {
                    fail!("nonLinear: select meshes");
                }
                let g = &mut s.graph;
                g.evaluate();
                let mut lo = DVec3::splat(f64::MAX);
                let mut hi = DVec3::splat(f64::MIN);
                for (m, _) in &meshes {
                    let (l, h) = world_bounds(g, *m);
                    lo = lo.min(l);
                    hi = hi.max(h);
                }
                let centre = (lo + hi) * 0.5;
                let half = ((hi - lo) * 0.5).max(DVec3::splat(1e-3));
                let sc = if kind == "wave" { DVec3::splat(half.x.max(half.z)) } else { DVec3::splat(half.y) };
                g.open_chunk("Nonlinear");
                let r = (|| -> CmdResult<(NodeId, NodeId)> {
                    let h = make_handle(g, &format!("{kind}1Handle"), &kind, centre, sc)?;
                    let mut first = NO_NODE;
                    for (m, _) in &meshes {
                        let geom = world(g, crate::dag::transform_of(g, *m));
                        let n = insert(g, *m, &kind, &format!("{kind}1"))?;
                        set_data(g, n, json!({"geom": mat_json(&geom)}))?;
                        g.connect(&plug(h, Transform::WORLD_MATRIX), &plug(n, HANDLE), false)?;
                        for (k, v) in [("curvature", "curvature"), ("startAngle", "startAngle"), ("endAngle", "endAngle"), ("factor", "factor"), ("amplitude", "amplitude"), ("wavelength", "wavelength")] {
                            if let Some(x) = a.get(k).and_then(Value::as_f64) {
                                if let Some(i) = g.node(n).unwrap().ty().attr_index(v) {
                                    g.set_value(plug(n, i), PlugValue::Float(x))?;
                                }
                            }
                        }
                        if first == NO_NODE {
                            first = n;
                        }
                    }
                    g.select(&[h], SelectMode::Replace);
                    Ok((first, h))
                })();
                g.close_chunk();
                let (n, h) = r?;
                Ok(json!([g.path(n), g.path(h)]))
            },
        )
        .positional("meshes (the selection when none)")
        .flag(f("type", "typ", ArgType::String, "bend, twist, squash, flare, sine or wave.").choices(NONLINEAR))
        .flag(f("curvature", "cur", ArgType::Float, "bend: how far it bends."))
        .flag(f("startAngle", "sa", ArgType::Float, "twist: degrees at the low bound."))
        .flag(f("endAngle", "ea", ArgType::Float, "twist: degrees at the high bound."))
        .flag(f("factor", "f", ArgType::Float, "squash: stretch (positive) or squash (negative)."))
        .flag(f("amplitude", "amp", ArgType::Float, "sine, wave: height of the waves."))
        .flag(f("wavelength", "wav", ArgType::Float, "sine, wave: length of the waves.")),
    );

    c.register(
        Cmd::new(
            "cluster",
            "Make a cluster of the selected vertices (or whole meshes): a handle they follow. Returns [cluster, handle].",
            |s, a| {
                let meshes = targets(s, a)?;
                if meshes.is_empty() {
                    fail!("cluster: select vertices or meshes");
                }
                let g = &mut s.graph;
                g.evaluate();
                // The handle sits at the middle of the picked points.
                let mut sum = DVec3::ZERO;
                let mut n_pts = 0.0;
                for (m, pts) in &meshes {
                    let cage = crate::modifiers::cage(g, *m).unwrap();
                    let w = world(g, crate::dag::transform_of(g, *m));
                    for p in 0..cage.point_capacity() {
                        if cage.point_valid(p) && pts.as_ref().is_none_or(|v| v.contains(&p)) {
                            sum += w.transform_point3(cage.position(p));
                            n_pts += 1.0;
                        }
                    }
                }
                let centre = if n_pts > 0.0 { sum / n_pts } else { DVec3::ZERO };
                g.open_chunk("Cluster");
                let r = (|| -> CmdResult<(NodeId, NodeId)> {
                    let h = make_handle(g, "cluster1Handle", "cluster", centre, DVec3::ONE)?;
                    g.evaluate();
                    let hw = world(g, h);
                    let mut first = NO_NODE;
                    for (m, pts) in &meshes {
                        let cage = crate::modifiers::cage(g, *m).unwrap();
                        let geom = world(g, crate::dag::transform_of(g, *m));
                        let weights: Vec<f64> = (0..cage.point_capacity()).map(|p| if pts.as_ref().is_none_or(|v| v.contains(&p)) { 1.0 } else { 0.0 }).collect();
                        let n = insert(g, *m, "cluster", "cluster1")?;
                        set_data(g, n, json!({"geom": mat_json(&geom), "bindPre": mat_json(&hw.inverse()), "weights": weights}))?;
                        g.connect(&plug(h, Transform::WORLD_MATRIX), &plug(n, HANDLE), false)?;
                        if first == NO_NODE {
                            first = n;
                        }
                    }
                    components::set_mode_undoable(g, ComponentMode::Object);
                    g.select(&[h], SelectMode::Replace);
                    Ok((first, h))
                })();
                g.close_chunk();
                let (n, h) = r?;
                Ok(json!([g.path(n), g.path(h)]))
            },
        )
        .positional("meshes or vertices (the selection when none)")
        .flag(f("name", "n", ArgType::String, "Accepted for Maya scripts.")),
    );

    c.register(
        Cmd::new(
            "blendShape",
            "Blend shapes: the targets named first, the base mesh last (or selected the same way). Each target gets a weight \
             on the blendShape node, named after it. Returns [blendShape].",
            |s, a| {
                let names = objects(s, a);
                let g = &mut s.graph;
                if names.len() < 2 {
                    fail!("blendShape: select the targets, then the base mesh");
                }
                let ids: Vec<NodeId> = names.iter().map(|n| g.find(n).ok().flatten().ok_or_else(|| aztek_ops::CommandError::new(format!("blendShape: no object named {n}")))).collect::<CmdResult<_>>()?;
                let Some(base) = components::as_mesh(g, *ids.last().unwrap()) else { fail!("blendShape: the base is not a mesh") };
                g.evaluate();
                let base_mesh = crate::modifiers::cage(g, base).unwrap();
                let mut targets = Vec::new();
                let mut target_names = Vec::new();
                for &t in &ids[..ids.len() - 1] {
                    let Some(tm) = components::as_mesh(g, t) else { fail!("blendShape: {} is not a mesh", g.path(t)) };
                    let tmesh = crate::modifiers::cage(g, tm).unwrap();
                    if tmesh.point_capacity() != base_mesh.point_capacity() {
                        fail!("blendShape: {} has a different number of points from the base", g.path(t));
                    }
                    let mut deltas = Vec::new();
                    for p in 0..base_mesh.point_capacity() {
                        if base_mesh.point_valid(p) && tmesh.point_valid(p) {
                            let d = tmesh.position(p) - base_mesh.position(p);
                            if d.length_squared() > 1e-18 {
                                deltas.push(json!([p, d.x, d.y, d.z]));
                            }
                        }
                    }
                    let tname = g.node(crate::dag::transform_of(g, t)).map(|n| n.name.clone()).unwrap_or_default();
                    targets.push(json!({"name": tname, "deltas": deltas}));
                    target_names.push(tname);
                }
                g.open_chunk("Blend Shape");
                let r = (|| -> CmdResult<NodeId> {
                    let n = insert(g, base, "blendShape", &a.string("name", "blendShape1"))?;
                    set_data(g, n, json!({"targets": targets}))?;
                    for t in &target_names {
                        let mut def = float_attr_short(t, "", 0.0, "Weight of this target.", Some(0.0), Some(1.0));
                        def.flags |= flags::USER;
                        g.add_attr(n, def)?;
                    }
                    Ok(n)
                })();
                g.close_chunk();
                let n = r?;
                Ok(json!([g.path(n)]))
            },
        )
        .positional("targets, then the base")
        .flag(f("name", "n", ArgType::String, "Name of the blendShape node.")),
    );
}

/// Lines drawing a deformer handle, in world space.
pub fn handle_lines(kind: &str, w: &DMat4) -> Vec<(DVec3, DVec3)> {
    let p = |x: f64, y: f64, z: f64| w.transform_point3(DVec3::new(x, y, z));
    let mut out = Vec::new();
    match kind {
        "cluster" => {
            // A "C" in the view's plane is Maya's; a small diamond reads the same here.
            let k = 0.25;
            let pts = [p(k, 0.0, 0.0), p(0.0, k, 0.0), p(-k, 0.0, 0.0), p(0.0, -k, 0.0)];
            for i in 0..4 {
                out.push((pts[i], pts[(i + 1) % 4]));
            }
            out.push((p(0.0, 0.0, -k), p(0.0, 0.0, k)));
        }
        "wave" => {
            let n = 24;
            for i in 0..n {
                let (a, b) = (i as f64 / n as f64 * std::f64::consts::TAU, (i + 1) as f64 / n as f64 * std::f64::consts::TAU);
                out.push((p(a.cos(), 0.0, a.sin()), p(b.cos(), 0.0, b.sin())));
            }
            out.push((p(-1.0, 0.0, 0.0), p(1.0, 0.0, 0.0)));
        }
        _ => {
            out.push((p(0.0, -1.0, 0.0), p(0.0, 1.0, 0.0)));
            for y in [-1.0, 1.0] {
                out.push((p(-0.15, y, 0.0), p(0.15, y, 0.0)));
                out.push((p(0.0, y, -0.15), p(0.0, y, 0.15)));
            }
        }
    }
    out
}
