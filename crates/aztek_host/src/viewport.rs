//! The 3D view: what to draw from the scene, its GPU buffers, picking, and the move/rotate/scale manipulators.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use aztek_geo::bvh::MeshBvh;
use aztek_geo::render_mesh::{build_render_mesh, RenderMeshOptions};
use aztek_geo::{Index, Mesh};
use aztek_scene::component_pick::{self, Screen};
use aztek_scene::components::{self, ComponentMode};
use aztek_scene::graph::Graph;
use aztek_scene::mesh_nodes::MeshNode;
use aztek_scene::plug::{NodeId, NO_NODE};
use aztek_scene::transform::DagNode;
use aztek_scene::Session;
use ash::vk;
use glam::{DMat3, DMat4, DVec2, DVec3, Vec3};

use crate::camera::{CamKind, Camera};
use crate::gpu::{Buffer, Gpu};
use crate::render::{c8, Kind, LineVertex, Push, Renderer, ShadedVertex};

pub struct DrawItem {
    pub shape: NodeId,
    pub xf: NodeId,
    pub world: DMat4,
    pub mesh: Arc<Mesh>,
    /// Smooth preview levels (the 2 and 3 keys); 0 off.
    pub smooth: i32,
}

fn visible(g: &Graph, id: NodeId) -> bool {
    let mut n = id;
    while n != NO_NODE {
        let Some(node) = g.node(n) else { return false };
        if node.ty().dag && !node.bool(DagNode::VISIBILITY) {
            return false;
        }
        if aztek_scene::display_layers::hidden_by_own_layer(g, n) {
            return false;
        }
        n = g.parent(n);
    }
    true
}

/// Every visible, non-intermediate mesh shape with its evaluated mesh.
pub fn gather(g: &mut Graph) -> Vec<DrawItem> {
    g.evaluate();
    let mut out = Vec::new();
    for id in g.nodes() {
        let Some(n) = g.node(id) else { continue };
        if !n.ty().is_a("mesh") || n.bool(DagNode::INTERMEDIATE_OBJECT) || !visible(g, id) {
            continue;
        }
        let Some(mesh) = n.data().mesh(MeshNode::OUT_MESH).cloned() else { continue };
        let world = n.data().matrix(DagNode::WORLD_MATRIX);
        let smooth = n.int(MeshNode::SMOOTH_PREVIEW).clamp(0, 3) as i32;
        out.push(DrawItem { shape: id, xf: aztek_scene::dag::transform_of(g, id), world, mesh, smooth });
    }
    out
}

struct GpuMesh {
    mesh_key: usize,
    smooth: i32,
    /// The subdivided mesh smooth preview draws.
    display: Option<Arc<Mesh>>,
    tris: Option<(Buffer, Buffer, u32)>,
    /// Per triangle source face, for the selected-face tint.
    tri_faces: Vec<Index>,
    tri_indices: Vec<Index>,
    bvh: Option<MeshBvh>,
    sel_key: u64,
    lines: Option<(Buffer, u32)>,
    points: Option<(Buffer, u32)>,
    tint: Option<(Buffer, u32)>,
    /// One vertex per render vertex's source point, for the weight colours.
    vertex_point: Vec<Index>,
    weight_tris: Option<(Buffer, Buffer, u32)>,
    weight_key: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Select,
    Move,
    Rotate,
    Scale,
    /// Click to place joints, each under the last.
    Joint,
    /// Paint skin weights: the brush on the selected skinned meshes.
    PaintSkin,
    /// CV Curve Tool: clicks place CVs; Enter makes the curve.
    CurveCv,
    /// EP Curve Tool: clicks place edit points; Enter makes the curve.
    CurveEp,
    Sculpt,
    MultiCut,
    InsertLoop,
    OffsetLoop,
    TargetWeld,
    SlideEdge,
    Crease,
    CreatePolygon,
    AppendPolygon,
    QuadDraw,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Handle {
    Axis(usize),
    /// Move in the view plane / uniform scale.
    Center,
}

pub struct Drag {
    pub handle: Handle,
    pub start: DVec2,
    pub pivot: DVec3,
    /// Whether a preview command has run and must be undone before the next.
    pub applied: bool,
    pub components: bool,
    pub targets: Vec<String>,
    /// World points a point-snapping move can land on.
    pub snap_points: Vec<DVec3>,
}

pub struct Viewport {
    /// Held snap: 0 none, 1 grid (X), 2 point (V).
    pub snap: i32,
    /// D held (or Insert): the move manipulator moves pivots.
    pub pivot_mode: bool,
    pub cam: Camera,
    meshes: HashMap<NodeId, GpuMesh>,
    grid: Option<(Buffer, u32)>,
    pub wireframe: bool,
    pub wire_on_shaded: bool,
    pub tool: Tool,
    pub drag: Option<Drag>,
    pub hover: Option<Handle>,
    /// Viewport rect in physical pixels: x, y, w, h.
    pub rect: [f64; 4],
    pub marquee: Option<(DVec2, DVec2)>,
    pub grid_spacing: f64,
    pub show_grid: bool,
    pub manip_scale: f64,
    /// Smooth preview draws the cage too (the 2 key) rather than the smooth mesh alone (3).
    pub smooth_cage: bool,
    /// The component under the cursor: (shape, kind, id).
    pub hover_comp: Option<(NodeId, ComponentMode, Index)>,
    /// The paint brush under the cursor: world centre and normal.
    pub brush: Option<(DVec3, DVec3)>,
    /// A curve tool's points so far, and its degree.
    pub curve_points: Vec<DVec3>,
    pub curve_degree: i64,
    /// Manipulator axis orientation: world, object, component, parent or normal.
    pub axis_mode: String,
    /// The manipulator's axes as columns.
    pub frame: DMat3,
    /// What the modelling tools draw over the scene, world space.
    pub tool_lines: Vec<(DVec3, DVec3, [u8; 4])>,
    pub tool_points: Vec<(DVec3, [u8; 4])>,
}

const LEAD: [u8; 4] = [66, 255, 163, 255];
const SELECTED: [u8; 4] = [255, 255, 255, 255];
const WIRE: [u8; 4] = [10, 10, 30, 255];
const VERT: [u8; 4] = [170, 30, 170, 255];
const SEL_COMP: [u8; 4] = [255, 160, 30, 255];
const UNSEL_EDGE: [u8; 4] = [40, 70, 200, 255];

impl Viewport {
    pub fn new(kind: CamKind) -> Viewport {
        Viewport {
            cam: Camera::new(kind),
            meshes: HashMap::new(),
            grid: None,
            wireframe: false,
            wire_on_shaded: false,
            tool: Tool::Select,
            drag: None,
            hover: None,
            rect: [0.0, 0.0, 1.0, 1.0],
            marquee: None,
            grid_spacing: 1.0,
            snap: 0,
            pivot_mode: false,
            show_grid: true,
            manip_scale: 1.0,
            smooth_cage: false,
            hover_comp: None,
            brush: None,
            curve_points: Vec::new(),
            curve_degree: 3,
            axis_mode: "world".into(),
            frame: DMat3::IDENTITY,
            tool_lines: Vec::new(),
            tool_points: Vec::new(),
        }
    }

    pub fn size(&self) -> DVec2 {
        DVec2::new(self.rect[2].max(1.0), self.rect[3].max(1.0))
    }

    pub fn aspect(&self) -> f64 {
        self.rect[2].max(1.0) / self.rect[3].max(1.0)
    }

    pub fn view_proj(&self) -> DMat4 {
        self.cam.proj(self.aspect()) * self.cam.view()
    }

    pub fn screen(&self) -> Screen {
        Screen { view_proj: self.view_proj(), width: self.rect[2], height: self.rect[3] }
    }

    /// Maya's grid: 12 units each way, a line per unit, darker every 5, black axes; in the view's own plane for the
    /// orthographic cameras.
    fn grid_lines(&self) -> Vec<LineVertex> {
        let n = 12i32;
        let s = self.grid_spacing as f32;
        let ext = n as f32 * s;
        // Maps (u, v) in the grid plane to world.
        let to = |u: f32, v: f32| -> [f32; 3] {
            match self.cam.kind {
                CamKind::Front => [u, v, 0.0],
                CamKind::Side => [0.0, v, -u],
                _ => [u, 0.0, v],
            }
        };
        let mut v = Vec::new();
        // Each line in unit pieces: lines reaching far behind the camera clip badly on some drivers.
        for i in -n..=n {
            let t = i as f32 * s;
            let c = if i == 0 { [0, 0, 0, 255] } else if i % 5 == 0 { [70, 70, 70, 255] } else { [105, 105, 105, 255] };
            for k in -n..n {
                let (a, b) = (k as f32 * s, (k + 1) as f32 * s);
                v.push(LineVertex { pos: to(t, a), color: c8(c) });
                v.push(LineVertex { pos: to(t, b), color: c8(c) });
                v.push(LineVertex { pos: to(a, t), color: c8(c) });
                v.push(LineVertex { pos: to(b, t), color: c8(c) });
            }
        }
        let _ = ext;
        v
    }

    /// Drops GPU buffers of shapes no longer drawn.
    fn prune(&mut self, gpu: &mut Gpu, items: &[DrawItem]) {
        let live: Vec<NodeId> = items.iter().map(|i| i.shape).collect();
        let dead: Vec<NodeId> = self.meshes.keys().copied().filter(|k| !live.contains(k)).collect();
        for k in dead {
            let m = self.meshes.remove(&k).unwrap();
            drop_mesh(gpu, m);
        }
    }

    pub fn draw(&mut self, gpu: &mut Gpu, r: &Renderer, s: &mut Session, items: &[DrawItem], scale: f64) {
        self.prune(gpu, items);
        let [x, y, w, h] = self.rect;
        if w < 2.0 || h < 2.0 {
            return;
        }
        r.set_viewport(gpu, x as f32, y as f32, w as f32, h as f32);
        let vp = self.view_proj();
        let view = self.cam.view();
        if self.grid.is_none() {
            let lines = self.grid_lines();
            self.grid = Some((gpu.buffer_with(vk::BufferUsageFlags::VERTEX_BUFFER, &lines), lines.len() as u32));
        }
        if self.show_grid {
            r.bind(gpu, Kind::Lines);
            r.set_line_width(gpu, 1.0);
            r.push(gpu, &push(vp, DMat4::IDENTITY, view, [1.0; 4], 0.0, 1.0));
            let (gb, gn) = self.grid.as_ref().unwrap();
            r.draw(gpu, gb, None, *gn);
        }

        let g = &s.graph;
        let mode = components::mode(g);
        let sel = g.selection().to_vec();
        let lead = sel.last().copied();
        for it in items {
            let entry = self.meshes.entry(it.shape).or_insert_with(|| GpuMesh {
                mesh_key: 0,
                smooth: 0,
                display: None,
                tris: None,
                tri_faces: Vec::new(),
                tri_indices: Vec::new(),
                bvh: None,
                sel_key: u64::MAX,
                lines: None,
                points: None,
                tint: None,
                vertex_point: Vec::new(),
                weight_tris: None,
                weight_key: u64::MAX,
            });
            let key = Arc::as_ptr(&it.mesh) as usize;
            let mesh_changed = entry.mesh_key != key || entry.smooth != it.smooth;
            if mesh_changed {
                entry.smooth = it.smooth;
                entry.display = (it.smooth > 0).then(|| {
                    let o = aztek_geo::ops::subdivide::SubdivideOptions { levels: it.smooth, ..Default::default() };
                    Arc::new(aztek_geo::ops::subdivide::subdivide(&it.mesh, &o, None))
                });
                if let Some((a, b, _)) = entry.tris.take() {
                    gpu.drop_buffer(a);
                    gpu.drop_buffer(b);
                }
                let shown = entry.display.clone().unwrap_or_else(|| it.mesh.clone());
                let rm = build_render_mesh(&shown, &RenderMeshOptions { max_uv_sets: 0, ..Default::default() });
                let verts: Vec<ShadedVertex> = rm
                    .positions
                    .iter()
                    .zip(&rm.normals)
                    .map(|(p, n)| ShadedVertex { pos: p.to_array(), nrm: n.to_array() })
                    .collect();
                if !rm.indices.is_empty() {
                    entry.tris = Some((
                        gpu.buffer_with(vk::BufferUsageFlags::VERTEX_BUFFER, &verts),
                        gpu.buffer_with(vk::BufferUsageFlags::INDEX_BUFFER, &rm.indices),
                        rm.indices.len() as u32,
                    ));
                }
                entry.vertex_point = rm.vertex_point.clone();
                entry.tri_faces = rm.triangle_faces.clone();
                entry.tri_indices = rm.indices.clone();
                entry.bvh = None;
                entry.mesh_key = key;
                entry.sel_key = u64::MAX;
                // Tint reuses the triangle vertex buffer; rebuilt with the selection key below.
            }
            let selected = sel.contains(&it.xf) || sel.contains(&it.shape);
            let hilited = selected && mode != ComponentMode::Object;
            let comps: &[u32] = if hilited { components::selected(g, it.shape, mode) } else { &[] };
            let comp_set: std::collections::HashSet<u32> = comps.iter().copied().collect();
            let soft = aztek_scene::soft_select::state();
            let soft_on = soft.0 && hilited && !comps.is_empty();
            let mut h = DefaultHasher::new();
            (mode as i32, selected, lead == Some(it.xf), self.wireframe, self.wire_on_shaded, comps, self.smooth_cage, entry.smooth).hash(&mut h);
            (soft_on, soft.1.to_bits(), Arc::as_ptr(&it.mesh) as usize).hash(&mut h);
            let sel_key = h.finish();
            if sel_key != entry.sel_key {
                entry.sel_key = sel_key;
                for b in [entry.lines.take(), entry.points.take(), entry.tint.take()].into_iter().flatten() {
                    gpu.drop_buffer(b.0);
                }
                // Smooth preview: the cage's wire on the 2 key and in component modes, the smooth mesh's otherwise.
                let cage = entry.display.is_none() || self.smooth_cage || hilited;
                let wire_mesh = if cage { it.mesh.clone() } else { entry.display.clone().unwrap() };
                let m = &wire_mesh;
                let edge_color = |e: Index| -> [u8; 4] {
                    if hilited {
                        if mode == ComponentMode::Edge && comp_set.contains(&e) {
                            SEL_COMP
                        } else {
                            UNSEL_EDGE
                        }
                    } else if selected {
                        if lead == Some(it.xf) {
                            LEAD
                        } else {
                            SELECTED
                        }
                    } else {
                        WIRE
                    }
                };
                let want_lines = selected || self.wireframe || self.wire_on_shaded || (entry.display.is_some() && self.smooth_cage);
                if want_lines {
                    let mut lv = Vec::new();
                    for e in 0..m.edge_capacity() {
                        if !m.edge_valid(e) {
                            continue;
                        }
                        let (a, b) = (m.edge_point(e, 0), m.edge_point(e, 1));
                        let c = edge_color(e);
                        lv.push(LineVertex { pos: m.position(a).as_vec3().to_array(), color: c8(c) });
                        lv.push(LineVertex { pos: m.position(b).as_vec3().to_array(), color: c8(c) });
                    }
                    if !lv.is_empty() {
                        entry.lines = Some((gpu.buffer_with(vk::BufferUsageFlags::VERTEX_BUFFER, &lv), lv.len() as u32));
                    }
                }
                if soft_on {
                    // Soft selection's falloff, Maya's colours: yellow at the selection through red to black.
                    let pts = components::convert(&it.mesh, mode, comps, ComponentMode::Vertex, false);
                    let w = aztek_scene::mesh_edit::transform::soft_weights(g, &it.mesh, &it.world, &pts);
                    let mut pv = Vec::new();
                    for p in 0..it.mesh.point_capacity() {
                        if !it.mesh.point_valid(p) {
                            continue;
                        }
                        let x = w.get(p as usize).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                        let c = if x >= 0.999 {
                            [255, 255, 0, 255]
                        } else if x <= 0.0 {
                            if mode == ComponentMode::Vertex { [10, 10, 10, 255] } else { continue }
                        } else if x > 0.5 {
                            [255, (255.0 * (x - 0.5) * 2.0) as u8, 0, 255]
                        } else {
                            [(80.0 + 175.0 * x * 2.0) as u8, 0, 0, 255]
                        };
                        pv.push(LineVertex { pos: it.mesh.position(p).as_vec3().to_array(), color: c8(c) });
                    }
                    if !pv.is_empty() {
                        entry.points = Some((gpu.buffer_with(vk::BufferUsageFlags::VERTEX_BUFFER, &pv), pv.len() as u32));
                    }
                } else if hilited && mode == ComponentMode::Vertex {
                    let mut pv = Vec::new();
                    for p in 0..m.point_capacity() {
                        if m.point_valid(p) {
                            let c = if comp_set.contains(&p) { SEL_COMP } else { VERT };
                            pv.push(LineVertex { pos: m.position(p).as_vec3().to_array(), color: c8(c) });
                        }
                    }
                    if !pv.is_empty() {
                        entry.points = Some((gpu.buffer_with(vk::BufferUsageFlags::VERTEX_BUFFER, &pv), pv.len() as u32));
                    }
                }
                if hilited && mode == ComponentMode::Face && !comps.is_empty() {
                    let mut ti = Vec::new();
                    for (t, &f) in entry.tri_faces.iter().enumerate() {
                        if comp_set.contains(&f) {
                            ti.extend_from_slice(&entry.tri_indices[t * 3..t * 3 + 3]);
                        }
                    }
                    if !ti.is_empty() {
                        entry.tint = Some((gpu.buffer_with(vk::BufferUsageFlags::INDEX_BUFFER, &ti), ti.len() as u32));
                    }
                }
            }
            let mvp = vp * it.world;
            // Paint skin weights: the painted influence's weight in grey, white for 1, as Maya's colour feedback.
            let mut painted = false;
            if self.tool == Tool::PaintSkin && selected && s.paint_ctx.color_feedback && !self.wireframe && entry.display.is_none() {
                if let Some(sk) = aztek_scene::skin::skin_of(g, it.shape) {
                    if let (Some(bind), Some(inf)) = (aztek_scene::skin::bind_of(g, sk), aztek_scene::skin::paint_influence(s, sk)) {
                        let w = aztek_scene::skin::point_weights(&bind, inf);
                        let mut h = DefaultHasher::new();
                        for x in &w {
                            x.to_bits().hash(&mut h);
                        }
                        (Arc::as_ptr(&it.mesh) as usize, inf).hash(&mut h);
                        let wk = h.finish();
                        if wk != entry.weight_key || entry.weight_tris.is_none() {
                            entry.weight_key = wk;
                            if let Some((a, b, _)) = entry.weight_tris.take() {
                                gpu.drop_buffer(a);
                                gpu.drop_buffer(b);
                            }
                            let verts: Vec<LineVertex> = entry
                                .vertex_point
                                .iter()
                                .map(|&pt| {
                                    let x = w.get(pt as usize).copied().unwrap_or(0.0).clamp(0.0, 1.0) as f32;
                                    LineVertex { pos: it.mesh.position(pt).as_vec3().to_array(), color: [x, x, x, 1.0] }
                                })
                                .collect();
                            if !verts.is_empty() && !entry.tri_indices.is_empty() {
                                entry.weight_tris = Some((
                                    gpu.buffer_with(vk::BufferUsageFlags::VERTEX_BUFFER, &verts),
                                    gpu.buffer_with(vk::BufferUsageFlags::INDEX_BUFFER, &entry.tri_indices),
                                    entry.tri_indices.len() as u32,
                                ));
                            }
                        }
                        if let Some((vb, ib, n)) = &entry.weight_tris {
                            r.bind(gpu, Kind::ColorTris);
                            r.push(gpu, &push(mvp, it.world, view, [1.0; 4], 0.0, 1.0));
                            r.draw(gpu, vb, Some((ib, *n)), 0);
                            painted = true;
                        }
                    }
                }
            }
            // Quad Draw's mesh sits on the live surface, chords and all, so it's pulled forward to stay in sight.
            let retopo = if self.tool == Tool::QuadDraw && !s.live_surface.contains(&it.shape) && !s.live_surface.is_empty() { 0.03 } else { 0.0 };
            if !self.wireframe && !painted {
                if let Some((vb, ib, n)) = &entry.tris {
                    r.bind(gpu, Kind::Shaded);
                    r.push(gpu, &push(mvp, it.world, view, [0.5, 0.5, 0.5, 1.0], retopo, 1.0));
                    r.draw(gpu, vb, Some((ib, *n)), 0);
                    if let Some((ti, tn)) = &entry.tint {
                        r.bind(gpu, Kind::Tint);
                        r.push(gpu, &push(mvp, it.world, view, [1.0, 0.45, 0.05, 0.45], 2e-4, 1.0));
                        r.draw(gpu, vb, Some((ti, *tn)), 0);
                    }
                }
            }
            if let Some((lb, ln)) = &entry.lines {
                r.bind(gpu, Kind::Lines);
                r.set_line_width(gpu, if selected && mode == ComponentMode::Object { 1.5 } else { 1.0 });
                r.push(gpu, &push(mvp, it.world, view, [1.0; 4], 1.5e-3 + retopo, 1.0));
                r.draw(gpu, lb, None, *ln);
            }
            if let Some((pb, pn)) = &entry.points {
                r.bind(gpu, Kind::Points);
                r.push(gpu, &push(mvp, it.world, view, [1.0; 4], 2e-3, (5.0 * scale) as f32));
                r.draw(gpu, pb, None, *pn);
            }
        }
        self.draw_rig(gpu, r, s, scale);
        self.draw_hover(gpu, r, items, scale);
        self.draw_manip(gpu, r, s, scale);
        self.draw_brush(gpu, r, s);
        self.draw_curve_tool(gpu, r, scale);
        self.draw_tool_overlay(gpu, r, scale);
    }

    fn draw_tool_overlay(&mut self, gpu: &mut Gpu, r: &Renderer, scale: f64) {
        if !self.tool_lines.is_empty() {
            let mut v = Vec::with_capacity(self.tool_lines.len() * 2);
            for (a, b, c) in &self.tool_lines {
                v.push(LineVertex { pos: a.as_vec3().to_array(), color: c8(*c) });
                v.push(LineVertex { pos: b.as_vec3().to_array(), color: c8(*c) });
            }
            let b = gpu.buffer_with(vk::BufferUsageFlags::VERTEX_BUFFER, &v);
            r.bind(gpu, Kind::LinesOnTop);
            r.set_line_width(gpu, (1.5 * scale) as f32);
            r.push(gpu, &push(self.view_proj(), DMat4::IDENTITY, self.cam.view(), [1.0; 4], 0.0, 1.0));
            r.draw(gpu, &b, None, v.len() as u32);
            gpu.drop_buffer(b);
        }
        if !self.tool_points.is_empty() {
            let v: Vec<LineVertex> = self.tool_points.iter().map(|(p, c)| LineVertex { pos: p.as_vec3().to_array(), color: c8(*c) }).collect();
            let b = gpu.buffer_with(vk::BufferUsageFlags::VERTEX_BUFFER, &v);
            r.bind(gpu, Kind::Points);
            r.push(gpu, &push(self.view_proj(), DMat4::IDENTITY, self.cam.view(), [1.0; 4], 0.05, (7.0 * scale) as f32));
            r.draw(gpu, &b, None, v.len() as u32);
            gpu.drop_buffer(b);
        }
    }

    /// Whether the tool has a manipulator.
    fn manipulates(&self) -> bool {
        matches!(self.tool, Tool::Move | Tool::Rotate | Tool::Scale)
    }

    /// A curve tool's points so far, with the curve they'd make.
    fn draw_curve_tool(&mut self, gpu: &mut Gpu, r: &Renderer, scale: f64) {
        if !matches!(self.tool, Tool::CurveCv | Tool::CurveEp) || self.curve_points.is_empty() {
            return;
        }
        let col = c8([255, 255, 255, 255]);
        let line = c8([120, 120, 255, 255]);
        let mut pv: Vec<LineVertex> = self.curve_points.iter().map(|p| LineVertex { pos: p.as_vec3().to_array(), color: col }).collect();
        let b = gpu.buffer_with(vk::BufferUsageFlags::VERTEX_BUFFER, &pv);
        r.bind(gpu, Kind::Points);
        r.push(gpu, &push(self.view_proj(), DMat4::IDENTITY, self.cam.view(), [1.0; 4], 1e-3, (6.0 * scale) as f32));
        r.draw(gpu, &b, None, pv.len() as u32);
        gpu.drop_buffer(b);
        pv.clear();
        if self.curve_points.len() >= 2 {
            let d = self.curve_degree.max(1) as usize;
            let curve = if self.tool == Tool::CurveEp { aztek_geo::nurbs::NurbsCurve::through(d, &self.curve_points) } else { aztek_geo::nurbs::NurbsCurve::open(d, self.curve_points.clone()) };
            let pts = curve.tessellate(12);
            for w in pts.windows(2) {
                pv.push(LineVertex { pos: w[0].as_vec3().to_array(), color: line });
                pv.push(LineVertex { pos: w[1].as_vec3().to_array(), color: line });
            }
            let b = gpu.buffer_with(vk::BufferUsageFlags::VERTEX_BUFFER, &pv);
            r.bind(gpu, Kind::Lines);
            r.set_line_width(gpu, (1.5 * scale) as f32);
            r.push(gpu, &push(self.view_proj(), DMat4::IDENTITY, self.cam.view(), [1.0; 4], 1e-3, 1.0));
            r.draw(gpu, &b, None, pv.len() as u32);
            gpu.drop_buffer(b);
        }
    }

    /// The brush's ring on the surface and its normal line, in red as Maya draws it.
    fn draw_brush(&mut self, gpu: &mut Gpu, r: &Renderer, s: &Session) {
        let Some((c, n)) = self.brush else { return };
        let radius = match self.tool {
            Tool::PaintSkin => s.paint_ctx.radius,
            Tool::Sculpt => s.sculpt_ctx.size,
            _ => return,
        };
        let (u, w) = n.any_orthonormal_pair();
        let mut v = Vec::new();
        let col = c8([230, 40, 40, 255]);
        let steps = 48;
        for i in 0..steps {
            let a = i as f64 / steps as f64 * std::f64::consts::TAU;
            let b = (i + 1) as f64 / steps as f64 * std::f64::consts::TAU;
            v.push(LineVertex { pos: (c + (u * a.cos() + w * a.sin()) * radius).as_vec3().to_array(), color: col });
            v.push(LineVertex { pos: (c + (u * b.cos() + w * b.sin()) * radius).as_vec3().to_array(), color: col });
        }
        v.push(LineVertex { pos: c.as_vec3().to_array(), color: col });
        v.push(LineVertex { pos: (c + n * radius * 0.5).as_vec3().to_array(), color: col });
        let b = gpu.buffer_with(vk::BufferUsageFlags::VERTEX_BUFFER, &v);
        r.bind(gpu, Kind::LinesOnTop);
        r.set_line_width(gpu, 1.5);
        r.push(gpu, &push(self.view_proj(), DMat4::IDENTITY, self.cam.view(), [1.0; 4], 0.0, 1.0));
        r.draw(gpu, &b, None, v.len() as u32);
        gpu.drop_buffer(b);
    }

    /// The selected skinned mesh under a pixel, for the brush (any selected mesh when sculpting): (item, world point, world
    /// normal).
    pub fn brush_hit(&mut self, s: &Session, items: &[DrawItem], px: DVec2) -> Option<(usize, DVec3, DVec3)> {
        let g = &s.graph;
        let sel = g.selection();
        let any = self.tool == Tool::Sculpt;
        let picked: Vec<usize> = (0..items.len()).filter(|&i| (sel.contains(&items[i].xf) || sel.contains(&items[i].shape)) && (any || aztek_scene::skin::skin_of(g, items[i].shape).is_some())).collect();
        let list: Vec<DrawItem> = picked
            .iter()
            .map(|&i| DrawItem { shape: items[i].shape, xf: items[i].xf, world: items[i].world, mesh: items[i].mesh.clone(), smooth: 0 })
            .collect();
        let (k, f, wp) = self.raycast(&list, px)?;
        let n = list[k].world.transform_vector3(list[k].mesh.face_normal(f)).normalize_or_zero();
        Some((picked[k], wp, n))
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Picking

    fn bvh_for(&mut self, it: &DrawItem) -> Option<&MeshBvh> {
        let e = self.meshes.get_mut(&it.shape)?;
        if e.bvh.is_none() {
            e.bvh = Some(MeshBvh::new(&it.mesh));
        }
        e.bvh.as_ref()
    }

    /// The nearest mesh hit under a viewport pixel: (item index, face, world point).
    pub fn raycast(&mut self, items: &[DrawItem], px: DVec2) -> Option<(usize, Index, DVec3)> {
        let (o, d) = self.cam.ray(px, self.size());
        let mut best: Option<(usize, Index, DVec3, f64)> = None;
        for (i, it) in items.iter().enumerate() {
            let inv = it.world.inverse();
            let lo = inv.transform_point3(o);
            let ld = inv.transform_vector3(d);
            let len = ld.length();
            if len < 1e-12 {
                continue;
            }
            let Some(bvh) = self.bvh_for(it) else { continue };
            if let Some(hit) = bvh.raycast_with(lo, ld / len, f64::MAX, true) {
                let wp = it.world.transform_point3(lo + ld / len * hit.t);
                let dist = (wp - o).length();
                if best.as_ref().is_none_or(|b| dist < b.3) {
                    best = Some((i, hit.face, wp, dist));
                }
            }
        }
        best.map(|b| (b.0, b.1, b.2))
    }

    /// What a click at px picks, as select arguments: object names or component strings.
    /// The component under a pixel on the hilited meshes: (item, kind, id).
    pub fn pick_component(&mut self, s: &Session, items: &[DrawItem], px: DVec2) -> Option<(usize, ComponentMode, Index)> {
        let g = &s.graph;
        let mode = components::mode(g);
        if mode == ComponentMode::Object {
            return None;
        }
        let sel = g.selection();
        let hil: Vec<usize> = (0..items.len()).filter(|&i| sel.contains(&items[i].xf)).collect();
        if mode == ComponentMode::Face {
            let hit_items: Vec<DrawItem> = hil
                .iter()
                .map(|&i| DrawItem { shape: items[i].shape, xf: items[i].xf, world: items[i].world, mesh: items[i].mesh.clone(), smooth: 0 })
                .collect();
            return self.raycast(&hit_items, px).map(|(i, f, _)| (hil[i], mode, f));
        }
        let screen = self.screen();
        let hil_items: Vec<DrawItem> = hil
            .iter()
            .map(|&i| DrawItem { shape: items[i].shape, xf: items[i].xf, world: items[i].world, mesh: items[i].mesh.clone(), smooth: 0 })
            .collect();
        // Over a face: the nearest of its own vertices or edges, however far, as Maya pre-selects.
        if let Some((k, f, _)) = self.raycast(&hil_items, px) {
            let it = &hil_items[k];
            let m = &it.mesh;
            let proj = |p: Index| screen.project(it.world.transform_point3(m.position(p)));
            let mut best: Option<(f64, Index)> = None;
            for c in m.face_corners(f) {
                let (d, id) = if mode == ComponentMode::Vertex {
                    let p = m.corner_point(c);
                    let Some(q) = proj(p) else { continue };
                    ((q - px).length(), p)
                } else {
                    let e = m.corner_edge(c);
                    let (Some(a), Some(b)) = (proj(m.edge_point(e, 0)), proj(m.edge_point(e, 1))) else { continue };
                    (seg_distance(px, a, b), e)
                };
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, id));
                }
            }
            if let Some((_, id)) = best {
                return Some((hil[k], mode, id));
            }
        }
        // Off the surface: the nearest within reach that no face hides.
        let mut best: Option<(f64, usize, Index)> = None;
        for (k, &i) in hil.iter().enumerate() {
            let it = &items[i];
            for c in component_pick::nearby(&it.mesh, &it.world, mode, &screen, px, component_pick::radius(mode)) {
                let local = if mode == ComponentMode::Vertex {
                    it.mesh.position(c.id)
                } else {
                    (it.mesh.position(it.mesh.edge_point(c.id, 0)) + it.mesh.position(it.mesh.edge_point(c.id, 1))) * 0.5
                };
                let wp = it.world.transform_point3(local);
                if !self.visible(&hil_items[k..=k], wp, &screen) {
                    continue;
                }
                if best.is_none_or(|b| c.px < b.0) {
                    best = Some((c.px, i, c.id));
                }
                break;
            }
        }
        best.map(|b| (b.1, mode, b.2))
    }

    /// Whether a world point is in front of every surface of the items along its line of sight.
    fn visible(&mut self, items: &[DrawItem], wp: DVec3, screen: &Screen) -> bool {
        let Some(q) = screen.project(wp) else { return false };
        let (o, _) = self.cam.ray(q, self.size());
        let dist = (wp - o).length();
        match self.raycast(items, q) {
            Some((_, _, hit)) => (hit - o).length() >= dist - dist.max(1.0) * 1e-3,
            None => true,
        }
    }

    /// Where a click places a joint: on the view plane through the origin in the orthographic views, on the ground
    /// in perspective (or facing the camera at the centre of interest when looking along the ground).
    pub fn place_point(&self, px: DVec2) -> DVec3 {
        let (o, d) = self.cam.ray(px, self.size());
        let (n, p0) = match self.cam.kind {
            CamKind::Persp => {
                if d.y.abs() > 0.05 {
                    (DVec3::Y, DVec3::ZERO)
                } else {
                    ((self.cam.eye() - self.cam.coi).normalize(), self.cam.coi)
                }
            }
            _ => ((self.cam.eye() - self.cam.coi).normalize(), DVec3::ZERO),
        };
        let den = n.dot(d);
        if den.abs() < 1e-9 {
            return self.cam.coi;
        }
        let t = n.dot(p0 - o) / den;
        let mut p = o + d * t;
        if self.snap == 1 {
            // X held: to the grid.
            p = (p / self.grid_spacing).round() * self.grid_spacing;
        }
        // Snap tiny noise so ortho placements sit exactly in their plane.
        for i in 0..3 {
            if p[i].abs() < 1e-9 {
                p[i] = 0.0;
            }
        }
        p
    }

    /// The joint, locator or IK handle drawn nearest px (within a few pixels), as its transform.
    pub fn pick_rig(&self, g: &Graph, px: DVec2) -> Option<NodeId> {
        let screen = self.screen();
        let mut best: Option<(f64, NodeId)> = None;
        for it in rig_items(g) {
            for (a, b) in &it.segs {
                let (Some(pa), Some(pb)) = (screen.project(*a), screen.project(*b)) else { continue };
                let d = seg_distance(px, pa, pb);
                if d < 6.0 && best.is_none_or(|x| d < x.0) {
                    best = Some((d, it.node));
                }
            }
        }
        best.map(|b| b.1)
    }

    fn draw_rig(&mut self, gpu: &mut Gpu, r: &Renderer, s: &Session, scale: f64) {
        let g = &s.graph;
        let sel = g.selection();
        let lead = sel.last().copied();
        let mut v = Vec::new();
        for it in rig_items(g) {
            let c = if Some(it.node) == lead {
                [100, 255, 160, 255]
            } else if sel.contains(&it.node) {
                [255, 255, 255, 255]
            } else {
                it.color
            };
            for (a, b) in &it.segs {
                v.push(LineVertex { pos: a.as_vec3().to_array(), color: c8(c) });
                v.push(LineVertex { pos: b.as_vec3().to_array(), color: c8(c) });
            }
        }
        if !v.is_empty() {
            let b = gpu.buffer_with(vk::BufferUsageFlags::VERTEX_BUFFER, &v);
            r.bind(gpu, Kind::Lines);
            r.set_line_width(gpu, (1.5 * scale) as f32);
            r.push(gpu, &push(self.view_proj(), DMat4::IDENTITY, self.cam.view(), [1.0; 4], 1e-3, 1.0));
            r.draw(gpu, &b, None, v.len() as u32);
            gpu.drop_buffer(b);
        }
        // CVs of the selected curves in a component mode, and of curves with dispCV on, with the hull.
        let mode = components::mode(g);
        let mut points = Vec::new();
        let mut hull = Vec::new();
        for (shape, xf, world_cvs) in curve_cvs(g) {
            let hilited = mode == ComponentMode::Vertex && (sel.contains(&xf) || sel.contains(&shape));
            let n = g.node(shape);
            let disp = n.is_some_and(|n| n.data().bool(aztek_scene::curves::CurveShape::DISP_CV));
            let disp_hull = n.is_some_and(|n| n.data().bool(aztek_scene::curves::CurveShape::DISP_HULL));
            if !hilited && !disp {
                continue;
            }
            let picked = components::selected(g, shape, ComponentMode::Vertex);
            for (i, p) in world_cvs.iter().enumerate() {
                let c = if picked.contains(&(i as u32)) { SEL_COMP } else { VERT };
                points.push(LineVertex { pos: p.as_vec3().to_array(), color: c8(c) });
            }
            if disp_hull || hilited {
                for w in world_cvs.windows(2) {
                    hull.push(LineVertex { pos: w[0].as_vec3().to_array(), color: c8([120, 60, 120, 255]) });
                    hull.push(LineVertex { pos: w[1].as_vec3().to_array(), color: c8([120, 60, 120, 255]) });
                }
            }
        }
        if !hull.is_empty() {
            let b = gpu.buffer_with(vk::BufferUsageFlags::VERTEX_BUFFER, &hull);
            r.bind(gpu, Kind::Lines);
            r.set_line_width(gpu, 1.0);
            r.push(gpu, &push(self.view_proj(), DMat4::IDENTITY, self.cam.view(), [1.0; 4], 1e-3, 1.0));
            r.draw(gpu, &b, None, hull.len() as u32);
            gpu.drop_buffer(b);
        }
        if !points.is_empty() {
            let b = gpu.buffer_with(vk::BufferUsageFlags::VERTEX_BUFFER, &points);
            r.bind(gpu, Kind::Points);
            r.push(gpu, &push(self.view_proj(), DMat4::IDENTITY, self.cam.view(), [1.0; 4], 2e-3, (6.0 * scale) as f32));
            r.draw(gpu, &b, None, points.len() as u32);
            gpu.drop_buffer(b);
        }
    }

    /// The CV of a hilited curve nearest a pixel.
    fn pick_cv(&self, g: &Graph, px: DVec2) -> Option<(NodeId, usize)> {
        let screen = self.screen();
        let sel = g.selection();
        let mut best: Option<(f64, NodeId, usize)> = None;
        for (shape, xf, cvs) in curve_cvs(g) {
            if !sel.contains(&xf) && !sel.contains(&shape) {
                continue;
            }
            for (i, p) in cvs.iter().enumerate() {
                let Some(q) = screen.project(*p) else { continue };
                let d = (q - px).length();
                if d < 14.0 && best.is_none_or(|b| d < b.0) {
                    best = Some((d, shape, i));
                }
            }
        }
        best.map(|b| (b.1, b.2))
    }

    /// What a click at px picks, as select arguments: object names or component strings.
    pub fn pick(&mut self, s: &Session, items: &[DrawItem], px: DVec2) -> Vec<String> {
        let g = &s.graph;
        if components::mode(g) == ComponentMode::Object {
            if let Some(id) = self.pick_rig(g, px) {
                return vec![g.path(id)];
            }
            return self.raycast(items, px).map(|(i, _, _)| vec![g.path(items[i].xf)]).unwrap_or_default();
        }
        if components::mode(g) == ComponentMode::Vertex {
            if let Some((shape, i)) = self.pick_cv(g, px) {
                return vec![format!("{}.cv[{i}]", g.path(aztek_scene::dag::transform_of(g, shape)))];
            }
            if let Some(hit) = self.pick_lattice_point(g, px) {
                return vec![hit];
            }
        }
        match self.pick_component(s, items, px) {
            Some((i, kind, id)) => {
                let p = match kind {
                    ComponentMode::Vertex => "vtx",
                    ComponentMode::Edge => "e",
                    _ => "f",
                };
                vec![format!("{}.{p}[{id}]", g.path(items[i].xf))]
            }
            None => Vec::new(),
        }
    }

    /// The control point of a selected lattice nearest px, within a few pixels.
    fn pick_lattice_point(&self, g: &Graph, px: DVec2) -> Option<String> {
        let screen = self.screen();
        let mut best: Option<(f64, String)> = None;
        for (xf, shape, dims, pts) in selected_lattices(g) {
            for (i, p) in pts.iter().enumerate() {
                let Some(q) = screen.project(*p) else { continue };
                let d = (q - px).length();
                if d < 10.0 && best.as_ref().is_none_or(|b| d < b.0) {
                    let name = components::grid_strings(&g.path(xf), dims, &[i as u32], true);
                    best = name.into_iter().next().map(|n| (d, n));
                }
            }
            let _ = shape;
        }
        best.map(|b| b.1)
    }

    /// Draws the pre-selection highlight: the component under the cursor in red.
    fn draw_hover(&mut self, gpu: &mut Gpu, r: &Renderer, items: &[DrawItem], scale: f64) {
        let Some((shape, kind, id)) = self.hover_comp else { return };
        let Some(it) = items.iter().find(|i| i.shape == shape) else { return };
        let m = &it.mesh;
        let c = c8([255, 60, 60, 255]);
        let pos = |p: Index| m.position(p).as_vec3().to_array();
        let mut v = Vec::new();
        match kind {
            ComponentMode::Vertex if m.point_valid(id) => v.push(LineVertex { pos: pos(id), color: c }),
            ComponentMode::Edge if m.edge_valid(id) => {
                v.push(LineVertex { pos: pos(m.edge_point(id, 0)), color: c });
                v.push(LineVertex { pos: pos(m.edge_point(id, 1)), color: c });
            }
            ComponentMode::Face if m.face_valid(id) => {
                let pts = m.face_points(id);
                for k in 0..pts.len() {
                    v.push(LineVertex { pos: pos(pts[k]), color: c });
                    v.push(LineVertex { pos: pos(pts[(k + 1) % pts.len()]), color: c });
                }
            }
            _ => return,
        }
        let b = gpu.buffer_with(vk::BufferUsageFlags::VERTEX_BUFFER, &v);
        let mvp = self.view_proj() * it.world;
        if kind == ComponentMode::Vertex {
            r.bind(gpu, Kind::Points);
            r.push(gpu, &push(mvp, it.world, self.cam.view(), [1.0; 4], 2.5e-3, (7.0 * scale) as f32));
        } else {
            r.bind(gpu, Kind::Lines);
            r.set_line_width(gpu, (2.0 * scale) as f32);
            r.push(gpu, &push(mvp, it.world, self.cam.view(), [1.0; 4], 2.5e-3, 1.0));
        }
        r.draw(gpu, &b, None, v.len() as u32);
        gpu.drop_buffer(b);
    }

    /// Everything inside a viewport-pixel rectangle.
    pub fn pick_rect(&self, s: &Session, items: &[DrawItem], a: DVec2, b: DVec2) -> Vec<String> {
        let (lo, hi) = (a.min(b), a.max(b));
        let inside = |p: DVec2| p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y;
        let g = &s.graph;
        let mode = components::mode(g);
        let screen = self.screen();
        let mut out = Vec::new();
        if mode == ComponentMode::Vertex {
            for (xf, _, dims, pts) in selected_lattices(g) {
                let ids: Vec<u32> = (0..pts.len() as u32).filter(|&i| screen.project(pts[i as usize]).is_some_and(inside)).collect();
                if !ids.is_empty() {
                    out.extend(components::grid_strings(&g.path(xf), dims, &ids, false));
                }
            }
        }
        if mode == ComponentMode::Object {
            for it in rig_items(g) {
                if it.segs.iter().any(|(a, b)| screen.project(*a).is_some_and(inside) || screen.project(*b).is_some_and(inside)) {
                    out.push(g.path(it.node));
                }
            }
        }
        if mode == ComponentMode::Vertex {
            for (shape, xf, cvs) in curve_cvs(g) {
                if !g.selection().contains(&xf) && !g.selection().contains(&shape) {
                    continue;
                }
                for (i, p) in cvs.iter().enumerate() {
                    if screen.project(*p).is_some_and(inside) {
                        out.push(format!("{}.cv[{i}]", g.path(xf)));
                    }
                }
            }
        }
        for it in items {
            let m = &it.mesh;
            let proj = |p: Index| screen.project(it.world.transform_point3(m.position(p)));
            match mode {
                ComponentMode::Object => {
                    if (0..m.point_capacity()).any(|p| m.point_valid(p) && proj(p).is_some_and(inside)) {
                        out.push(g.path(it.xf));
                    }
                }
                _ if !g.selection().contains(&it.xf) => {}
                ComponentMode::Vertex => {
                    let ids: Vec<Index> = (0..m.point_capacity()).filter(|&p| m.point_valid(p) && proj(p).is_some_and(inside)).collect();
                    push_ranges(&mut out, &g.path(it.xf), "vtx", &ids);
                }
                ComponentMode::Edge => {
                    let ids: Vec<Index> = (0..m.edge_capacity())
                        .filter(|&e| {
                            m.edge_valid(e) && {
                                let (a, b) = (m.edge_point(e, 0), m.edge_point(e, 1));
                                screen.project(it.world.transform_point3((m.position(a) + m.position(b)) * 0.5)).is_some_and(inside)
                            }
                        })
                        .collect();
                    push_ranges(&mut out, &g.path(it.xf), "e", &ids);
                }
                ComponentMode::Face => {
                    let ids: Vec<Index> = (0..m.face_capacity())
                        .filter(|&f| {
                            m.face_valid(f) && {
                                let pts = m.face_points(f);
                                let c = pts.iter().map(|&p| m.position(p)).sum::<DVec3>() / pts.len().max(1) as f64;
                                screen.project(it.world.transform_point3(c)).is_some_and(inside)
                            }
                        })
                        .collect();
                    push_ranges(&mut out, &g.path(it.xf), "f", &ids);
                }
            }
        }
        out
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Manipulators

    /// Where the manipulator sits: the lead object's pivot, or the selected components' centre.
    pub fn manip_pivot(&self, s: &mut Session, items: &[DrawItem]) -> Option<DVec3> {
        if !self.manipulates() {
            return None;
        }
        let mode = components::mode(&s.graph);
        if mode == ComponentMode::Object {
            let lead = *s.graph.selection().last()?;
            return Some(aztek_scene::transform_ops::world_pivot(&mut s.graph, lead));
        }
        let g = &s.graph;
        let (mut sum, mut n) = (DVec3::ZERO, 0.0);
        for it in items {
            if !g.selection().contains(&it.xf) {
                continue;
            }
            let ids = components::selected(g, it.shape, mode);
            if ids.is_empty() {
                continue;
            }
            let pts = components::convert(&it.mesh, mode, ids, ComponentMode::Vertex, false);
            for p in pts {
                sum += it.world.transform_point3(it.mesh.position(p));
                n += 1.0;
            }
        }
        if mode == ComponentMode::Vertex {
            for (shape, _, cvs) in curve_cvs(g) {
                for &i in components::selected(g, shape, mode) {
                    if let Some(p) = cvs.get(i as usize) {
                        sum += *p;
                        n += 1.0;
                    }
                }
            }
        }
        (n > 0.0).then(|| sum / n)
    }

    /// Works out the manipulator's axes for the axis orientation in Tool Settings. The move tool honours every mode; rotate
    /// and scale read them too.
    pub fn update_frame(&mut self, s: &mut Session) {
        let g = &mut s.graph;
        self.frame = DMat3::IDENTITY;
        if self.axis_mode == "world" {
            return;
        }
        let Some(&lead) = g.selection().last() else { return };
        let ortho = |m: &DMat4| {
            let x = m.x_axis.truncate().normalize_or_zero();
            let y = m.y_axis.truncate().normalize_or_zero();
            let z = x.cross(y);
            if x == DVec3::ZERO || y == DVec3::ZERO {
                DMat3::IDENTITY
            } else {
                DMat3::from_cols(x, y, z)
            }
        };
        g.evaluate();
        let mode = components::mode(g);
        match self.axis_mode.as_str() {
            "object" => self.frame = ortho(&aztek_scene::rig::world(g, lead)),
            "parent" => {
                let p = g.parent(lead);
                if p != NO_NODE {
                    self.frame = ortho(&aztek_scene::rig::world(g, p));
                }
            }
            "component" | "normal" => {
                if mode == ComponentMode::Object {
                    self.frame = ortho(&aztek_scene::rig::world(g, lead));
                } else {
                    self.frame = aztek_scene::mesh_edit::transform::selection_normal_frame(g);
                }
            }
            _ => {}
        }
    }

    /// World units per pixel at a point.
    fn units_per_px(&self, at: DVec3) -> f64 {
        self.cam.units_per_px_at(at, self.rect[3], self.aspect())
    }

    pub fn handle_len(&self, pivot: DVec3, scale: f64) -> f64 {
        self.units_per_px(pivot) * 100.0 * scale * self.manip_scale
    }

    fn draw_manip(&mut self, gpu: &mut Gpu, r: &Renderer, s: &mut Session, scale: f64) {
        let items = gather(&mut s.graph);
        let Some(pivot) = self.manip_pivot(s, &items) else { return };
        let len = self.handle_len(pivot, scale) as f32;
        let p = pivot.as_vec3();
        let colors = [[230, 40, 40, 255], [40, 230, 40, 255], [40, 80, 255, 255]];
        let active = self.drag.as_ref().map(|d| d.handle).or(self.hover);
        let mut v = Vec::new();
        self.update_frame(s);
        let axes = [self.frame.x_axis.as_vec3(), self.frame.y_axis.as_vec3(), self.frame.z_axis.as_vec3()];
        match self.eff_tool() {
            Tool::Rotate => {
                for (i, ax) in axes.iter().enumerate() {
                    let c = if active == Some(Handle::Axis(i)) { [255, 255, 0, 255] } else { colors[i] };
                    let (u, w) = ax.any_orthonormal_pair();
                    let n = 64;
                    for k in 0..n {
                        let a0 = k as f32 / n as f32 * std::f32::consts::TAU;
                        let a1 = (k + 1) as f32 / n as f32 * std::f32::consts::TAU;
                        v.push(LineVertex { pos: (p + (u * a0.cos() + w * a0.sin()) * len).to_array(), color: c8(c) });
                        v.push(LineVertex { pos: (p + (u * a1.cos() + w * a1.sin()) * len).to_array(), color: c8(c) });
                    }
                }
            }
            _ => {
                for (i, ax) in axes.iter().enumerate() {
                    let c = if active == Some(Handle::Axis(i)) { [255, 255, 0, 255] } else { colors[i] };
                    let tip = p + *ax * len;
                    v.push(LineVertex { pos: p.to_array(), color: c8(c) });
                    v.push(LineVertex { pos: tip.to_array(), color: c8(c) });
                    // A small cross at the tip: arrow for move, box for scale.
                    let (u, w) = ax.any_orthonormal_pair();
                    let k = len * 0.08;
                    let base = if self.eff_tool() == Tool::Move { tip - *ax * len * 0.2 } else { tip };
                    for d in [u, -u, w, -w] {
                        v.push(LineVertex { pos: tip.to_array(), color: c8(c) });
                        v.push(LineVertex { pos: (base + d * k).to_array(), color: c8(c) });
                    }
                }
                let c = if active == Some(Handle::Center) { [255, 255, 0, 255] } else { [230, 230, 120, 255] };
                let view = self.cam.view();
                let right = Vec3::new(view.x_axis.x as f32, view.y_axis.x as f32, view.z_axis.x as f32);
                let up = Vec3::new(view.x_axis.y as f32, view.y_axis.y as f32, view.z_axis.y as f32);
                let k = len * 0.1;
                let corners = [p + (right + up) * k, p + (up - right) * k, p - (right + up) * k, p + (right - up) * k];
                for i in 0..4 {
                    v.push(LineVertex { pos: corners[i].to_array(), color: c8(c) });
                    v.push(LineVertex { pos: corners[(i + 1) % 4].to_array(), color: c8(c) });
                }
            }
        }
        let b = gpu.buffer_with(vk::BufferUsageFlags::VERTEX_BUFFER, &v);
        r.bind(gpu, Kind::LinesOnTop);
        r.set_line_width(gpu, 2.0);
        r.push(gpu, &push(self.view_proj(), DMat4::IDENTITY, self.cam.view(), [1.0; 4], 0.0, 1.0));
        r.draw(gpu, &b, None, v.len() as u32);
        gpu.drop_buffer(b);
    }

    /// The manipulator handle under a pixel.
    pub fn manip_hit(&self, pivot: DVec3, px: DVec2, scale: f64) -> Option<Handle> {
        let screen = self.screen();
        let len = self.handle_len(pivot, scale);
        let c = screen.project(pivot)?;
        if self.eff_tool() != Tool::Rotate && (px - c).abs().max_element() < 10.0 * scale {
            return Some(Handle::Center);
        }
        let mut best: Option<(f64, Handle)> = None;
        for i in 0..3 {
            let ax = self.frame.col(i);
            let d = if self.eff_tool() == Tool::Rotate {
                let (u, w) = ax.any_orthonormal_pair();
                (0..48)
                    .filter_map(|k| {
                        let a = k as f64 / 48.0 * std::f64::consts::TAU;
                        let b = (k + 1) as f64 / 48.0 * std::f64::consts::TAU;
                        let (pa, pb) = screen.project_segment(pivot + (u * a.cos() + w * a.sin()) * len, pivot + (u * b.cos() + w * b.sin()) * len)?;
                        Some(seg_dist(px, pa, pb))
                    })
                    .fold(f64::MAX, f64::min)
            } else {
                let Some((pa, pb)) = screen.project_segment(pivot, pivot + ax * len) else { continue };
                seg_dist(px, pa, pb)
            };
            if d < 8.0 * scale && best.is_none_or(|b| d < b.0) {
                best = Some((d, Handle::Axis(i)));
            }
        }
        best.map(|b| b.1)
    }

    /// A move's offset with X (grid) or V (point) snapping held: the pivot lands on the grid, or on the vertex nearest the
    /// cursor; along an axis handle only that axis moves.
    fn snapped(&self, d: &Drag, world: DVec3, axis: Option<DVec3>, px: DVec2) -> DVec3 {
        let target = match self.snap {
            1 => {
                let s = self.grid_spacing;
                let t = d.pivot + world;
                (t / s).round() * s
            }
            2 => {
                let screen = self.screen();
                let mut best: Option<(f64, DVec3)> = None;
                for &p in &d.snap_points {
                    if let Some(q) = screen.project(p) {
                        let dist = (q - px).length();
                        if dist < 25.0 && best.is_none_or(|b| dist < b.0) {
                            best = Some((dist, p));
                        }
                    }
                }
                match best {
                    Some((_, p)) => p,
                    None => return world,
                }
            }
            _ => return world,
        };
        let off = target - d.pivot;
        match axis {
            Some(a) => a * off.dot(a),
            None => off,
        }
    }

    /// The tool the manipulator acts as: pivot mode moves pivots whatever the tool.
    pub fn eff_tool(&self) -> Tool {
        if self.pivot_mode && self.manipulates() {
            Tool::Move
        } else {
            self.tool
        }
    }

    /// The command for a drag from d.start to px: (command, call).
    pub fn drag_call(&self, d: &Drag, px: DVec2, scale: f64) -> Option<(String, serde_json::Value)> {
        let screen = self.screen();
        let c = screen.project(d.pivot)?;
        let delta = px - d.start;
        let upp = self.units_per_px(d.pivot);
        let comps = d.components;
        let targets = serde_json::json!(d.targets);
        match self.eff_tool() {
            Tool::Move => {
                let world = match d.handle {
                    Handle::Axis(i) => {
                        let ax = self.frame.col(i);
                        let tip = screen.project(d.pivot + ax * upp * 100.0)?;
                        let sdir = tip - c;
                        let l2 = sdir.length_squared();
                        if l2 < 1e-6 {
                            return None;
                        }
                        ax * (delta.dot(sdir) / l2) * upp * 100.0
                    }
                    Handle::Center => {
                        let view = self.cam.view();
                        let right = DVec3::new(view.x_axis.x, view.y_axis.x, view.z_axis.x);
                        let up = DVec3::new(view.x_axis.y, view.y_axis.y, view.z_axis.y);
                        (right * delta.x - up * delta.y) * upp
                    }
                };
                let axis = match d.handle {
                    Handle::Axis(i) => Some(self.frame.col(i)),
                    Handle::Center => None,
                };
                let world = self.snapped(d, world, axis, px);
                let by = serde_json::json!(world.to_array());
                Some(if comps {
                    ("moveComponents".into(), serde_json::json!({ "by": by }))
                } else if self.pivot_mode {
                    let pivots: Vec<String> = d.targets.iter().flat_map(|t| [format!("{t}.scalePivot"), format!("{t}.rotatePivot")]).collect();
                    ("move".into(), serde_json::json!({ "args": pivots, "by": by }))
                } else {
                    ("move".into(), serde_json::json!({ "args": targets, "by": by }))
                })
            }
            Tool::Rotate => {
                let Handle::Axis(i) = d.handle else { return None };
                // Angle swept about the pivot on screen.
                let a0 = (d.start - c).to_angle();
                let a1 = (px - c).to_angle();
                let mut ang = (a1 - a0).to_degrees();
                // Screen y runs down: flip so the ring turns the way the cursor does when the axis faces the camera.
                let toward = (self.cam.eye() - d.pivot).dot(self.frame.col(i)).signum();
                ang *= -toward;
                let mut by = [0.0; 3];
                by[i] = ang;
                // About a turned axis: the same rotation as world Euler angles.
                if self.frame != DMat3::IDENTITY {
                    let q = glam::DQuat::from_axis_angle(self.frame.col(i), ang.to_radians());
                    let e = aztek_scene::transform::matrix_to_euler(&DMat3::from_quat(q), aztek_scene::transform::RotateOrder::Xyz);
                    by = [e.x, e.y, e.z];
                }
                Some(if comps {
                    ("rotateComponents".into(), serde_json::json!({ "by": by, "pivot": d.pivot.to_array() }))
                } else {
                    ("rotate".into(), serde_json::json!({ "args": targets, "by": by }))
                })
            }
            Tool::Scale => {
                let f = (1.0 + (delta.x - delta.y) / (100.0 * scale)).max(0.001);
                let by = match d.handle {
                    Handle::Axis(i) => {
                        let mut b = [1.0; 3];
                        b[i] = f;
                        b
                    }
                    Handle::Center => [f; 3],
                };
                Some(if comps {
                    ("scaleComponents".into(), serde_json::json!({ "by": by, "pivot": d.pivot.to_array() }))
                } else {
                    ("scale".into(), serde_json::json!({ "args": targets, "by": by }))
                })
            }
            _ => None,
        }
    }

    /// Frames the selection (or everything when nothing is selected or all is true).
    pub fn frame(&mut self, s: &mut Session, items: &[DrawItem], all: bool) {
        let g = &s.graph;
        let mode = components::mode(g);
        let mut lo = DVec3::splat(f64::MAX);
        let mut hi = DVec3::splat(f64::MIN);
        let sel = g.selection().to_vec();
        let mut add = |p: DVec3| {
            lo = lo.min(p);
            hi = hi.max(p);
        };
        let mut any = false;
        for it in items {
            let picked = sel.contains(&it.xf);
            if !all && !sel.is_empty() && !picked {
                continue;
            }
            let ids: Vec<Index> = if !all && picked && mode != ComponentMode::Object {
                let c = components::selected(g, it.shape, mode);
                if c.is_empty() {
                    (0..it.mesh.point_capacity()).filter(|&p| it.mesh.point_valid(p)).collect()
                } else {
                    components::convert(&it.mesh, mode, c, ComponentMode::Vertex, false)
                }
            } else {
                (0..it.mesh.point_capacity()).filter(|&p| it.mesh.point_valid(p)).collect()
            };
            for p in ids {
                add(it.world.transform_point3(it.mesh.position(p)));
                any = true;
            }
        }
        if mode == ComponentMode::Object || all {
            for it in rig_items(g) {
                if !all && !sel.is_empty() && !sel.contains(&it.node) {
                    continue;
                }
                for (a, b) in &it.segs {
                    add(*a);
                    add(*b);
                    any = true;
                }
            }
        }
        if !any {
            if all || sel.is_empty() {
                let aspect = self.aspect();
                self.cam.frame(DVec3::ZERO, self.grid_spacing * 12.0, aspect);
            }
            return;
        }
        let aspect = self.aspect();
        self.cam.frame((lo + hi) * 0.5, ((hi - lo).length() * 0.5).max(1e-3), aspect);
    }

    /// Rebuilds the grid (after a camera change moves its plane).
    pub fn reset_grid(&mut self, gpu: &mut Gpu) {
        if let Some((b, _)) = self.grid.take() {
            gpu.drop_buffer(b);
        }
    }

    pub fn release(&mut self, gpu: &mut Gpu) {
        for (_, m) in self.meshes.drain() {
            drop_mesh(gpu, m);
        }
        if let Some((b, _)) = self.grid.take() {
            gpu.drop_buffer(b);
        }
    }
}

fn drop_mesh(gpu: &mut Gpu, m: GpuMesh) {
    if let Some((a, b, _)) = m.tris {
        gpu.drop_buffer(a);
        gpu.drop_buffer(b);
    }
    for b in [m.lines, m.points, m.tint].into_iter().flatten() {
        gpu.drop_buffer(b.0);
    }
    if let Some((a, b, _)) = m.weight_tris {
        gpu.drop_buffer(a);
        gpu.drop_buffer(b);
    }
}

fn push_ranges(out: &mut Vec<String>, path: &str, p: &str, ids: &[Index]) {
    if ids.is_empty() {
        return;
    }
    let kind = match p {
        "vtx" => ComponentMode::Vertex,
        "e" => ComponentMode::Edge,
        _ => ComponentMode::Face,
    };
    for c in aztek_scene::history::format_components(kind, ids).split_whitespace() {
        out.push(format!("{path}.{c}"));
    }
}

fn seg_dist(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let d = b - a;
    let l2 = d.dot(d);
    let t = if l2 > 0.0 { ((p - a).dot(d) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (a + d * t - p).length()
}

fn push(mvp: DMat4, model: DMat4, view: DMat4, color: [f32; 4], bias: f32, point: f32) -> Push {
    let nm = (view * model).inverse().transpose();
    let m = nm.as_mat4();
    // Rows of the upper 3x3.
    let row = |i: usize| [m.x_axis[i], m.y_axis[i], m.z_axis[i]];
    let (r0, r1, r2) = (row(0), row(1), row(2));
    Push {
        mvp: mvp.as_mat4().to_cols_array(),
        n0: [r0[0], r0[1], r0[2], bias],
        n1: [r1[0], r1[1], r1[2], point],
        n2: [r2[0], r2[1], r2[2], 0.0],
        color,
    }
}

fn seg_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let t = if ab.length_squared() < 1e-12 { 0.0 } else { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) };
    (p - (a + ab * t)).length()
}

/// Something drawn as lines in world space: joints, locators, IK handles.
pub struct RigItem {
    pub node: NodeId,
    pub color: [u8; 4],
    pub segs: Vec<(DVec3, DVec3)>,
}

fn circle(c: DVec3, u: DVec3, v: DVec3, r: f64, out: &mut Vec<(DVec3, DVec3)>) {
    let n = 20;
    for i in 0..n {
        let a = i as f64 / n as f64 * std::f64::consts::TAU;
        let b = (i + 1) as f64 / n as f64 * std::f64::consts::TAU;
        out.push((c + (u * a.cos() + v * a.sin()) * r, c + (u * b.cos() + v * b.sin()) * r));
    }
}

/// Maya's drawing override index colours, 0 to 31.
const INDEX_COLORS: [[u8; 3]; 32] = [
    [120, 120, 120], [0, 0, 0], [64, 64, 64], [153, 153, 153], [155, 0, 40], [0, 4, 96], [0, 0, 255], [0, 70, 25],
    [38, 0, 67], [200, 0, 200], [138, 72, 51], [63, 35, 31], [153, 38, 0], [255, 0, 0], [0, 255, 0], [0, 65, 153],
    [255, 255, 255], [255, 255, 0], [100, 220, 255], [67, 255, 163], [255, 176, 176], [228, 172, 121], [255, 255, 99], [0, 153, 84],
    [161, 106, 48], [158, 161, 48], [104, 161, 48], [48, 161, 93], [48, 161, 161], [48, 103, 161], [111, 48, 161], [161, 48, 106],
];

/// The drawing override on a node, or on one above it, as Maya inherits it.
fn override_color(g: &Graph, id: NodeId) -> Option<[u8; 4]> {
    let mut n = id;
    while n != NO_NODE {
        let node = g.node(n)?;
        if node.ty().dag && node.bool(DagNode::OVERRIDE_ENABLED) {
            if node.bool(DagNode::OVERRIDE_RGB_COLORS) {
                let c = node.data().vec3(DagNode::OVERRIDE_COLOR_RGB);
                return Some([(c.x * 255.0) as u8, (c.y * 255.0) as u8, (c.z * 255.0) as u8, 255]);
            }
            let i = node.int(DagNode::OVERRIDE_COLOR).clamp(0, 31) as usize;
            // Index 0 means no colour of its own.
            if i > 0 {
                let c = INDEX_COLORS[i];
                return Some([c[0], c[1], c[2], 255]);
            }
        }
        n = g.parent(n);
    }
    None
}

/// Each visible curve shape, its transform and its CVs in world space, one lap for a periodic curve.
fn curve_cvs(g: &Graph) -> Vec<(NodeId, NodeId, Vec<DVec3>)> {
    let mut out = Vec::new();
    for id in g.nodes() {
        let Some(n) = g.node(id) else { continue };
        if !n.ty().is_a("nurbsCurve") || n.bool(DagNode::INTERMEDIATE_OBJECT) || !visible(g, id) {
            continue;
        }
        let Some(c) = aztek_scene::curves::drawn_curve(n) else { continue };
        out.push((id, aztek_scene::dag::transform_of(g, id), c.distinct_cvs().to_vec()));
    }
    out
}

/// The selected lattices: transform, shape, grid size and points in world space.
fn selected_lattices(g: &Graph) -> Vec<(NodeId, NodeId, [u32; 3], Vec<DVec3>)> {
    let mut out = Vec::new();
    for &xf in g.selection() {
        let Some(shape) = aztek_scene::dag::shape_of_type(g, xf, "lattice") else { continue };
        let Some((d, pts)) = aztek_scene::deform_lattice::world_points(g, shape) else { continue };
        out.push((xf, shape, [d[0] as u32, d[1] as u32, d[2] as u32], pts));
    }
    out
}

pub fn rig_items(g: &Graph) -> Vec<RigItem> {
    use aztek_scene::rig::Joint;
    use aztek_scene::transform::{Locator, Transform};
    let mut out = Vec::new();
    for id in g.nodes() {
        let Some(n) = g.node(id) else { continue };
        let ty = n.ty();
        if !ty.dag || !visible(g, id) {
            continue;
        }
        let w = n.data().matrix(DagNode::WORLD_MATRIX);
        let c = w.w_axis.truncate();
        if ty.is_a("joint") {
            let radius = n.data().float(Joint::RADIUS) * 0.5;
            let (x, y, z) = (w.x_axis.truncate().normalize_or_zero(), w.y_axis.truncate().normalize_or_zero(), w.z_axis.truncate().normalize_or_zero());
            let mut segs = Vec::new();
            circle(c, x, y, radius, &mut segs);
            circle(c, y, z, radius, &mut segs);
            circle(c, z, x, radius, &mut segs);
            for &k in g.children(id) {
                let Some(kn) = g.node(k) else { continue };
                if !kn.ty().is_a("joint") {
                    continue;
                }
                let tip = kn.data().matrix(DagNode::WORLD_MATRIX).w_axis.truncate();
                let d = tip - c;
                let len = d.length();
                if len <= radius {
                    continue;
                }
                let dir = d / len;
                let (u, v) = dir.any_orthonormal_pair();
                let base = c + dir * radius;
                let k = radius * 0.6;
                let corners = [base + u * k, base + v * k, base - u * k, base - v * k];
                for i in 0..4 {
                    segs.push((corners[i], corners[(i + 1) % 4]));
                    segs.push((corners[i], tip));
                }
            }
            out.push(RigItem { node: id, color: override_color(g, id).unwrap_or([70, 110, 220, 255]), segs });
        } else if ty.is_a("deformHandle") {
            let kind = ty.attr_index("deformerType").map(|i| n.data().string(i).to_string()).unwrap_or_default();
            // A cluster's handle sits on its pivot.
            let at = w * glam::DMat4::from_translation(n.data().vec3(Transform::ROTATE_PIVOT));
            out.push(RigItem { node: id, color: [200, 120, 230, 255], segs: aztek_scene::deform::handle_lines(&kind, &at) });
        } else if ty.is_a("nurbsCurve") {
            if n.bool(DagNode::INTERMEDIATE_OBJECT) {
                continue;
            }
            let color = override_color(g, id).unwrap_or([10, 10, 110, 255]);
            out.push(RigItem { node: aztek_scene::dag::transform_of(g, id), color, segs: aztek_scene::curves::curve_lines(n) });
        } else if ty.is_a("softModHandle") {
            let at = w * glam::DMat4::from_translation(n.data().vec3(aztek_scene::transform::Shape::COUNT));
            out.push(RigItem { node: aztek_scene::dag::transform_of(g, id), color: [200, 120, 230, 255], segs: aztek_scene::deform::handle_lines("softMod", &at) });
        } else if ty.is_a("baseLattice") {
            out.push(RigItem { node: aztek_scene::dag::transform_of(g, id), color: [130, 70, 150, 255], segs: aztek_scene::deform_lattice::box_lines(&w) });
        } else if ty.is_a("lattice") {
            let Some((dims, pts)) = aztek_scene::deform_lattice::world_points(g, id) else { continue };
            let mut segs = aztek_scene::deform_lattice::grid_lines(dims, &pts);
            let cross = |p: DVec3, k: f64, segs: &mut Vec<(DVec3, DVec3)>| {
                for ax in [DVec3::X, DVec3::Y, DVec3::Z] {
                    segs.push((p - ax * k, p + ax * k));
                }
            };
            // A control point's mark, scaled to the lattice.
            let k = pts.first().zip(pts.get(1)).map_or(0.05, |(a, b)| (*a - *b).length() * 0.04).max(1e-4);
            for p in &pts {
                cross(*p, k, &mut segs);
            }
            out.push(RigItem { node: aztek_scene::dag::transform_of(g, id), color: [200, 120, 230, 255], segs });
            // Selected points show in vertex mode. No node: this is a highlight, and it mustn't pick or take selection colours.
            if components::mode(g) == ComponentMode::Vertex {
                let picked = components::selected(g, id, ComponentMode::Vertex);
                if !picked.is_empty() {
                    let mut segs = Vec::new();
                    for p in picked.iter().filter_map(|&i| pts.get(i as usize)) {
                        cross(*p, k * 2.5, &mut segs);
                    }
                    out.push(RigItem { node: aztek_scene::NO_NODE, color: [255, 200, 60, 255], segs });
                }
            }
        } else if ty.is_a("ikHandle") {
            let k = 0.4;
            let segs = vec![
                (c - DVec3::X * k, c + DVec3::X * k),
                (c - DVec3::Y * k, c + DVec3::Y * k),
                (c - DVec3::Z * k, c + DVec3::Z * k),
            ];
            out.push(RigItem { node: id, color: [200, 200, 255, 255], segs });
        } else if ty.is_a("locator") {
            let lp = n.data().vec3(Locator::LOCAL_POSITION);
            let ls = n.data().vec3(Locator::LOCAL_SCALE);
            let p = w.transform_point3(lp);
            let mut segs = Vec::new();
            for (i, ax) in [w.x_axis, w.y_axis, w.z_axis].iter().enumerate() {
                let d = ax.truncate() * ls[i];
                segs.push((p - d, p + d));
            }
            out.push(RigItem { node: aztek_scene::dag::transform_of(g, id), color: override_color(g, id).unwrap_or([60, 90, 230, 255]), segs });
        }
        let _ = Transform::TRANSLATE;
    }
    out
}
