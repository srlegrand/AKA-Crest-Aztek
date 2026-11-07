//! Port of `src/scene/Transform.h` / `Transform.cpp`.

use std::any::Any;
use std::sync::Arc;

use glam::{DMat3, DMat4, DVec3};

use crate::error::Result;
use crate::graph::Graph;
use crate::node::{vec3_attr, EvalContext, Node, NodeBehavior, NodeData, NodeType, IN_STORE};
use crate::plug::{flags, AttrDef, NodeId, PlugRef, PlugType, PlugValue, NO_NODE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotateOrder {
    Xyz,
    Yzx,
    Zxy,
    Xzy,
    Yxz,
    Zyx,
}

impl RotateOrder {
    pub fn from_index(i: i64) -> Self {
        [
            Self::Xyz,
            Self::Yzx,
            Self::Zxy,
            Self::Xzy,
            Self::Yxz,
            Self::Zyx,
        ][i.clamp(0, 5) as usize]
    }
    /// Axis indices in application order.
    fn axes(self) -> [usize; 3] {
        match self {
            Self::Xyz => [0, 1, 2],
            Self::Yzx => [1, 2, 0],
            Self::Zxy => [2, 0, 1],
            Self::Xzy => [0, 2, 1],
            Self::Yxz => [1, 0, 2],
            Self::Zyx => [2, 1, 0],
        }
    }
}

const ORDER_NAMES: [&str; 6] = ["xyz", "yzx", "zxy", "xzy", "yxz", "zyx"];

fn axis_rotation(axis: usize, radians: f64) -> DMat4 {
    match axis {
        0 => DMat4::from_rotation_x(radians),
        1 => DMat4::from_rotation_y(radians),
        _ => DMat4::from_rotation_z(radians),
    }
}

/// Rotation matrix from Euler degrees; XYZ means X is applied first.
pub fn euler_to_matrix(degrees: DVec3, order: RotateOrder) -> DMat4 {
    let a = order.axes();
    let r = degrees * (std::f64::consts::PI / 180.0);
    // First-applied rotation sits rightmost.
    axis_rotation(a[2], r[a[2]]) * axis_rotation(a[1], r[a[1]]) * axis_rotation(a[0], r[a[0]])
}

/// Euler degrees of a rotation, the middle angle in [-90, 90].
///
/// The C++ maps each order onto one of glm's `extractEulerAngle*` functions. This solves all six orders with one
/// parity-signed formula instead; both give the same angles wherever the rotation is not at gimbal lock.
pub fn matrix_to_euler(rot: &DMat3, order: RotateOrder) -> DVec3 {
    let [i, j, k] = order.axes();
    // Row-major access: m(r, c).
    let m = |r: usize, c: usize| rot.col(c)[r];
    let even = matches!(
        order,
        RotateOrder::Xyz | RotateOrder::Yzx | RotateOrder::Zxy
    );
    let s = if even { 1.0 } else { -1.0 };
    let cos_b = (m(i, i) * m(i, i) + m(j, i) * m(j, i)).sqrt();
    let b = (-s * m(k, i)).atan2(cos_b);
    let (a, c) = if cos_b > 1e-12 {
        ((s * m(k, j)).atan2(m(k, k)), (s * m(j, i)).atan2(m(i, i)))
    } else {
        // Gimbal lock: only a and c together are known; put it all in the first-applied axis.
        ((-s * m(j, k)).atan2(m(j, j)), 0.0)
    };
    let mut out = DVec3::ZERO;
    out[i] = a;
    out[j] = b;
    out[k] = c;
    out * (180.0 / std::f64::consts::PI)
}

/// Euler angles of `rot` in `order`, wrapped and flipped to lie closest to `reference`, so sampled rotations stay
/// continuous.
pub fn euler_near(rot: &DMat3, order: RotateOrder, reference: DVec3) -> DVec3 {
    let unwrap = |mut e: DVec3| {
        for i in 0..3 {
            e[i] += 360.0 * ((reference[i] - e[i]) / 360.0).round();
        }
        e
    };
    let mut a = matrix_to_euler(rot, order);
    let s = order.axes();
    // The other Euler solution for the same rotation.
    let mut b = a;
    b[s[0]] += 180.0;
    b[s[1]] = 180.0 - b[s[1]];
    b[s[2]] += 180.0;
    a = unwrap(a);
    b = unwrap(b);
    let dist = |e: DVec3| (e - reference).dot(e - reference);
    if dist(b) >= dist(a) {
        return a;
    }
    let check = DMat3::from_mat4(euler_to_matrix(b, order));
    let err: f64 = (0..3).map(|c| (check.col(c) - rot.col(c)).length()).sum();
    if err < 1e-6 {
        b
    } else {
        a
    }
}

/// How new rotate keys are stored: three Euler curves, or one rotation curve with slerp or smooth interpolation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotationInterpolation {
    Euler,
    Slerp,
    Smooth,
}

/// The transform's rotationInterpolation attr, with `def` as its default (joints default to Smooth).
pub fn rotation_interpolation_attr(def: i64) -> AttrDef {
    AttrDef {
        name: "rotationInterpolation".into(),
        short_name: "rin".into(),
        ty: PlugType::Enum,
        default_value: PlugValue::Int(def),
        flags: IN_STORE,
        enum_names: vec!["euler".into(), "slerp".into(), "smooth".into()],
        enum_docs: vec![
            "Three curves, one per axis. Can flip near gimbal lock.".into(),
            "One rotation curve: constant speed along the shortest arc between keys.".into(),
            "One rotation curve, cubic in quaternion space: eases through keys, honouring tangents.".into(),
        ],
        doc: "How new rotate keys are stored: euler (three curves), or one rotation curve interpolated by slerp or smooth \
              (cubic in quaternion space). convertRotationCurves converts existing keys."
            .into(),
        ..Default::default()
    }
}

/// The pref (file "general") that makes a hidden parent hide its children, as the viewports draw them.
pub const INHERIT_VISIBILITY_PREF: &str = "/display/inheritVisibility";

/// Whether hidden parents hide their children (the "general" inheritVisibility pref, default on).
pub fn inherit_visibility() -> bool {
    aztek_core::PrefStore::get().bool("general", INHERIT_VISIBILITY_PREF, true)
}

/// Whether the viewport shows a node: its own visibility and display layer, and every DAG ancestor's too when
/// `inherit` is on.
pub fn effectively_visible_with(g: &Graph, id: NodeId, inherit: bool) -> bool {
    let mut n = id;
    while n != NO_NODE {
        if let Some(node) = g.node(n) {
            if node.ty().dag && (!node.bool(Transform::VISIBILITY) || crate::display_layers::hidden_by_own_layer(g, n)) {
                return false;
            }
        }
        n = if inherit { g.parent(n) } else { NO_NODE };
    }
    true
}

/// effectively_visible_with under the inherit visibility pref.
pub fn effectively_visible(g: &Graph, id: NodeId) -> bool {
    effectively_visible_with(g, id, inherit_visibility())
}

/// The matrix terms a transform-like node contributes. Joints override `pre_rotation` (jointOrient) and `rotation`
/// (IK blend) when they are ported.
pub trait TransformBehavior {
    /// Rotation applied between rotate and the parent: a joint's jointOrient, identity otherwise.
    fn pre_rotation(&self, _d: &NodeData) -> DMat4 {
        DMat4::IDENTITY
    }
    /// The R term of the local matrix: rotate in rotateOrder.
    fn rotation(&self, d: &NodeData) -> DMat4 {
        euler_to_matrix(
            d.vec3(Transform::ROTATE),
            RotateOrder::from_index(d.int(Transform::ROTATE_ORDER)),
        )
    }
    /// The inverse of the parent's scale, between translate and the rotation terms: a joint's segment scale compensate.
    /// Identity otherwise.
    fn inverse_parent_scale(&self, _d: &NodeData) -> DMat4 {
        DMat4::IDENTITY
    }
    /// rotateAxis as a matrix (XYZ order), applied before rotate as Maya does.
    fn rotate_axis_matrix(&self, d: &NodeData) -> DMat4 {
        euler_to_matrix(d.vec3(Transform::ROTATE_AXIS), RotateOrder::Xyz)
    }
}

/// Maya's dagNode: what every node in the hierarchy has, transforms and shapes alike. Abstract.
#[derive(Clone, Default)]
pub struct DagNode;

impl DagNode {
    pub const VISIBILITY: usize = 0;
    pub const PARENT_MATRIX: usize = 1;
    pub const WORLD_MATRIX: usize = 2;
    pub const DISPLAY_LAYER: usize = 3;
    pub const INTERMEDIATE_OBJECT: usize = 4;
    pub const OVERRIDE_ENABLED: usize = 5;
    pub const OVERRIDE_DISPLAY_TYPE: usize = 6;
    pub const OVERRIDE_COLOR: usize = 7;
    pub const OVERRIDE_RGB_COLORS: usize = 8;
    pub const OVERRIDE_COLOR_RGB: usize = 9;
    pub const GHOSTING: usize = 10;
    pub const GHOST_PRE_FRAMES: usize = 11;
    pub const GHOST_POST_FRAMES: usize = 12;
    pub const GHOST_STEP: usize = 13;
    pub const COUNT: usize = 14;

    pub fn make_type() -> NodeType {
        let keyable = flags::INPUT | flags::STORABLE | flags::KEYABLE;
        NodeType {
            name: "dagNode".into(),
            dag: true,
            doc: "Anything in the hierarchy: transforms and the shapes under them.".into(),
            own_attrs: vec![
                AttrDef {
                    name: "visibility".into(),
                    short_name: "v".into(),
                    ty: PlugType::Bool,
                    default_value: PlugValue::Bool(true),
                    flags: keyable,
                    ..Default::default()
                },
                AttrDef {
                    name: "parentMatrix".into(),
                    short_name: "pm".into(),
                    ty: PlugType::Matrix,
                    default_value: PlugValue::Matrix(DMat4::IDENTITY),
                    flags: flags::HIDDEN,
                    doc: "World matrix of the frame translate, rotate and scale live in: the parent's, or the active space's."
                        .into(),
                    ..Default::default()
                },
                AttrDef {
                    name: "worldMatrix".into(),
                    short_name: "wm".into(),
                    ty: PlugType::Matrix,
                    default_value: PlugValue::Matrix(DMat4::IDENTITY),
                    flags: flags::OUTPUT,
                    doc: "Object-to-world matrix.".into(),
                    ..Default::default()
                },
                AttrDef {
                    name: "displayLayer".into(),
                    short_name: "dl".into(),
                    ty: PlugType::Int,
                    default_value: PlugValue::Int(0),
                    flags: flags::INPUT | flags::HIDDEN,
                    doc: "Display layer membership: connected from a displayLayer's drawInfo.".into(),
                    ..Default::default()
                },
                AttrDef {
                    name: "intermediateObject".into(),
                    short_name: "io".into(),
                    ty: PlugType::Bool,
                    default_value: PlugValue::Bool(false),
                    flags: IN_STORE,
                    doc: "A shape kept for history (a deformer's original) that is not drawn or picked.".into(),
                    ..Default::default()
                },
                AttrDef {
                    name: "overrideEnabled".into(),
                    short_name: "ove".into(),
                    ty: PlugType::Bool,
                    default_value: PlugValue::Bool(false),
                    flags: IN_STORE,
                    doc: "Use this node's own display settings: colour and display type.".into(),
                    ..Default::default()
                },
                AttrDef {
                    name: "overrideDisplayType".into(),
                    short_name: "ovdt".into(),
                    ty: PlugType::Enum,
                    default_value: PlugValue::Int(0),
                    flags: IN_STORE,
                    enum_names: vec!["Normal".into(), "Template".into(), "Reference".into()],
                    doc: "Normal, Template (grey, unpickable) or Reference (unpickable).".into(),
                    ..Default::default()
                },
                AttrDef {
                    name: "overrideColor".into(),
                    short_name: "ovc".into(),
                    ty: PlugType::Int,
                    default_value: PlugValue::Int(0),
                    flags: IN_STORE,
                    doc: "Index into Maya's colour table, 0 to 31.".into(),
                    min_value: Some(0.0),
                    max_value: Some(31.0),
                    ..Default::default()
                },
                AttrDef {
                    name: "overrideRGBColors".into(),
                    short_name: "ovrgbf".into(),
                    ty: PlugType::Bool,
                    default_value: PlugValue::Bool(false),
                    flags: IN_STORE,
                    doc: "Take the colour from overrideColorRGB instead of the index.".into(),
                    ..Default::default()
                },
                crate::node::vec3_attr("overrideColorRGB", "ovrgb", DVec3::ZERO, IN_STORE, "The override colour, 0 to 1 per channel."),
                AttrDef {
                    name: "ghosting".into(),
                    short_name: "gh".into(),
                    ty: PlugType::Bool,
                    default_value: PlugValue::Bool(false),
                    flags: IN_STORE,
                    doc: "Show this object at the frames around the current one.".into(),
                    ..Default::default()
                },
                AttrDef {
                    name: "ghostPreSteps".into(),
                    short_name: "gprs".into(),
                    ty: PlugType::Int,
                    default_value: PlugValue::Int(3),
                    flags: IN_STORE,
                    doc: "How many ghosts before the current frame.".into(),
                    ..Default::default()
                },
                AttrDef {
                    name: "ghostPostSteps".into(),
                    short_name: "gpos".into(),
                    ty: PlugType::Int,
                    default_value: PlugValue::Int(3),
                    flags: IN_STORE,
                    doc: "How many ghosts after the current frame.".into(),
                    ..Default::default()
                },
                AttrDef {
                    name: "ghostStepSize".into(),
                    short_name: "gss".into(),
                    ty: PlugType::Int,
                    default_value: PlugValue::Int(1),
                    flags: IN_STORE,
                    doc: "Frames between ghosts.".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }
}

/// Maya's shape: a DAG leaf under a transform. Its world matrix is its parent's. Abstract.
#[derive(Clone, Default)]
pub struct Shape;

impl Shape {
    pub const COUNT: usize = DagNode::COUNT;

    pub fn make_type(base: Arc<NodeType>) -> NodeType {
        NodeType {
            name: "shape".into(),
            base: Some(base),
            dag: true,
            doc: "Geometry or a glyph drawn at its transform.".into(),
            ..Default::default()
        }
    }

    /// What every shape computes: its world matrix from its parent.
    pub fn compute_matrices(d: &mut NodeData) {
        let parent = d.matrix(DagNode::PARENT_MATRIX);
        d.set_output(DagNode::WORLD_MATRIX, parent);
    }
}

#[derive(Clone, Default)]
pub struct Transform;

impl Transform {
    pub const VISIBILITY: usize = DagNode::VISIBILITY;
    pub const PARENT_MATRIX: usize = DagNode::PARENT_MATRIX;
    pub const WORLD_MATRIX: usize = DagNode::WORLD_MATRIX;
    pub const DISPLAY_LAYER: usize = DagNode::DISPLAY_LAYER;
    pub const INTERMEDIATE_OBJECT: usize = DagNode::INTERMEDIATE_OBJECT;
    pub const TRANSLATE: usize = DagNode::COUNT;
    pub const ROTATE: usize = DagNode::COUNT + 1;
    pub const SCALE: usize = DagNode::COUNT + 2;
    pub const ROTATE_ORDER: usize = DagNode::COUNT + 3;
    pub const ROTATE_PIVOT: usize = DagNode::COUNT + 4;
    pub const SCALE_PIVOT: usize = DagNode::COUNT + 5;
    pub const ROTATE_PIVOT_TRANSLATE: usize = DagNode::COUNT + 6;
    pub const SCALE_PIVOT_TRANSLATE: usize = DagNode::COUNT + 7;
    pub const MATRIX: usize = DagNode::COUNT + 8;
    pub const PIVOT_ORIENT: usize = DagNode::COUNT + 9;
    pub const ROTATE_AXIS: usize = DagNode::COUNT + 10;
    pub const ROTATION_INTERPOLATION: usize = DagNode::COUNT + 11;
    pub const SPACE: usize = DagNode::COUNT + 12;
    pub const SPACE_FRAMES: usize = DagNode::COUNT + 13;
    pub const COUNT: usize = DagNode::COUNT + 14;

    /// Maya's pivot model, column vectors:
    /// T * Rpt * Rp * Pre * R * Ra * Rp^-1 * Spt * Sp * S * Sp^-1  (Pre = pre_rotation, Ra = rotateAxis)
    pub fn local_matrix(n: &Node) -> DMat4 {
        Self::local_matrix_of(n.behavior(), n.data())
    }

    pub fn local_matrix_of(b: &dyn NodeBehavior, d: &NodeData) -> DMat4 {
        let r = match b.as_transform() {
            Some(t) => t.rotation(d),
            None => euler_to_matrix(
                d.vec3(Self::ROTATE),
                RotateOrder::from_index(d.int(Self::ROTATE_ORDER)),
            ),
        };
        Self::local_matrix_with(b, d, r)
    }

    /// `local_matrix` with `rotation` standing in for the R term (rotate, or a joint's IK blend).
    pub fn local_matrix_with(b: &dyn NodeBehavior, d: &NodeData, rotation: DMat4) -> DMat4 {
        let (before, after) = Self::split_local(b, d);
        before * rotation * after
    }

    /// The terms either side of R, so `local_matrix_with(n, R) == before * R * after`.
    pub fn split_local(b: &dyn NodeBehavior, d: &NodeData) -> (DMat4, DMat4) {
        let tr = DMat4::from_translation;
        let rp = d.vec3(Self::ROTATE_PIVOT);
        let sp = d.vec3(Self::SCALE_PIVOT);
        let t = b.as_transform();
        let pre = t.map(|t| t.pre_rotation(d)).unwrap_or(DMat4::IDENTITY);
        let axis = t
            .map(|t| t.rotate_axis_matrix(d))
            .unwrap_or(DMat4::IDENTITY);
        let inverse_scale = t
            .map(|t| t.inverse_parent_scale(d))
            .unwrap_or(DMat4::IDENTITY);
        let before = tr(d.vec3(Self::TRANSLATE))
            * tr(d.vec3(Self::ROTATE_PIVOT_TRANSLATE))
            * tr(rp)
            * inverse_scale
            * pre;
        let after = axis
            * tr(-rp)
            * tr(d.vec3(Self::SCALE_PIVOT_TRANSLATE))
            * tr(sp)
            * DMat4::from_scale(d.vec3(Self::SCALE))
            * tr(-sp);
        (before, after)
    }

    /// Sets translate/rotate/scale (undoably) so the local matrix becomes `m`, keeping pivots. Shear is dropped.
    pub fn set_local_matrix(g: &mut Graph, id: NodeId, m: &DMat4) -> Result<()> {
        let n = g
            .node(id)
            .ok_or_else(|| crate::error::GraphError::invalid("no such node"))?;
        // A joint's compensated parent scale sits inside m: what is left is its own rotation and scale.
        let compensated = n
            .behavior()
            .as_transform()
            .map(|t| t.inverse_parent_scale(n.data()))
            .unwrap_or(DMat4::IDENTITY);
        let (scale, rotation, translation) = decompose(&(compensated.inverse() * *m));
        let translation = if compensated == DMat4::IDENTITY { translation } else { m.w_axis.truncate() };
        // Shapes have no channels of their own: they sit wherever their transform is.
        if !n.ty().is_a("transform") {
            return Ok(());
        }
        let order = RotateOrder::from_index(n.int(Self::ROTATE_ORDER));
        let t = n.behavior().as_transform();
        let pre = t
            .map(|t| DMat3::from_mat4(t.pre_rotation(n.data())))
            .unwrap_or(DMat3::IDENTITY);
        let axis = t
            .map(|t| DMat3::from_mat4(t.rotate_axis_matrix(n.data())))
            .unwrap_or(DMat3::IDENTITY);
        let euler = matrix_to_euler(&(pre.transpose() * rotation * axis.transpose()), order);

        g.set_value(PlugRef::new(id, Self::ROTATE as i32), euler.into())?;
        g.set_value(PlugRef::new(id, Self::SCALE as i32), scale.into())?;

        // With pivots, translate is whatever makes the rest of the chain land on m's translation.
        let n = g.node(id).unwrap();
        let without_t = Self::local_matrix(n);
        let current = n.vec3(Self::TRANSLATE);
        let offset = without_t.w_axis.truncate() - current;
        g.set_value(
            PlugRef::new(id, Self::TRANSLATE as i32),
            (translation - offset).into(),
        )
    }

    pub fn make_type(base: Arc<NodeType>) -> NodeType {
        let keyable = flags::INPUT | flags::STORABLE | flags::KEYABLE;
        let unkeyable = IN_STORE;
        NodeType {
            name: "transform".into(),
            icon: "box-multiple".into(),
            base: Some(base),
            dag: true,
            own_attrs: vec![
                vec3_attr("translate", "t", DVec3::ZERO, keyable, "Position relative to the parent, cm."),
                vec3_attr("rotate", "r", DVec3::ZERO, keyable, "Euler rotation in degrees."),
                vec3_attr("scale", "s", DVec3::ONE, keyable, "Scale."),
                AttrDef {
                    name: "rotateOrder".into(),
                    short_name: "ro".into(),
                    ty: PlugType::Enum,
                    default_value: PlugValue::Int(0),
                    enum_names: ORDER_NAMES.iter().map(|s| s.to_string()).collect(),
                    doc: "Euler order, first-applied axis first.".into(),
                    ..Default::default()
                },
                vec3_attr("rotatePivot", "rp", DVec3::ZERO, unkeyable, "Rotation pivot in local space."),
                vec3_attr("scalePivot", "sp", DVec3::ZERO, unkeyable, "Scale pivot in local space."),
                vec3_attr(
                    "rotatePivotTranslate",
                    "rpt",
                    DVec3::ZERO,
                    unkeyable,
                    "Compensates rotatePivot moves so the object stays put.",
                ),
                vec3_attr(
                    "scalePivotTranslate",
                    "spt",
                    DVec3::ZERO,
                    unkeyable,
                    "Compensates scalePivot moves so the object stays put.",
                ),
                AttrDef {
                    name: "matrix".into(),
                    short_name: "m".into(),
                    ty: PlugType::Matrix,
                    default_value: PlugValue::Matrix(DMat4::IDENTITY),
                    flags: flags::OUTPUT,
                    doc: "Local matrix.".into(),
                    ..Default::default()
                },
                vec3_attr(
                    "pivotOrient",
                    "po",
                    DVec3::ZERO,
                    unkeyable,
                    "Orientation of the pivot relative to the object's axes, Euler XYZ degrees: the manipulator's axes in \
                     object space. Leaves the object where it is.",
                ),
                vec3_attr(
                    "rotateAxis",
                    "ra",
                    DVec3::ZERO,
                    unkeyable,
                    "Rotation applied before rotate, Euler XYZ degrees: orients the rotation axes.",
                ),
                rotation_interpolation_attr(0),
                AttrDef {
                    name: "space".into(),
                    short_name: "spc".into(),
                    ty: PlugType::Enum,
                    default_value: PlugValue::Int(0),
                    flags: keyable | flags::HIDDEN | flags::RESET_ON_DISCONNECT,
                    doc: "The active space (addSpace): translate, rotate and scale are read in its frame. 0 is the parent. \
                          Keyed stepped; shown in the channel box only on nodes with spaces."
                        .into(),
                    ..Default::default()
                },
                AttrDef {
                    name: "spaceFrames".into(),
                    short_name: "spf".into(),
                    ty: PlugType::Data,
                    default_value: PlugValue::Data(None),
                    flags: flags::INPUT | flags::HIDDEN,
                    doc: "Every space's world frame, from the node's spaceSwitch.".into(),
                    ..Default::default()
                },
            ],
            create: Some(Arc::new(|| Box::new(Transform))),
            // The Unreal live link's identity, dropped with the link: older scene files load without it.
            retired_attrs: vec!["linkId".into()],
            ..Default::default()
        }
    }
}

impl TransformBehavior for Transform {}

impl NodeBehavior for Transform {
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

#[derive(Clone, Default)]
pub struct Locator;

impl Locator {
    pub const LOCAL_POSITION: usize = Shape::COUNT;
    pub const LOCAL_SCALE: usize = Shape::COUNT + 1;

    /// The locator shape; base is the shape type.
    pub fn make_type(base: Arc<NodeType>) -> NodeType {
        let local_position = vec3_attr("localPosition", "lp", DVec3::ZERO, IN_STORE, "Where the cross is drawn, object space.");
        let local_scale = vec3_attr("localScale", "ls", DVec3::ONE, IN_STORE, "Size of the cross along each axis.");
        NodeType {
            name: "locator".into(),
            icon: "crosshair".into(),
            base: Some(base),
            dag: true,
            own_attrs: vec![local_position, local_scale],
            create: Some(Arc::new(|| Box::new(Locator))),
            ..Default::default()
        }
    }
}

impl NodeBehavior for Locator {
    fn compute(&mut self, _ctx: &EvalContext, d: &mut NodeData) {
        Shape::compute_matrices(d)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// glm::decompose without perspective: scale, rotation and translation, skew removed. A mirrored matrix gets all
/// three scales negated, as glm does.
pub fn decompose(m: &DMat4) -> (DVec3, DMat3, DVec3) {
    let translation = m.w_axis.truncate();
    let mut rows = [
        m.x_axis.truncate(),
        m.y_axis.truncate(),
        m.z_axis.truncate(),
    ];
    let mut scale = DVec3::ZERO;
    scale.x = rows[0].length();
    rows[0] = rows[0].normalize_or_zero();
    let xy = rows[0].dot(rows[1]);
    rows[1] -= rows[0] * xy;
    scale.y = rows[1].length();
    rows[1] = rows[1].normalize_or_zero();
    let xz = rows[0].dot(rows[2]);
    rows[2] -= rows[0] * xz;
    let yz = rows[1].dot(rows[2]);
    rows[2] -= rows[1] * yz;
    scale.z = rows[2].length();
    rows[2] = rows[2].normalize_or_zero();
    if rows[0].dot(rows[1].cross(rows[2])) < 0.0 {
        scale = -scale;
        for r in &mut rows {
            *r = -*r;
        }
    }
    (
        scale,
        DMat3::from_cols(rows[0], rows[1], rows[2]),
        translation,
    )
}
