//! Cameras: the camera shape with Maya's attributes and the camera command. The four default cameras (persp, top, front,
//! side) and the default sets belong to the application's scene, made by `make_defaults`.

use std::any::Any;
use std::sync::Arc;

use aztek_ops::{fail, ArgType, CmdResult, CommandDef, Commands, FlagDef};
use glam::{DMat3, DVec3};
use serde_json::{json, Value};

use crate::attr_defs::{bool_attr, float_attr_short};
use crate::graph::{Graph, SelectMode};
use crate::node::{types, EvalContext, NodeBehavior, NodeData, NodeType};
use crate::plug::{flags, NodeId, PlugValue, NO_NODE};
use crate::rig::plug;
use crate::session::Session;
use crate::transform::{DagNode, Shape, Transform};

pub const TYPE: &str = "camera";
pub const FOCAL_LENGTH: usize = Shape::COUNT;
pub const H_APERTURE: usize = Shape::COUNT + 1;
pub const V_APERTURE: usize = Shape::COUNT + 2;
pub const NEAR_CLIP: usize = Shape::COUNT + 3;
pub const FAR_CLIP: usize = Shape::COUNT + 4;
pub const ORTHOGRAPHIC: usize = Shape::COUNT + 5;
pub const ORTHO_WIDTH: usize = Shape::COUNT + 6;
pub const COI: usize = Shape::COUNT + 7;
pub const LENS_SQUEEZE: usize = Shape::COUNT + 8;

struct CameraShape;

impl NodeBehavior for CameraShape {
    fn compute(&mut self, _ctx: &EvalContext, d: &mut NodeData) {
        Shape::compute_matrices(d);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub fn register_types() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        types::register_builtins();
        let shape = types::find("shape").expect("shape");
        let stored = |d: crate::plug::AttrDef| crate::plug::AttrDef { flags: flags::INPUT | flags::STORABLE, ..d };
        types::register_type(NodeType {
            name: TYPE.into(),
            base: Some(shape),
            dag: true,
            icon: "camera".into(),
            doc: "A camera: what a view looks through.".into(),
            own_attrs: vec![
                float_attr_short("focalLength", "fl", 35.0, "Focal length, mm.", Some(2.5), Some(100000.0)),
                stored(float_attr_short("horizontalFilmAperture", "hfa", 1.41732, "Film width, inches.", Some(0.0001), None)),
                stored(float_attr_short("verticalFilmAperture", "vfa", 0.94488, "Film height, inches.", Some(0.0001), None)),
                stored(float_attr_short("nearClipPlane", "ncp", 0.1, "Near clip distance.", Some(0.0), None)),
                stored(float_attr_short("farClipPlane", "fcp", 10000.0, "Far clip distance.", Some(0.0), None)),
                stored(bool_attr("orthographic", false, "No perspective.")),
                stored(float_attr_short("orthographicWidth", "ow", 30.0, "View width when orthographic.", Some(1e-6), None)),
                stored(float_attr_short("centerOfInterest", "coi", 5.0, "Distance to what the camera tumbles about.", Some(1e-6), None)),
                stored(float_attr_short("lensSqueezeRatio", "lsr", 1.0, "Lens squeeze.", Some(0.0), None)),
            ],
            create: Some(Arc::new(|| Box::new(CameraShape))),
            ..Default::default()
        })
        .expect(TYPE);
    });
}

/// The shape under a transform, or the shape itself.
pub fn shape_of(g: &Graph, id: NodeId) -> Option<NodeId> {
    crate::dag::shape_of_type(g, id, TYPE)
}

fn create(g: &mut Graph, name: &str, parent: NodeId) -> CmdResult<(NodeId, NodeId)> {
    let (tr, sh) = crate::dag::create_with_shape(g, TYPE, name, parent)?;
    Ok((tr, sh))
}

/// A camera's rotate for a view direction: Euler XYZ with no roll, looking down -Z.
pub fn rotate_for(yaw: f64, pitch: f64) -> DVec3 {
    DVec3::new(-pitch, yaw, 0.0)
}

/// Makes persp, top, front and side, hidden as Maya's are, plus the default sets. None of it goes on the undo stack.
pub fn make_defaults(g: &mut Graph) {
    crate::shading::make_defaults(g);
    if g.find("persp").ok().flatten().is_some() {
        return;
    }
    g.without_undo(|g| {
        let mut made = |name: &str, t: DVec3, r: DVec3, ortho: bool, coi: f64| {
            if let Ok((tr, sh)) = create(g, name, NO_NODE) {
                let _ = g.set_value(plug(tr, Transform::TRANSLATE), PlugValue::Vec3(t));
                let _ = g.set_value(plug(tr, Transform::ROTATE), PlugValue::Vec3(r));
                let _ = g.set_value(plug(tr, DagNode::VISIBILITY), PlugValue::Bool(false));
                let _ = g.set_value(plug(sh, ORTHOGRAPHIC), PlugValue::Bool(ortho));
                let _ = g.set_value(plug(sh, COI), PlugValue::Float(coi));
            }
        };
        made("persp", DVec3::new(28.0, 21.0, 28.0), DVec3::new(-27.9, 45.0, 0.0), false, 44.8);
        made("top", DVec3::new(0.0, 1000.1, 0.0), DVec3::new(-90.0, 0.0, 0.0), true, 1000.1);
        made("front", DVec3::new(0.0, 0.0, 1000.1), DVec3::ZERO, true, 1000.1);
        made("side", DVec3::new(1000.1, 0.0, 0.0), DVec3::new(0.0, 90.0, 0.0), true, 1000.1);
        for set in ["defaultLightSet", "defaultObjectSet"] {
            let _ = crate::object_sets::create(g, set, &[], false);
        }
        g.select(&[], SelectMode::Replace);
    });
}

/// Whether a node is one of the four default cameras (or a shape of one).
pub fn is_default_camera(g: &Graph, id: NodeId) -> bool {
    let tr = crate::dag::transform_of(g, id);
    g.node(tr).is_some_and(|n| matches!(n.name.as_str(), "persp" | "top" | "front" | "side")) && shape_of(g, tr).is_some()
}

type Cmd = CommandDef<Session>;

fn f(name: &str, short: &str, ty: ArgType, doc: &str) -> FlagDef {
    FlagDef::new(name, short, ty, Value::Null, doc)
}

fn asked(a: &aztek_ops::Args, name: &str) -> bool {
    a.bool(name) || a.get("queried").and_then(Value::as_array).is_some_and(|q| q.iter().any(|x| x == name))
}

const NUMS: &[(&str, usize)] = &[
    ("focalLength", FOCAL_LENGTH),
    ("horizontalFilmAperture", H_APERTURE),
    ("verticalFilmAperture", V_APERTURE),
    ("nearClipPlane", NEAR_CLIP),
    ("farClipPlane", FAR_CLIP),
    ("orthographicWidth", ORTHO_WIDTH),
    ("centerOfInterest", COI),
    ("lensSqueezeRatio", LENS_SQUEEZE),
];

pub fn register(c: &mut Commands<Session>) {
    register_types();
    c.register(
        Cmd::new(
            "camera",
            "Create a camera, or query and edit one: -fl focal length, -hfa -vfa film, -ncp -fcp clips, -o orthographic, -ow width, \
             -coi, -p position, -rot rotation, -wci world centre of interest. Returns [camera1, cameraShape1].",
            |s, a| {
                let names = a.named();
                let g = &mut s.graph;
                if a.bool("query") || a.bool("edit") {
                    let n = match names.first() {
                        Some(n) => n.clone(),
                        None => match g.selection().last() {
                            Some(&i) => g.path(i),
                            None => fail!("camera: name a camera"),
                        },
                    };
                    let id = crate::rig::find(g, &n, "camera")?;
                    let Some(sh) = shape_of(g, id) else { fail!("camera: {n} is not a camera") };
                    let tr = crate::dag::transform_of(g, sh);
                    if a.bool("query") {
                        for &(flag, at) in NUMS {
                            if asked(a, flag) {
                                return Ok(json!(g.node(sh).unwrap().data().float(at)));
                            }
                        }
                        if asked(a, "orthographic") {
                            return Ok(json!(g.node(sh).unwrap().data().bool(ORTHOGRAPHIC)));
                        }
                        g.evaluate();
                        let w = crate::rig::world(g, tr);
                        if asked(a, "position") {
                            let p = w.w_axis.truncate();
                            return Ok(json!([p.x, p.y, p.z]));
                        }
                        if asked(a, "rotation") {
                            let r = g.node(tr).unwrap().data().vec3(Transform::ROTATE);
                            return Ok(json!([r.x, r.y, r.z]));
                        }
                        if asked(a, "worldCenterOfInterest") {
                            let d = g.node(sh).unwrap().data().float(COI);
                            let p = w.w_axis.truncate() - w.z_axis.truncate().normalize_or_zero() * d;
                            return Ok(json!([p.x, p.y, p.z]));
                        }
                        return Ok(Value::Null);
                    }
                    return apply(g, a, tr, sh).map(|_| Value::Null);
                }
                g.open_chunk("Camera");
                let r = (|| -> CmdResult<(NodeId, NodeId)> {
                    let (tr, sh) = create(g, &a.string("name", "camera1"), NO_NODE)?;
                    apply(g, a, tr, sh)?;
                    Ok((tr, sh))
                })();
                g.close_chunk();
                let (tr, sh) = r?;
                g.select(&[tr], SelectMode::Replace);
                Ok(json!([g.path(tr), g.path(sh)]))
            },
        )
        .positional("a camera, with -q and -e")
        .flag(f("focalLength", "fl", ArgType::Float, "Focal length, mm."))
        .flag(f("horizontalFilmAperture", "hfa", ArgType::Float, "Film width, inches."))
        .flag(f("verticalFilmAperture", "vfa", ArgType::Float, "Film height, inches."))
        .flag(f("nearClipPlane", "ncp", ArgType::Float, "Near clip."))
        .flag(f("farClipPlane", "fcp", ArgType::Float, "Far clip."))
        .flag(f("orthographic", "o", ArgType::Bool, "No perspective."))
        .flag(f("orthographicWidth", "ow", ArgType::Float, "View width when orthographic."))
        .flag(f("centerOfInterest", "coi", ArgType::Float, "Distance to what it tumbles about."))
        .flag(f("lensSqueezeRatio", "lsr", ArgType::Float, "Lens squeeze."))
        .flag(f("position", "p", ArgType::Vec3, "World position."))
        .flag(f("rotation", "rot", ArgType::Vec3, "Rotation, degrees."))
        .flag(f("worldCenterOfInterest", "wci", ArgType::Vec3, "The point it looks at."))
        .flag(f("worldUp", "wup", ArgType::Vec3, "Accepted for Maya scripts."))
        .flag(f("name", "n", ArgType::String, "Name."))
        .flag(f("query", "q", ArgType::Bool, "Query mode."))
        .flag(f("edit", "e", ArgType::Bool, "Edit mode."))
        .flag(f("queried", "", ArgType::List, "Flags asked about in query mode.")),
    );
}

fn apply(g: &mut Graph, a: &aztek_ops::Args, tr: NodeId, sh: NodeId) -> CmdResult<()> {
    for &(flag, at) in NUMS {
        if let Some(v) = a.get(flag).and_then(Value::as_f64) {
            g.set_value(plug(sh, at), PlugValue::Float(v))?;
        }
    }
    if a.has("orthographic") {
        g.set_value(plug(sh, ORTHOGRAPHIC), PlugValue::Bool(a.bool("orthographic")))?;
    }
    if let Some(p) = a.vec3("position") {
        g.set_value(plug(tr, Transform::TRANSLATE), PlugValue::Vec3(DVec3::from_array(p)))?;
    }
    if let Some(r) = a.vec3("rotation") {
        g.set_value(plug(tr, Transform::ROTATE), PlugValue::Vec3(DVec3::from_array(r)))?;
    }
    if let Some(t) = a.vec3("worldCenterOfInterest") {
        g.evaluate();
        let eye = crate::rig::world(g, tr).w_axis.truncate();
        let d = eye - DVec3::from_array(t);
        let len = d.length();
        if len > 1e-9 {
            let z = d / len;
            let yaw = z.x.atan2(z.z).to_degrees();
            let pitch = z.y.clamp(-1.0, 1.0).asin().to_degrees();
            g.set_value(plug(tr, Transform::ROTATE), PlugValue::Vec3(rotate_for(yaw, pitch)))?;
            g.set_value(plug(sh, COI), PlugValue::Float(len))?;
        }
    }
    let _ = DMat3::IDENTITY;
    Ok(())
}
