//! Rigging as Maya does it: joints (joint, orientJoint), smooth skinning (skinCluster, skinPercent), constraints
//! (parent, point, orient, scale, aim) and IK handles (ikHandle with the RP and SC solvers).

use std::any::Any;
use std::sync::Arc;

use aztek_ops::{fail, objects, ArgType, Args, CmdResult, CommandDef, Commands, FlagDef};
use glam::{DMat3, DMat4, DQuat, DVec3};
use serde_json::{json, Value};

use crate::attr_defs::float_attr_short;
use crate::graph::{Graph, SelectMode};
use crate::node::{types, vec3_attr, EvalContext, NodeBehavior, NodeData, NodeType};
use crate::plug::{flags, AttrDef, NodeId, PlugRef, PlugType, PlugValue, NO_NODE};
use crate::session::Session;
use crate::transform::{euler_to_matrix, matrix_to_euler, RotateOrder, Transform, TransformBehavior};

pub(crate) fn plug(n: NodeId, a: usize) -> PlugRef {
    PlugRef::new(n, a as i32)
}

pub(crate) fn data_attr(name: &str, short: &str, doc: &str) -> AttrDef {
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

pub(crate) fn matrix_in(name: &str, short: &str, array: bool, doc: &str) -> AttrDef {
    AttrDef {
        name: name.into(),
        short_name: short.into(),
        ty: PlugType::Matrix,
        default_value: PlugValue::Matrix(DMat4::IDENTITY),
        flags: flags::INPUT | flags::HIDDEN | if array { flags::ARRAY } else { 0 },
        doc: doc.into(),
        ..Default::default()
    }
}

fn vec3_out(name: &str, short: &str, doc: &str) -> AttrDef {
    vec3_attr(name, short, DVec3::ZERO, flags::OUTPUT | flags::HIDDEN, doc)
}

/// Data payloads: plain JSON kept on a node (bind matrices, weights, rest poses).
#[derive(Debug, Clone, Default)]
pub struct JsonData(pub Value);

impl crate::plug::Data for JsonData {
    fn kind(&self) -> &str {
        "rigData"
    }
    fn to_json(&self) -> Value {
        json!({"kind": "rigData", "value": self.0})
    }
    fn bytes(&self) -> usize {
        self.0.to_string().len()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub(crate) fn json_of(d: &NodeData, attr: usize) -> Value {
    match d.value(attr) {
        PlugValue::Data(Some(x)) => x.as_any().downcast_ref::<JsonData>().map(|j| j.0.clone()).unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

pub(crate) fn set_json(g: &mut Graph, n: NodeId, attr: usize, v: Value) -> CmdResult<()> {
    g.set_value(plug(n, attr), PlugValue::Data(Some(Arc::new(JsonData(v)))))?;
    Ok(())
}

pub(crate) fn mat_json(m: &DMat4) -> Value {
    json!(m.to_cols_array().to_vec())
}

pub(crate) fn mat_from(v: &Value) -> DMat4 {
    let a: Vec<f64> = v.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
    if a.len() == 16 {
        DMat4::from_cols_array(&a.try_into().unwrap())
    } else {
        DMat4::IDENTITY
    }
}

fn rot3(m: &DMat4) -> DMat3 {
    let m3 = DMat3::from_mat4(*m);
    DMat3::from_cols(m3.x_axis.normalize_or_zero(), m3.y_axis.normalize_or_zero(), m3.z_axis.normalize_or_zero())
}

// ---------------------------------------------------------------------------------------------------------------------
// Joints

pub struct Joint;

impl Joint {
    pub const JOINT_ORIENT: usize = Transform::COUNT;
    pub const RADIUS: usize = Transform::COUNT + 1;
    pub const SEGMENT_SCALE_COMPENSATE: usize = Transform::COUNT + 2;
}

impl TransformBehavior for Joint {
    fn pre_rotation(&self, d: &NodeData) -> DMat4 {
        euler_to_matrix(d.vec3(Joint::JOINT_ORIENT), RotateOrder::Xyz)
    }
}

impl NodeBehavior for Joint {
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

pub fn is_joint(g: &Graph, id: NodeId) -> bool {
    g.node(id).is_some_and(|n| n.ty().is_a("joint"))
}

pub fn world(g: &mut Graph, id: NodeId) -> DMat4 {
    match g.value(&plug(id, Transform::WORLD_MATRIX)) {
        Ok(PlugValue::Matrix(m)) => m,
        _ => DMat4::IDENTITY,
    }
}

fn parent_world(g: &mut Graph, id: NodeId) -> DMat4 {
    let p = g.parent(id);
    if p == NO_NODE {
        DMat4::IDENTITY
    } else {
        world(g, p)
    }
}

fn vec3_of(g: &Graph, id: NodeId, attr: usize) -> DVec3 {
    g.node(id).map(|n| n.data().vec3(attr)).unwrap_or(DVec3::ZERO)
}

fn set_vec3(g: &mut Graph, id: NodeId, attr: usize, v: DVec3) -> CmdResult<()> {
    g.set_value(plug(id, attr), PlugValue::Vec3(v))?;
    Ok(())
}

/// A joint at a world position under parent (none for a root), oriented like its parent.
pub fn create_joint(g: &mut Graph, name: &str, parent: NodeId, at: DVec3, radius: f64) -> CmdResult<NodeId> {
    let t = types::find("joint").unwrap();
    let j = g.create_node(&t, if name.is_empty() { "joint1" } else { name }, parent, true)?;
    let pw = if parent == NO_NODE { DMat4::IDENTITY } else { world(g, parent) };
    set_vec3(g, j, Transform::TRANSLATE, pw.inverse().transform_point3(at))?;
    g.set_value(plug(j, Joint::RADIUS), PlugValue::Float(radius))?;
    Ok(j)
}

/// The joint's world rotation set to frame, keeping rotate at zero: jointOrient takes it. Children keep their world
/// placement.
fn set_world_orientation(g: &mut Graph, j: NodeId, frame: DMat3) -> CmdResult<()> {
    let kids: Vec<NodeId> = g.children(j).iter().copied().filter(|&c| g.node(c).is_some_and(|n| n.ty().is_a("transform"))).collect();
    let kid_worlds: Vec<DMat4> = kids.iter().map(|&k| world(g, k)).collect();
    let pw = rot3(&parent_world(g, j));
    let local = pw.transpose() * frame;
    set_vec3(g, j, Transform::ROTATE, DVec3::ZERO)?;
    set_vec3(g, j, Joint::JOINT_ORIENT, matrix_to_euler(&local, RotateOrder::Xyz))?;
    g.evaluate();
    let jw = world(g, j);
    for (k, w) in kids.into_iter().zip(kid_worlds) {
        set_vec3(g, k, Transform::TRANSLATE, jw.inverse().transform_point3(w.w_axis.truncate()))?;
        if is_joint(g, k) {
            let kr = rot3(&w);
            let local = rot3(&jw).transpose() * kr;
            let r = euler_to_matrix(vec3_of(g, k, Transform::ROTATE), RotateOrder::Xyz);
            let jo = local * DMat3::from_mat4(r).transpose();
            set_vec3(g, k, Joint::JOINT_ORIENT, matrix_to_euler(&jo, RotateOrder::Xyz))?;
        }
    }
    Ok(())
}

/// Maya's orientJoint: the first axis of `order` aims at the first child joint, the second toward `up` (a world axis
/// like "yup"). Joints with no child line up with their parent ("none" zeroes jointOrient).
pub fn orient_joint(g: &mut Graph, j: NodeId, order: &str, up: &str) -> CmdResult<()> {
    g.evaluate();
    if order == "none" {
        let kids: Vec<NodeId> = g.children(j).to_vec();
        let worlds: Vec<DMat4> = kids.iter().map(|&k| world(g, k)).collect();
        set_vec3(g, j, Joint::JOINT_ORIENT, DVec3::ZERO)?;
        g.evaluate();
        let jw = world(g, j);
        for (k, w) in kids.into_iter().zip(worlds) {
            set_vec3(g, k, Transform::TRANSLATE, jw.inverse().transform_point3(w.w_axis.truncate()))?;
        }
        return Ok(());
    }
    let o: Vec<char> = order.chars().collect();
    if o.len() != 3 {
        fail!("orientJoint: {order} is not an axis order like xyz");
    }
    let child = g.children(j).iter().copied().find(|&c| is_joint(g, c));
    let here = world(g, j).w_axis.truncate();
    let Some(child) = child else {
        // End joints take their parent's orientation.
        let p = g.parent(j);
        let frame = if p != NO_NODE && is_joint(g, p) { rot3(&world(g, p)) } else { DMat3::IDENTITY };
        return set_world_orientation(g, j, frame);
    };
    let aim = (world(g, child).w_axis.truncate() - here).normalize_or_zero();
    if aim == DVec3::ZERO {
        return Ok(());
    }
    let mut upv = match up {
        "xup" => DVec3::X,
        "xdown" => -DVec3::X,
        "ydown" => -DVec3::Y,
        "zup" => DVec3::Z,
        "zdown" => -DVec3::Z,
        "none" => DVec3::Y,
        _ => DVec3::Y,
    };
    if upv.cross(aim).length() < 1e-6 {
        upv = if aim.cross(DVec3::Z).length() > 1e-6 { DVec3::Z } else { DVec3::X };
    }
    let second = (upv - aim * upv.dot(aim)).normalize();
    let third = aim.cross(second);
    // Columns: the world direction of local X, Y, Z.
    let mut cols = [DVec3::ZERO; 3];
    let idx = |c: char| match c {
        'x' => 0,
        'y' => 1,
        _ => 2,
    };
    cols[idx(o[0])] = aim;
    cols[idx(o[1])] = second;
    // Right handed: the third axis completes the frame.
    let third_axis = idx(o[2]);
    let (a, b) = (idx(o[0]), idx(o[1]));
    let parity = (a, b);
    let sign = if matches!(parity, (0, 1) | (1, 2) | (2, 0)) { 1.0 } else { -1.0 };
    cols[third_axis] = third * sign;
    set_world_orientation(g, j, DMat3::from_cols(cols[0], cols[1], cols[2]))
}

// ---------------------------------------------------------------------------------------------------------------------
// Skin


// ---------------------------------------------------------------------------------------------------------------------
// Constraints

pub const CONSTRAINT_TYPES: &[&str] = &["parentConstraint", "pointConstraint", "orientConstraint", "scaleConstraint", "aimConstraint"];
const CN_TARGET: usize = 0;
const CN_PARENT: usize = 1;
const CN_DATA: usize = 2;
const CN_T: usize = 3;
const CN_R: usize = 4;
const CN_S: usize = 5;

pub struct ConstraintNode {
    kind: usize,
}

fn decompose(m: &DMat4) -> (DVec3, DMat3, DVec3) {
    let m3 = DMat3::from_mat4(*m);
    let s = DVec3::new(m3.x_axis.length(), m3.y_axis.length(), m3.z_axis.length());
    (m.w_axis.truncate(), rot3(m), s)
}

fn quat_avg(qs: &[(DQuat, f64)]) -> DQuat {
    let mut acc = glam::DVec4::ZERO;
    let first = qs.first().map(|q| q.0).unwrap_or(DQuat::IDENTITY);
    for &(q, w) in qs {
        let q = if q.dot(first) < 0.0 { -q } else { q };
        acc += glam::DVec4::new(q.x, q.y, q.z, q.w) * w;
    }
    DQuat::from_xyzw(acc.x, acc.y, acc.z, acc.w).normalize()
}

impl NodeBehavior for ConstraintNode {
    fn compute(&mut self, _ctx: &EvalContext, d: &mut NodeData) {
        let data = json_of(d, CN_DATA);
        let offsets: Vec<DMat4> = data.get("offsets").and_then(Value::as_array).map(|a| a.iter().map(mat_from).collect()).unwrap_or_default();
        let weights: Vec<f64> = data.get("weights").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
        let order = RotateOrder::from_index(data.get("rotateOrder").and_then(Value::as_i64).unwrap_or(0));
        let rest = mat_from(data.get("rest").unwrap_or(&Value::Null));
        let pinv = d.matrix(CN_PARENT).inverse();
        let mut worlds = Vec::new();
        for (i, &w) in weights.iter().enumerate() {
            let tm = match d.element(CN_TARGET, i as i32) {
                Some(PlugValue::Matrix(m)) => *m,
                _ => continue,
            };
            worlds.push((tm, offsets.get(i).copied().unwrap_or(DMat4::IDENTITY), w));
        }
        let total: f64 = worlds.iter().map(|x| x.2).sum::<f64>().max(1e-12);
        let (rt, rr, rs) = decompose(&(pinv * rest));
        let mut t = rt;
        let mut r = rr;
        let mut s = rs;
        match self.kind {
            0 => {
                // Parent: the whole target frame, times the offset taken at creation.
                let mut tp = DVec3::ZERO;
                let mut qs = Vec::new();
                for (tm, off, w) in &worlds {
                    let wm = pinv * *tm * *off;
                    let (tt, rot, _) = decompose(&wm);
                    tp += tt * *w;
                    qs.push((DQuat::from_mat3(&rot), *w));
                }
                if !worlds.is_empty() {
                    t = tp / total;
                    r = DMat3::from_quat(quat_avg(&qs));
                }
            }
            1 => {
                let mut tp = DVec3::ZERO;
                for (tm, off, w) in &worlds {
                    tp += (tm.w_axis.truncate() + off.w_axis.truncate()) * *w;
                }
                if !worlds.is_empty() {
                    t = pinv.transform_point3(tp / total);
                }
            }
            2 => {
                let mut qs = Vec::new();
                for (tm, off, w) in &worlds {
                    qs.push((DQuat::from_mat3(&(rot3(&pinv) * rot3(tm) * rot3(off))), *w));
                }
                if !worlds.is_empty() {
                    r = DMat3::from_quat(quat_avg(&qs));
                }
            }
            3 => {
                let mut sp = DVec3::ZERO;
                for (tm, off, w) in &worlds {
                    let (_, _, ts) = decompose(tm);
                    let (_, _, os) = decompose(off);
                    sp += ts * os * *w;
                }
                if !worlds.is_empty() {
                    s = sp / total;
                }
            }
            _ => {
                // Aim: local X at the targets, Y toward world up.
                let here = rest.w_axis.truncate();
                let mut tp = DVec3::ZERO;
                for (tm, _, w) in &worlds {
                    tp += tm.w_axis.truncate() * *w;
                }
                let aim = (tp / total - (d.matrix(CN_PARENT) * pinv * rest).w_axis.truncate()).normalize_or_zero();
                let _ = here;
                if aim != DVec3::ZERO {
                    let up = if aim.cross(DVec3::Y).length() > 1e-6 { DVec3::Y } else { DVec3::Z };
                    let y = (up - aim * up.dot(aim)).normalize();
                    let z = aim.cross(y);
                    let off = offsets.first().copied().unwrap_or(DMat4::IDENTITY);
                    r = rot3(&pinv) * DMat3::from_cols(aim, y, z) * rot3(&off);
                }
            }
        }
        d.set_output(CN_T, PlugValue::Vec3(t));
        d.set_output(CN_R, PlugValue::Vec3(matrix_to_euler(&r, order)));
        d.set_output(CN_S, PlugValue::Vec3(s));
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// IK

pub const MAX_CHAIN: usize = 8;
const IK_HANDLE: usize = 0;
const IK_PARENT: usize = 1;
const IK_DATA: usize = 2;
const IK_POLE: usize = 3;
const IK_TWIST: usize = 4;
const IK_OUT: usize = 5;

pub struct IkHandle;

impl IkHandle {
    pub const POLE_VECTOR: usize = Transform::COUNT;
    pub const TWIST: usize = Transform::COUNT + 1;
}

impl TransformBehavior for IkHandle {}

impl NodeBehavior for IkHandle {
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

pub struct IkSolve;

struct RestJoint {
    t: DVec3,
    jo: DMat3,
    r: DMat3,
}

fn rotation_between(a: DVec3, b: DVec3) -> DQuat {
    let (a, b) = (a.normalize_or_zero(), b.normalize_or_zero());
    if a == DVec3::ZERO || b == DVec3::ZERO {
        return DQuat::IDENTITY;
    }
    DQuat::from_rotation_arc(a, b)
}

/// World rotations for the chain so its end reaches target: analytic for two bones (in the pole vector's plane),
/// cyclic coordinate descent for longer chains.
fn solve_chain(parent: &DMat4, rest: &[RestJoint], target: DVec3, pole: DVec3, twist: f64, rp: bool) -> Vec<DMat3> {
    let pr = rot3(parent);
    let mut rots = Vec::new();
    let mut pos = Vec::new();
    let mut w = pr;
    let mut at = parent.w_axis.truncate();
    for (i, j) in rest.iter().enumerate() {
        at = if i == 0 { parent.transform_point3(j.t) } else { at + w * j.t };
        w = w * j.jo * j.r;
        rots.push(w);
        pos.push(at);
    }
    let n = pos.len();
    if n < 2 {
        return rots;
    }
    if n == 3 && rp {
        let (a, b, c) = (pos[0], pos[1], pos[2]);
        let (l1, l2) = ((b - a).length(), (c - b).length());
        let to = target - a;
        let d = to.length().clamp(1e-6, (l1 + l2) * 0.99999);
        let x = to.normalize_or_zero();
        if x == DVec3::ZERO {
            return rots;
        }
        let mut y = pole - x * pole.dot(x);
        if y.length() < 1e-9 {
            y = (b - a) - x * (b - a).dot(x);
        }
        let y = DQuat::from_axis_angle(x, twist.to_radians()) * y.normalize_or_zero();
        let cos_a = ((l1 * l1 + d * d - l2 * l2) / (2.0 * l1 * d)).clamp(-1.0, 1.0);
        let b2 = a + (x * cos_a + y * (1.0 - cos_a * cos_a).sqrt()) * l1;
        let c2 = a + x * d;
        // Rest plane and new plane.
        let mut n0 = (b - a).cross(c - b);
        if n0.length() < 1e-9 {
            n0 = (b - a).cross(pole);
        }
        let n1 = (b2 - a).cross(c2 - b2);
        let frame = |dir: DVec3, nrm: DVec3| {
            let dir = dir.normalize_or_zero();
            let nrm = (nrm - dir * nrm.dot(dir)).normalize_or_zero();
            DMat3::from_cols(dir, nrm.cross(dir), nrm)
        };
        let qa = frame(b2 - a, n1) * frame(b - a, n0).transpose();
        let b_dir_after = qa * (c - b);
        let qb = DMat3::from_quat(rotation_between(b_dir_after, c2 - b2));
        rots[0] = qa * rots[0];
        rots[1] = qb * qa * rots[1];
        rots[2] = qb * qa * rots[2];
        return rots;
    }
    // CCD.
    let mut pos = pos;
    for _ in 0..24 {
        for i in (0..n - 1).rev() {
            let to_end = pos[n - 1] - pos[i];
            let to_target = target - pos[i];
            let q = DMat3::from_quat(rotation_between(to_end, to_target));
            for k in i..n {
                rots[k] = q * rots[k];
                pos[k] = pos[i] + q * (pos[k] - pos[i]);
            }
        }
        if (pos[n - 1] - target).length() < 1e-5 {
            break;
        }
    }
    rots
}

impl NodeBehavior for IkSolve {
    fn compute(&mut self, _ctx: &EvalContext, d: &mut NodeData) {
        let data = json_of(d, IK_DATA);
        let rest: Vec<RestJoint> = data
            .get("joints")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|j| {
                        let v = |k: &str| {
                            let x = j.get(k).and_then(Value::as_array);
                            let f = |i: usize| x.and_then(|x| x.get(i)).and_then(Value::as_f64).unwrap_or(0.0);
                            DVec3::new(f(0), f(1), f(2))
                        };
                        RestJoint {
                            t: v("t"),
                            jo: DMat3::from_mat4(euler_to_matrix(v("jo"), RotateOrder::Xyz)),
                            r: DMat3::from_mat4(euler_to_matrix(v("r"), RotateOrder::Xyz)),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let rp = data.get("solver").and_then(Value::as_str) != Some("ikSCsolver");
        let parent = d.matrix(IK_PARENT);
        let target = d.matrix(IK_HANDLE).w_axis.truncate();
        let pole = d.vec3(IK_POLE);
        let twist = d.float(IK_TWIST);
        let rots = solve_chain(&parent, &rest, target, pole, twist, rp);
        let mut pw = rot3(&parent);
        for i in 0..MAX_CHAIN {
            let out = match (rots.get(i), rest.get(i)) {
                (Some(w), Some(j)) => {
                    // world = parentWorld * jo * R
                    let r = j.jo.transpose() * pw.transpose() * *w;
                    pw = *w;
                    matrix_to_euler(&r, RotateOrder::Xyz)
                }
                _ => DVec3::ZERO,
            };
            d.set_output(IK_OUT + i, PlugValue::Vec3(out));
        }
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Types

pub fn register_types() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(register_types_now);
}

fn register_types_now() {
    crate::plug::data::register_kind("rigData", Arc::new(|j| Ok(Arc::new(JsonData(j.get("value").cloned().unwrap_or(Value::Null))) as Arc<dyn crate::plug::Data>)));
    let xf = types::find("transform").expect("transform");
    let joint = NodeType {
        name: "joint".into(),
        base: Some(xf.clone()),
        dag: true,
        icon: "bone".into(),
        doc: "A skeleton joint: a transform with a jointOrient, drawn as a ball with bones to its child joints.".into(),
        own_attrs: vec![
            vec3_attr("jointOrient", "jo", DVec3::ZERO, flags::INPUT | flags::STORABLE, "Orientation between the parent and rotate, Euler XYZ degrees."),
            float_attr_short("radius", "radi", 1.0, "Display size of the joint.", Some(0.0), None),
            AttrDef { flags: flags::INPUT | flags::STORABLE, ..crate::attr_defs::bool_attr("segmentScaleCompensate", true, "Children don't inherit this joint's scale.") },
        ],
        create: Some(Arc::new(|| Box::new(Joint))),
        ..Default::default()
    };
    types::register_type(joint).expect("joint");
    let handle = NodeType {
        name: "ikHandle".into(),
        base: Some(xf),
        dag: true,
        icon: "target".into(),
        doc: "Where an IK chain's end reaches for. Move it; the joints follow.".into(),
        own_attrs: vec![
            vec3_attr("poleVector", "pv", DVec3::new(0.0, 0.0, -1.0), flags::INPUT | flags::STORABLE | flags::KEYABLE, "Which way the chain's middle bends (RP solver)."),
            float_attr_short("twist", "twi", 0.0, "Turn of the chain about the handle line, degrees.", None, None),
        ],
        create: Some(Arc::new(|| Box::new(IkHandle))),
        ..Default::default()
    };
    types::register_type(handle).expect("ikHandle");
    let mut ik_attrs = vec![
        matrix_in("handleMatrix", "hm", false, "The handle's world matrix."),
        matrix_in("startParentMatrix", "spm", false, "World matrix of the start joint's parent."),
        data_attr("chain", "ch", "The chain's rest pose."),
        vec3_attr("poleVector", "pv", DVec3::ZERO, flags::INPUT | flags::HIDDEN, "From the handle."),
        AttrDef { flags: flags::INPUT | flags::HIDDEN, ..float_attr_short("twist", "twi", 0.0, "From the handle.", None, None) },
    ];
    for i in 0..MAX_CHAIN {
        ik_attrs.push(vec3_out(&format!("outRotate{i}"), &format!("or{i}"), "A chain joint's rotate."));
    }
    types::register_type(NodeType {
        name: "ikSolve".into(),
        icon: "target".into(),
        doc: "Solves an IK handle's chain.".into(),
        own_attrs: ik_attrs,
        create: Some(Arc::new(|| Box::new(IkSolve))),
        ..Default::default()
    })
    .expect("ikSolve");
    for (k, name) in CONSTRAINT_TYPES.iter().enumerate() {
        types::register_type(NodeType {
            name: name.to_string(),
            icon: "link".into(),
            doc: "Drives an object from its targets.".into(),
            own_attrs: vec![
                matrix_in("target", "tg", true, "Each target's world matrix."),
                matrix_in("constraintParentInverseMatrix", "cpim", false, "World matrix of the driven object's parent."),
                data_attr("constraintData", "cd", "Weights, offsets and the driven object's rest."),
                vec3_out("constraintTranslate", "ct", "Translate for the driven object."),
                vec3_out("constraintRotate", "cr", "Rotate for the driven object."),
                vec3_out("constraintScale", "cs", "Scale for the driven object."),
            ],
            create: Some(Arc::new(move || Box::new(ConstraintNode { kind: k }))),
            ..Default::default()
        })
        .expect(name);
    }
}

pub fn is_rig_chain_node(type_name: &str) -> bool {
    type_name == crate::skin::SKIN_TYPE || crate::deform::is_deformer(type_name)
}

// ---------------------------------------------------------------------------------------------------------------------
// Commands

type Cmd = CommandDef<Session>;

fn f(name: &str, short: &str, ty: ArgType, doc: &str) -> FlagDef {
    FlagDef::new(name, short, ty, Value::Null, doc)
}

pub(crate) fn find(g: &Graph, name: &str, cmd: &str) -> CmdResult<NodeId> {
    match g.find(name)? {
        Some(id) => Ok(id),
        None => fail!("{cmd}: no object named {name}"),
    }
}

fn named_or_selected(s: &Session, a: &Args) -> Vec<String> {
    objects(s, a)
}

pub fn register(c: &mut Commands<Session>) {
    register_types();

    c.register(
        Cmd::new(
            "joint",
            "Create a joint at -p (world), under the selected joint, and select it. With -e: -oj orients joints (xyz...), \
             -sao the secondary axis (yup...), -ch their children too, -zso zeroes scale orient. -q -p returns the position.",
            |s, a| {
                let names = a.named();
                let g = &mut s.graph;
                if a.bool("query") {
                    let names = if names.is_empty() { g.selection().iter().map(|&i| g.path(i)).collect() } else { names };
                    let Some(n) = names.first() else { fail!("joint: name a joint") };
                    let id = find(g, n, "joint")?;
                    g.evaluate();
                    let p = world(g, id).w_axis.truncate();
                    return Ok(json!([p.x, p.y, p.z]));
                }
                if a.bool("edit") {
                    let names = if names.is_empty() { g.selection().iter().map(|&i| g.path(i)).collect() } else { names };
                    let order = a.string("orientJoint", "");
                    let up = a.string("secondaryAxisOrient", "yup");
                    let mut todo = Vec::new();
                    for n in names {
                        let id = find(g, &n, "joint")?;
                        todo.push(id);
                        if a.bool("children") {
                            for d in g.descendants(id) {
                                if is_joint(g, d) {
                                    todo.push(d);
                                }
                            }
                        }
                    }
                    for id in todo {
                        if !order.is_empty() {
                            orient_joint(g, id, &order, &up)?;
                        }
                        if let Some(p) = a.vec3("position") {
                            g.evaluate();
                            let pw = parent_world(g, id);
                            let p = DVec3::from_array(p);
                            let local = if a.bool("relative") { vec3_of(g, id, Transform::TRANSLATE) + p } else { pw.inverse().transform_point3(p) };
                            set_vec3(g, id, Transform::TRANSLATE, local)?;
                        }
                        if let Some(r) = a.get("radius").and_then(Value::as_f64) {
                            g.set_value(plug(id, Joint::RADIUS), PlugValue::Float(r))?;
                        }
                    }
                    return Ok(Value::Null);
                }
                let parent = g.selection().last().copied().filter(|&id| is_joint(g, id)).unwrap_or(NO_NODE);
                g.evaluate();
                let p = DVec3::from_array(a.vec3("position").unwrap_or([0.0; 3]));
                let at = if a.bool("relative") && parent != NO_NODE { world(g, parent).transform_point3(p) } else { p };
                let j = create_joint(g, &a.string("name", ""), parent, at, a.float("radius", 1.0))?;
                if let Some(o) = a.vec3("orientation") {
                    set_vec3(g, j, Joint::JOINT_ORIENT, DVec3::from_array(o))?;
                }
                g.select(&[j], SelectMode::Replace);
                Ok(json!(g.path(j)))
            },
        )
        .positional("joints (edit and query)")
        .flag(f("position", "p", ArgType::Vec3, "World position (or offset from the parent joint with -r)."))
        .flag(f("relative", "r", ArgType::Bool, "-p is relative to the parent joint."))
        .flag(f("absolute", "a", ArgType::Bool, "-p is a world position (the default)."))
        .flag(f("name", "n", ArgType::String, "Name of the new joint."))
        .flag(f("radius", "rad", ArgType::Float, "Display size."))
        .flag(f("orientation", "o", ArgType::Vec3, "jointOrient of the new joint, degrees."))
        .flag(f("orientJoint", "oj", ArgType::String, "Edit: aim axis order, xyz, yzx... or none."))
        .flag(f("secondaryAxisOrient", "sao", ArgType::String, "Edit: where the second axis points, yup, zup, xdown..."))
        .flag(f("children", "ch", ArgType::Bool, "Edit: the joints below too."))
        .flag(f("zeroScaleOrient", "zso", ArgType::Bool, "Accepted for Maya scripts."))
        .flag(f("edit", "e", ArgType::Bool, "Change existing joints."))
        .flag(f("query", "q", ArgType::Bool, "Query a joint (its world position)."))
        .flag(f("queried", "", ArgType::List, "Flags asked about in query mode.")),
    );

    c.register(
        Cmd::new(
            "orientJoint",
            "Orient joints: the first axis of the order aims at the child joint, the second toward -sao. Joints below follow \
             unless -ch 0.",
            |s, a| {
                let names = named_or_selected(s, a);
                let g = &mut s.graph;
                let order = a.positional.iter().find_map(|v| v.as_str().filter(|x| x.len() == 3 && x.chars().all(|c| "xyz".contains(c)) || *x == "none")).unwrap_or("xyz").to_string();
                let up = a.string("secondaryAxisOrient", "yup");
                for n in names.iter().filter(|n| **n != order) {
                    let id = find(g, n, "orientJoint")?;
                    let mut todo = vec![id];
                    if a.get("children").and_then(Value::as_bool).unwrap_or(true) {
                        todo.extend(g.descendants(id).into_iter().filter(|&d| is_joint(g, d)));
                    }
                    for j in todo {
                        orient_joint(g, j, &order, &up)?;
                    }
                }
                Ok(Value::Null)
            },
        )
        .positional("order (xyz), then joints")
        .flag(f("secondaryAxisOrient", "sao", ArgType::String, "yup, zup, xdown..."))
        .flag(f("children", "ch", ArgType::Bool, "The joints below too (default on).")),
    );


    for (k, name) in CONSTRAINT_TYPES.iter().enumerate() {
        let name = name.to_string();
        let n2 = name.clone();
        c.register(
            Cmd::new(
                &name,
                "Constrain the object named (or selected) last to the targets before it. -mo keeps the current offset; -w sets \
                 the targets' weight. Returns [constraint].",
                move |s, a| constrain(s, a, &n2, k),
            )
            .positional("targets, then the object to drive")
            .flag(f("maintainOffset", "mo", ArgType::Bool, "Keep the object where it is now relative to the targets."))
            .flag(f("weight", "w", ArgType::Float, "Weight of each target (1)."))
            .flag(f("name", "n", ArgType::String, "Name of the constraint."))
            .flag(f("aimVector", "aim", ArgType::Vec3, "Accepted for Maya scripts (local X aims)."))
            .flag(f("upVector", "u", ArgType::Vec3, "Accepted for Maya scripts (local Y up)."))
            .flag(f("worldUpType", "wut", ArgType::String, "Accepted for Maya scripts."))
            .flag(f("skipTranslate", "st", ArgType::Any, "Accepted for Maya scripts."))
            .flag(f("skipRotate", "sr", ArgType::Any, "Accepted for Maya scripts.")),
        );
    }

    c.register(
        Cmd::new(
            "ikHandle",
            "Create an IK handle from -sj to -ee (or two selected joints). The RP solver bends the middle toward the pole \
             vector; SC aims the chain. Returns [handle, effector].",
            |s, a| {
                let names = named_or_selected(s, a);
                let g = &mut s.graph;
                let start = match a.get("startJoint").and_then(Value::as_str) {
                    Some(n) => find(g, n, "ikHandle")?,
                    None => match names.first() {
                        Some(n) => find(g, n, "ikHandle")?,
                        None => fail!("ikHandle: -sj and -ee, or select two joints"),
                    },
                };
                let end = match a.get("endEffector").and_then(Value::as_str) {
                    Some(n) => find(g, n, "ikHandle")?,
                    None => match names.get(1) {
                        Some(n) => find(g, n, "ikHandle")?,
                        None => fail!("ikHandle: -sj and -ee, or select two joints"),
                    },
                };
                let mut chain = vec![end];
                let mut j = end;
                while j != start {
                    j = g.parent(j);
                    if j == NO_NODE || !is_joint(g, j) {
                        fail!("ikHandle: {} is not below {}", g.path(end), g.path(start));
                    }
                    chain.push(j);
                }
                chain.reverse();
                if chain.len() > MAX_CHAIN {
                    fail!("ikHandle: chains of up to {MAX_CHAIN} joints");
                }
                let solver = a.string("solver", "ikRPsolver");
                g.evaluate();
                let rest: Vec<Value> = chain
                    .iter()
                    .map(|&j| {
                        let (t, jo, r) = (vec3_of(g, j, Transform::TRANSLATE), vec3_of(g, j, Joint::JOINT_ORIENT), vec3_of(g, j, Transform::ROTATE));
                        json!({"t": [t.x, t.y, t.z], "jo": [jo.x, jo.y, jo.z], "r": [r.x, r.y, r.z]})
                    })
                    .collect();
                let end_pos = world(g, end).w_axis.truncate();
                let start_pos = world(g, start).w_axis.truncate();
                // Default pole vector: from the start-to-end line toward the middle joint.
                let mid = chain.get(chain.len() / 2).map(|&m| world(g, m).w_axis.truncate()).unwrap_or(start_pos);
                let line = (end_pos - start_pos).normalize_or_zero();
                let mut pole = (mid - start_pos) - line * (mid - start_pos).dot(line);
                if pole.length() < 1e-6 {
                    pole = DVec3::new(0.0, 0.0, -1.0);
                }
                g.open_chunk("IK Handle");
                let r = (|| -> CmdResult<NodeId> {
                    let ht = types::find("ikHandle").unwrap();
                    let h = g.create_node(&ht, &a.string("name", "ikHandle1"), NO_NODE, true)?;
                    set_vec3(g, h, Transform::TRANSLATE, end_pos)?;
                    set_vec3(g, h, IkHandle::POLE_VECTOR, pole.normalize())?;
                    let st = types::find("ikSolve").unwrap();
                    let sv = g.create_node(&st, "ikSolve1", NO_NODE, true)?;
                    set_json(g, sv, IK_DATA, json!({"joints": rest, "solver": solver}))?;
                    g.connect(&plug(h, Transform::WORLD_MATRIX), &plug(sv, IK_HANDLE), false)?;
                    g.connect(&plug(h, IkHandle::POLE_VECTOR), &plug(sv, IK_POLE), false)?;
                    g.connect(&plug(h, IkHandle::TWIST), &plug(sv, IK_TWIST), false)?;
                    let sp = g.parent(start);
                    if sp != NO_NODE {
                        g.connect(&plug(sp, Transform::WORLD_MATRIX), &plug(sv, IK_PARENT), false)?;
                    }
                    for (i, &j) in chain.iter().enumerate() {
                        g.connect(&plug(sv, IK_OUT + i), &plug(j, Transform::ROTATE), true)?;
                    }
                    g.select(&[h], SelectMode::Replace);
                    Ok(h)
                })();
                g.close_chunk();
                let h = r?;
                Ok(json!([g.path(h), "effector1"]))
            },
        )
        .positional("start and end joints (the selection when -sj/-ee are not given)")
        .flag(f("startJoint", "sj", ArgType::String, "First joint of the chain."))
        .flag(f("endEffector", "ee", ArgType::String, "Last joint of the chain."))
        .flag(f("solver", "sol", ArgType::String, "ikRPsolver (default) or ikSCsolver.").choices(&["ikRPsolver", "ikSCsolver"]))
        .flag(f("name", "n", ArgType::String, "Name of the handle.")),
    );
}

fn constrain(s: &mut Session, a: &Args, cmd: &str, kind: usize) -> CmdResult {
    let names = objects(s, a);
    let g = &mut s.graph;
    if names.len() < 2 {
        fail!("{cmd}: name or select the targets, then the object to drive");
    }
    let ids: Vec<NodeId> = names.iter().map(|n| find(g, n, cmd)).collect::<CmdResult<_>>()?;
    let driven = *ids.last().unwrap();
    let targets = &ids[..ids.len() - 1];
    if !g.node(driven).is_some_and(|n| n.ty().is_a("transform")) {
        fail!("{cmd}: {} is not a transform", names.last().unwrap());
    }
    g.evaluate();
    let dw = world(g, driven);
    let mo = a.bool("maintainOffset");
    let offsets: Vec<Value> = targets
        .iter()
        .map(|&t| {
            let tw = world(g, t);
            let off = if !mo {
                DMat4::IDENTITY
            } else {
                match kind {
                    1 => DMat4::from_translation(dw.w_axis.truncate() - tw.w_axis.truncate()),
                    2 => DMat4::from_mat3(rot3(&tw).transpose() * rot3(&dw)),
                    3 => {
                        let (_, _, ts) = decompose(&tw);
                        let (_, _, ds) = decompose(&dw);
                        DMat4::from_scale(ds / ts.max(DVec3::splat(1e-9)))
                    }
                    4 => {
                        // Aim: the driven object's rotation relative to the aim frame now.
                        let aim = (tw.w_axis.truncate() - dw.w_axis.truncate()).normalize_or_zero();
                        let up = if aim.cross(DVec3::Y).length() > 1e-6 { DVec3::Y } else { DVec3::Z };
                        let y = (up - aim * up.dot(aim)).normalize_or_zero();
                        let fr = DMat3::from_cols(aim, y, aim.cross(y));
                        DMat4::from_mat3(fr.transpose() * rot3(&dw))
                    }
                    _ => tw.inverse() * dw,
                }
            };
            mat_json(&off)
        })
        .collect();
    let w = a.float("weight", 1.0);
    let order = g.node(driven).map(|n| n.data().int(Transform::ROTATE_ORDER)).unwrap_or(0);
    let data = json!({"offsets": offsets, "weights": vec![w; targets.len()], "rotateOrder": order, "rest": mat_json(&dw)});
    let dname = g.node(driven).map(|n| n.name.clone()).unwrap_or_default();
    g.open_chunk(cmd);
    let r = (|| -> CmdResult<NodeId> {
        let t = types::find(cmd).unwrap();
        let default_name = format!("{dname}_{cmd}1");
        let c = g.create_node(&t, &a.string("name", &default_name), NO_NODE, true)?;
        set_json(g, c, CN_DATA, data)?;
        for (i, &t) in targets.iter().enumerate() {
            g.connect(&plug(t, Transform::WORLD_MATRIX), &PlugRef::element(c, CN_TARGET as i32, i as i32), false)?;
        }
        let p = g.parent(driven);
        if p != NO_NODE {
            g.connect(&plug(p, Transform::WORLD_MATRIX), &plug(c, CN_PARENT), false)?;
        }
        let outs: &[(usize, usize)] = match kind {
            0 => &[(CN_T, Transform::TRANSLATE), (CN_R, Transform::ROTATE)],
            1 => &[(CN_T, Transform::TRANSLATE)],
            3 => &[(CN_S, Transform::SCALE)],
            _ => &[(CN_R, Transform::ROTATE)],
        };
        for &(o, attr) in outs {
            g.connect(&plug(c, o), &plug(driven, attr), true)?;
        }
        Ok(c)
    })();
    g.close_chunk();
    let c = r?;
    Ok(json!([g.path(c)]))
}
