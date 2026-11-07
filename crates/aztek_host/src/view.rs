//! One model panel: its surface, camera and mouse handling (navigation, picking, marquee, manipulator drags).

use aztek_scene::components::{self, ComponentMode};
use ash::vk;
use glam::{DMat4, DVec2, DVec3, Vec3};
use serde_json::{json, Value};

use crate::camera::CamKind;
use crate::core::Core;
use crate::font;
use crate::gpu::Gpu;
use crate::render::{c8, Kind, LineVertex, Push, Renderer};
use crate::viewport::{gather, Drag, Handle, Tool, Viewport};

pub const SHIFT: i32 = 1;
pub const CTRL: i32 = 2;
pub const ALT: i32 = 4;

/// One stroke of the paint skin weights brush: the weights as they build up. The stroke undoes as one.
struct PaintStroke {
    skin: aztek_scene::plug::NodeId,
    inf: u32,
    bind: aztek_scene::skin::SkinBind,
    applied: bool,
    invert: bool,
    smooth: bool,
    last: Option<DVec3>,
}

/// One stroke of a sculpting brush: the points as they build up, undoing as one.
struct SculptStroke {
    shape: aztek_scene::plug::NodeId,
    pos: Vec<DVec3>,
    base: Vec<DVec3>,
    mesh: std::sync::Arc<aztek_geo::Mesh>,
    world: DMat4,
    applied: bool,
    invert: bool,
    smooth: bool,
    last: Option<DVec3>,
}

pub struct View {
    pub gpu: Gpu,
    pub r: Renderer,
    pub vp: Viewport,
    /// Physical pixels per logical one.
    pub dpr: f64,
    nav: Option<(i32, DVec2)>,
    marquee: Option<DVec2>,
    joint_press: Option<DVec2>,
    /// The camera node this view looks through, by name, and what was last read from or written to it.
    pub cam_node: String,
    cam_sync: Option<[f64; 9]>,
    /// The modelling tools' state.
    pub ts: crate::tools::ToolState,
    /// A sculpt stroke in progress.
    sculpt: Option<SculptStroke>,
    /// A paint stroke in progress.
    stroke: Option<PaintStroke>,
    /// B held with the paint tool: a drag sizes the brush.
    pub brush_adjust: bool,
    brush_drag: Option<(DVec2, f64)>,
    /// Lines a drag's preview step wrote to the history, retracted before the next step.
    drag_lines: usize,
    /// The drag's last preview command, run for real on release.
    last_drag: Option<(String, Value)>,
    pub background: [f32; 4],
    pub show_grid: bool,
    pub needs_redraw: bool,
    /// Last frame read back on request, as RGB.
    pub capture: Option<(u32, u32, Vec<u8>)>,
    pub want_capture: bool,
}

impl View {
    pub fn new(gpu: Gpu, kind: CamKind, dpr: f64) -> View {
        let r = Renderer::new(&gpu);
        View {
            gpu,
            r,
            vp: Viewport::new(kind),
            dpr,
            nav: None,
            marquee: None,
            joint_press: None,
            cam_node: kind.name().to_string(),
            cam_sync: None,
            ts: Default::default(),
            sculpt: None,
            stroke: None,
            brush_adjust: false,
            brush_drag: None,
            drag_lines: 0,
            last_drag: None,
            background: [0.36, 0.36, 0.36, 1.0],
            show_grid: true,
            needs_redraw: true,
            capture: None,
            want_capture: false,
        }
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if let Err(e) = self.gpu.recreate_swapchain(w, h) {
            log::error!("viewport resize: {e}");
        }
        self.needs_redraw = true;
    }

    pub fn render(&mut self, core: &mut Core) {
        let e = self.gpu.extent;
        self.vp.rect = [0.0, 0.0, e.width as f64, e.height as f64];
        let bg = self.background;
        let mut frame = self.gpu.begin_frame([bg[0], bg[1], bg[2], 1.0]);
        if frame.is_none() {
            // Out of date (the window changed size): remake at the window's size and try once more.
            self.resize(e.width, e.height);
            frame = self.gpu.begin_frame([bg[0], bg[1], bg[2], 1.0]);
        }
        let e = self.gpu.extent;
        self.vp.rect = [0.0, 0.0, e.width as f64, e.height as f64];
        let Some(index) = frame else {
            self.needs_redraw = true;
            return;
        };
        self.read_camera(core);
        let items = gather(&mut core.s.graph);
        self.vp.show_grid = self.show_grid;
        self.vp.draw(&mut self.gpu, &self.r, &mut core.s, &items, self.dpr);
        self.overlay();
        let ok = self.gpu.end_frame(index);
        if self.want_capture {
            self.want_capture = false;
            match self.gpu.read_back(index) {
                Ok(c) => self.capture = Some(c),
                Err(e) => log::error!("capture: {e}"),
            }
        }
        if !ok {
            let (w, h) = (e.width, e.height);
            self.resize(w, h);
        }
        self.needs_redraw = self.vp.drag.is_some() && false;
    }

    /// Screen-space extras: marquee, axis triad, camera name.
    fn overlay(&mut self) {
        let [_, _, w, h] = self.vp.rect;
        if w < 2.0 || h < 2.0 {
            return;
        }
        let mut lines: Vec<LineVertex> = Vec::new();
        let mut seg = |a: DVec2, b: DVec2, c: [u8; 4]| {
            lines.push(LineVertex { pos: [a.x as f32, a.y as f32, 0.5], color: c8(c) });
            lines.push(LineVertex { pos: [b.x as f32, b.y as f32, 0.5], color: c8(c) });
        };
        if let Some((a, b)) = self.vp.marquee {
            let (lo, hi) = (a.min(b), a.max(b));
            let c = [255, 255, 255, 255];
            seg(lo, DVec2::new(hi.x, lo.y), c);
            seg(DVec2::new(hi.x, lo.y), hi, c);
            seg(hi, DVec2::new(lo.x, hi.y), c);
            seg(DVec2::new(lo.x, hi.y), lo, c);
        }
        // Axis triad, bottom left.
        let s = self.dpr;
        let origin = DVec2::new(34.0 * s, h - 34.0 * s);
        let view = self.vp.cam.view();
        let axes = [(DVec3::X, [230, 50, 50, 255], 'x'), (DVec3::Y, [60, 220, 60, 255], 'y'), (DVec3::Z, [70, 110, 255, 255], 'z')];
        for (ax, c, name) in axes {
            let v = view.transform_vector3(ax);
            let tip = origin + DVec2::new(v.x, -v.y) * 20.0 * s;
            seg(origin, tip, c);
            let lp = origin + DVec2::new(v.x, -v.y) * 27.0 * s - DVec2::new(3.0, -4.0) * s;
            for (a, b) in font::strokes(&name.to_string(), lp, 7.0 * s) {
                seg(a, b, c);
            }
        }
        // Camera name, bottom centre.
        let name = self.cam_node.as_str();
        let size = 10.0 * s;
        let tw = font::width(name, size);
        for (a, b) in font::strokes(name, DVec2::new(w * 0.5 - tw * 0.5, h - 12.0 * s), size) {
            seg(a, b, [225, 225, 225, 255]);
        }
        if lines.is_empty() {
            return;
        }
        // Pixels straight to clip space.
        let m = DMat4::from_cols_array(&[2.0 / w, 0.0, 0.0, 0.0, 0.0, 2.0 / h, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, -1.0, 0.0, 1.0]);
        let b = self.gpu.buffer_with(vk::BufferUsageFlags::VERTEX_BUFFER, &lines);
        self.r.set_viewport(&self.gpu, 0.0, 0.0, w as f32, h as f32);
        self.r.bind(&self.gpu, Kind::LinesOnTop);
        self.r.set_line_width(&self.gpu, (1.0 * s) as f32);
        let mut p: Push = bytemuck::Zeroable::zeroed();
        p.mvp = m.as_mat4().to_cols_array();
        p.n1[3] = 1.0;
        p.color = [1.0; 4];
        self.r.push(&self.gpu, &p);
        self.r.draw(&self.gpu, &b, None, lines.len() as u32);
        self.gpu.drop_buffer(b);
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Mouse

    fn camera_state(&self) -> [f64; 9] {
        let c = &self.vp.cam;
        let eye = c.eye();
        [eye.x, eye.y, eye.z, c.yaw, c.pitch, c.distance, c.ortho_width, c.hfov, c.orthographic as i32 as f64]
    }

    /// Follows the camera node when a script moved it.
    fn read_camera(&mut self, core: &mut Core) {
        use aztek_scene::cameras as cam;
        let g = &mut core.s.graph;
        let Ok(Some(id)) = g.find(&self.cam_node) else { return };
        let Some(sh) = cam::shape_of(g, id) else { return };
        g.evaluate();
        let tr = aztek_scene::dag::transform_of(g, sh);
        let w = aztek_scene::rig::world(g, tr);
        let d = g.node(sh).unwrap().data();
        let eye = w.w_axis.truncate();
        let z = w.z_axis.truncate().normalize_or_zero();
        let fl = d.float(cam::FOCAL_LENGTH);
        let hfa = d.float(cam::H_APERTURE);
        let hfov = (2.0 * (hfa * 25.4 * 0.5 / fl.max(1e-6)).atan()).to_degrees();
        let (ow, coi, ortho) = (d.float(cam::ORTHO_WIDTH), d.float(cam::COI).max(1e-3), d.bool(cam::ORTHOGRAPHIC));
        let yaw = z.x.atan2(z.z).to_degrees();
        let pitch = z.y.clamp(-1.0, 1.0).asin().to_degrees();
        let state = [eye.x, eye.y, eye.z, yaw, pitch, coi, ow, hfov, ortho as i32 as f64];
        let same = self.cam_sync.is_some_and(|s| s.iter().zip(&state).all(|(a, b)| (a - b).abs() < 1e-6));
        if same {
            return;
        }
        self.cam_sync = Some(state);
        let c = &mut self.vp.cam;
        c.yaw = yaw;
        c.pitch = pitch;
        c.distance = coi;
        c.ortho_width = ow;
        c.hfov = hfov;
        c.orthographic = ortho;
        c.coi = eye - c.dir_to_eye() * coi;
    }

    /// Writes what the mouse did to the view back to the camera node. Not undoable, as Maya's default.
    pub fn write_camera(&mut self, core: &mut Core) {
        use aztek_scene::cameras as cam;
        use aztek_scene::plug::{PlugRef, PlugValue};
        use aztek_scene::transform::Transform;
        let g = &mut core.s.graph;
        let Ok(Some(id)) = g.find(&self.cam_node) else { return };
        let Some(sh) = cam::shape_of(g, id) else { return };
        let tr = aztek_scene::dag::transform_of(g, sh);
        let (eye, yaw, pitch, dist, ow, ortho) = {
            let c = &self.vp.cam;
            (c.eye(), c.yaw, c.pitch, c.distance, c.ortho_width, c.orthographic)
        };
        // Driven or locked cameras keep to what drives them.
        g.without_undo(|g| {
            let _ = g.set_value(PlugRef::new(tr, Transform::TRANSLATE as i32), PlugValue::Vec3(eye));
            let _ = g.set_value(PlugRef::new(tr, Transform::ROTATE as i32), PlugValue::Vec3(cam::rotate_for(yaw, pitch)));
            let _ = g.set_value(PlugRef::new(sh, cam::COI as i32), PlugValue::Float(dist));
            if ortho {
                let _ = g.set_value(PlugRef::new(sh, cam::ORTHO_WIDTH as i32), PlugValue::Float(ow));
            }
        });
        // What's written is what's read back, so the next read leaves the view alone.
        self.cam_sync = None;
        self.read_camera(core);
    }

    /// kind: 0 press, 1 move, 2 release. button: 1 left, 2 middle, 3 right (0 on moves). buttons: held, bit per button.
    /// Returns whether the view should redraw.
    pub fn mouse(&mut self, core: &mut Core, kind: i32, button: i32, px: DVec2, mods: i32, buttons: i32) -> bool {
        let items = gather(&mut core.s.graph);
        let alt = mods & ALT != 0;
        if !alt && self.nav.is_none() && (kind != 1 || buttons == 0 || buttons & 1 != 0 || buttons & 2 != 0) {
            if let Some(r) = self.tool_mouse(core, kind, button, px, mods, &items) {
                return r;
            }
        }
        match kind {
            0 => {
                if alt {
                    self.nav = Some((button, px));
                    return false;
                }
                if self.vp.tool == Tool::Sculpt && button == 1 {
                    if self.brush_adjust {
                        self.brush_drag = Some((px, core.s.sculpt_ctx.size));
                        return true;
                    }
                    if let Some((i, wp, n)) = self.vp.brush_hit(&core.s, &items, px) {
                        let shape = items[i].shape;
                        if let Some(m) = aztek_scene::modifiers::cage(&mut core.s.graph, shape) {
                            let pos = m.positions().to_vec();
                            self.sculpt = Some(SculptStroke { shape, base: pos.clone(), pos, mesh: m, world: items[i].world, applied: false, invert: mods & CTRL != 0, smooth: mods & SHIFT != 0, last: None });
                            self.sculpt_dab(core, wp, n);
                            self.vp.brush = Some((wp, n));
                        }
                    }
                    return true;
                }
                if self.vp.tool == Tool::PaintSkin && button == 1 {
                    if self.brush_adjust {
                        self.brush_drag = Some((px, core.s.paint_ctx.radius));
                        return true;
                    }
                    if let Some((i, wp, n)) = self.vp.brush_hit(&core.s, &items, px) {
                        let skin = aztek_scene::skin::skin_of(&core.s.graph, items[i].shape);
                        if let Some(skin) = skin {
                            if let (Some(b), Some(inf)) = (aztek_scene::skin::bind_of(&core.s.graph, skin), aztek_scene::skin::paint_influence(&core.s, skin)) {
                                self.stroke = Some(PaintStroke { skin, inf, bind: (*b).clone(), applied: false, invert: mods & CTRL != 0, smooth: mods & SHIFT != 0, last: None });
                                self.paint_dab(core, &items, i, wp, n);
                                self.vp.brush = Some((wp, n));
                            }
                        }
                        return true;
                    }
                    self.marquee = Some(px);
                    return false;
                }
                self.vp.update_frame(&mut core.s);
                let pivot = self.vp.manip_pivot(&mut core.s, &items);
                let hit = pivot.and_then(|pv| self.vp.manip_hit(pv, px, self.dpr));
                let start_drag = match (button, hit, pivot) {
                    (1, Some(h), Some(_)) => Some(h),
                    // Middle drag anywhere drags the active handle, or in the view plane.
                    (2, h, Some(_)) => Some(h.or(self.vp.hover).unwrap_or(Handle::Center)),
                    _ => None,
                };
                if let (Some(h), Some(pv)) = (start_drag, pivot) {
                    let comps = components::mode(&core.s.graph) != ComponentMode::Object;
                    let targets = core.s.graph.selection().iter().map(|&id| core.s.graph.path(id)).collect();
                    // Points a V-snapped move can land on: other objects' vertices, or the unselected ones of these.
                    let mut snap_points = Vec::new();
                    {
                        let g = &core.s.graph;
                        let sel = g.selection();
                        let mode = components::mode(g);
                        for it in &items {
                            let picked = sel.contains(&it.xf);
                            if picked && !comps {
                                continue;
                            }
                            let skip: std::collections::HashSet<u32> = if picked {
                                components::convert(&it.mesh, mode, components::selected(g, it.shape, mode), ComponentMode::Vertex, false).into_iter().collect()
                            } else {
                                Default::default()
                            };
                            for p in 0..it.mesh.point_capacity() {
                                if it.mesh.point_valid(p) && !skip.contains(&p) {
                                    snap_points.push(it.world.transform_point3(it.mesh.position(p)));
                                }
                            }
                        }
                    }
                    self.vp.drag = Some(Drag { handle: h, start: px, pivot: pv, applied: false, components: comps, targets, snap_points });
                    self.drag_lines = 0;
                    return true;
                }
                if button == 1 && matches!(self.vp.tool, Tool::Joint | Tool::CurveCv | Tool::CurveEp) {
                    self.joint_press = Some(px);
                    return false;
                }
                if button == 1 {
                    self.marquee = Some(px);
                }
                false
            }
            1 => {
                if let Some((b, last)) = self.nav {
                    let d = px - last;
                    let aspect = self.vp.aspect();
                    match b {
                        1 => self.vp.cam.tumble(d / self.dpr),
                        2 => self.vp.cam.track(d, self.vp.rect[3], aspect),
                        _ => self.vp.cam.dolly((d.x + d.y) / self.dpr),
                    }
                    self.nav = Some((b, px));
                    self.write_camera(core);
                    return true;
                }
                if let Some((start, radius)) = self.brush_drag {
                    let upp = self.vp.cam.units_per_px_at(self.vp.brush.map(|b| b.0).unwrap_or(self.vp.cam.coi), self.vp.rect[3], self.vp.aspect());
                    let next = (radius + (px.x - start.x) * upp).max(1e-3);
                    if self.vp.tool == Tool::Sculpt {
                        core.s.sculpt_ctx.size = next;
                        core.version += 1;
                    } else {
                        let _ = core.exec_silent("artAttrSkinPaintCtx", json!({"edit": true, "radius": next}));
                    }
                    return true;
                }
                if self.sculpt.is_some() {
                    if let Some((_, wp, n)) = self.vp.brush_hit(&core.s, &items, px) {
                        self.sculpt_dab(core, wp, n);
                        self.vp.brush = Some((wp, n));
                    } else if core.s.sculpt_ctx.tool == "grab" {
                        // Grab keeps following off the surface, on the plane through the stroke's last point.
                        if let (Some(l), Some((_, n))) = (self.sculpt.as_ref().and_then(|s| s.last), self.vp.brush) {
                            let (o, d) = self.vp.cam.ray(px, self.vp.size());
                            let den = d.dot(n);
                            if den.abs() > 1e-6 {
                                let t = (l - o).dot(n) / den;
                                let wp = o + d * t;
                                self.sculpt_dab(core, wp, n);
                                self.vp.brush = Some((wp, n));
                            }
                        }
                    }
                    return true;
                }
                if self.vp.tool == Tool::Sculpt && buttons == 0 {
                    let was = self.vp.brush.is_some();
                    self.vp.brush = self.vp.brush_hit(&core.s, &items, px).map(|x| (x.1, x.2));
                    return was || self.vp.brush.is_some();
                }
                if self.stroke.is_some() {
                    if let Some((i, wp, n)) = self.vp.brush_hit(&core.s, &items, px) {
                        self.paint_dab(core, &items, i, wp, n);
                        self.vp.brush = Some((wp, n));
                    }
                    return true;
                }
                if self.vp.tool == Tool::PaintSkin && buttons == 0 {
                    let was = self.vp.brush.is_some();
                    self.vp.brush = self.vp.brush_hit(&core.s, &items, px).map(|x| (x.1, x.2));
                    return was || self.vp.brush.is_some();
                }
                if let Some(d) = &self.vp.drag {
                    if px == d.start {
                        return false;
                    }
                    if let Some((cmd, call)) = self.vp.drag_call(d, px, self.dpr) {
                        // Previews run quietly; the release echoes the one command that stays.
                        if d.applied {
                            core.retract_preview();
                        }
                        let ok = core.exec_silent(&cmd, call.clone()).is_ok();
                        self.last_drag = if ok { Some((cmd, call)) } else { None };
                        self.vp.drag.as_mut().unwrap().applied = ok;
                    }
                    return true;
                }
                if let Some(start) = self.marquee {
                    self.vp.marquee = Some((start, px));
                    return true;
                }
                if buttons == 0 {
                    let pivot = self.vp.manip_pivot(&mut core.s, &items);
                    let hover = pivot.and_then(|pv| self.vp.manip_hit(pv, px, self.dpr));
                    let comp = if hover.is_none() {
                        self.vp.pick_component(&core.s, &items, px).map(|(i, k, id)| (items[i].shape, k, id))
                    } else {
                        None
                    };
                    if hover != self.vp.hover || comp != self.vp.hover_comp {
                        self.vp.hover = hover;
                        self.vp.hover_comp = comp;
                        return true;
                    }
                }
                false
            }
            3 => {
                // Double-click: an edge loop in edge mode, as Maya does.
                if button == 1 && components::mode(&core.s.graph) == ComponentMode::Edge {
                    let _ = core.exec("selectLoop", json!({}));
                    return true;
                }
                false
            }
            _ => {
                if self.nav.is_some_and(|(b, _)| b == button) {
                    self.nav = None;
                    return true;
                }
                if self.brush_drag.take().is_some() {
                    return true;
                }
                if self.sculpt.take().is_some() {
                    core.version += 1;
                    return true;
                }
                if self.stroke.take().is_some() {
                    // The dabs' one undo entry stays; the panels hear of it.
                    core.version += 1;
                    return true;
                }
                if let Some(d) = self.vp.drag.take() {
                    if d.applied {
                        if let Some((cmd, call)) = self.last_drag.take() {
                            core.retract_preview();
                            let _ = core.exec(&cmd, call);
                        }
                    }
                    return true;
                }
                if let Some(start) = self.joint_press.take() {
                    if (px - start).length() < 6.0 * self.dpr {
                        let at = self.vp.place_point(px);
                        if self.vp.tool == Tool::Joint {
                            place_joint(core, at);
                        } else {
                            self.vp.curve_points.push(at);
                        }
                    }
                    return true;
                }
                if let Some(start) = self.marquee.take() {
                    self.vp.marquee = None;
                    let picked = if (px - start).length() < 4.0 * self.dpr {
                        self.vp.pick(&core.s, &items, px)
                    } else {
                        self.vp.pick_rect(&core.s, &items, start, px)
                    };
                    select_picked(core, picked, mods);
                    return true;
                }
                false
            }
        }
    }

    fn sculpt_dab(&mut self, core: &mut Core, wp: DVec3, n: DVec3) {
        let Some(st) = self.sculpt.as_mut() else { return };
        let ctx = core.s.sculpt_ctx.clone();
        if ctx.tool == "freeze" {
            let mut fr = core.s.sculpt_ctx.frozen.remove(&st.shape).unwrap_or_default();
            aztek_scene::sculpt::paint_freeze(&mut fr, &st.mesh, &st.world, wp, &ctx, st.invert);
            core.s.sculpt_ctx.frozen.insert(st.shape, fr);
            return;
        }
        let pull = st.last.map(|l| wp - l).unwrap_or(DVec3::ZERO);
        let frozen = ctx.frozen.get(&st.shape).map(|v| v.as_slice());
        aztek_scene::sculpt::dab(&mut st.pos, &st.base, &st.mesh, &st.world, wp, n, pull, &ctx, frozen, st.invert, st.smooth);
        st.last = Some(wp);
        if st.applied {
            core.s.undo();
        }
        let (shape, pos, mesh) = (st.shape, st.pos.clone(), st.mesh.clone());
        let ok = aztek_scene::sculpt::commit(&mut core.s, shape, &pos, &mesh).is_ok();
        if let Some(st) = self.sculpt.as_mut() {
            st.applied = ok;
        }
        core.version += 1;
    }

    /// A dab of the brush, replacing the stroke's previous undo entry so the whole stroke undoes in one step.
    fn paint_dab(&mut self, core: &mut Core, items: &[crate::viewport::DrawItem], i: usize, wp: DVec3, n: DVec3) {
        let Some(st) = self.stroke.as_mut() else { return };
        let ctx = core.s.paint_ctx.clone();
        // Dabs spaced by a fraction of the radius, so a fast drag leaves no gaps.
        let dabs = match st.last {
            Some(l) => {
                let steps = (((wp - l).length() / (ctx.radius * 0.25).max(1e-6)).ceil() as usize).clamp(1, 32);
                (1..=steps).map(|k| l.lerp(wp, k as f64 / steps as f64)).collect()
            }
            None => vec![wp],
        };
        for at in dabs {
            aztek_scene::skin::stamp(&mut st.bind, &items[i].mesh, &items[i].world, at, Some(n), st.inf, &ctx, st.invert, st.smooth);
        }
        st.last = Some(wp);
        if st.applied {
            core.s.undo();
        }
        st.applied = aztek_scene::skin::commit_paint(&mut core.s.graph, st.skin, st.bind.clone()).is_ok();
        core.version += 1;
    }

    pub fn wheel(&mut self, core: &mut Core, delta: f64) {
        self.vp.cam.dolly(delta * 0.5);
        self.write_camera(core);
    }

    pub fn set(&mut self, core: &mut Core, key: &str, v: &Value) {
        match key {
            "tool" => {
                self.ts = Default::default();
                self.vp.tool_lines.clear();
                self.vp.tool_points.clear();
                self.vp.tool = match v.as_str().unwrap_or("") {
                    "move" => Tool::Move,
                    "rotate" => Tool::Rotate,
                    "scale" => Tool::Scale,
                    "joint" => Tool::Joint,
                    "paintSkin" => Tool::PaintSkin,
                    "sculpt" => Tool::Sculpt,
                    "insertJoint" => Tool::InsertJoint,
                    "curveCv" => Tool::CurveCv,
                    "curveEp" => Tool::CurveEp,
                    "multiCut" => Tool::MultiCut,
                    "insertLoop" => Tool::InsertLoop,
                    "offsetLoop" => Tool::OffsetLoop,
                    "targetWeld" => Tool::TargetWeld,
                    "slideEdge" => Tool::SlideEdge,
                    "crease" => Tool::Crease,
                    "createPolygon" => Tool::CreatePolygon,
                    "appendPolygon" => Tool::AppendPolygon,
                    "quadDraw" => Tool::QuadDraw,
                    _ => Tool::Select,
                }
            }
            "wireframe" => self.vp.wireframe = v.as_bool().unwrap_or(false),
            "wireOnShaded" => self.vp.wire_on_shaded = v.as_bool().unwrap_or(false),
            "grid" => self.show_grid = v.as_bool().unwrap_or(true),
            "camera" => {
                if let Some(name) = v.as_str() {
                    if let Some(k) = CamKind::from_name(name) {
                        self.vp.cam = crate::camera::Camera::new(k);
                        self.vp.reset_grid(&mut self.gpu);
                    }
                    self.cam_node = name.to_string();
                    self.cam_sync = None;
                    self.read_camera(core);
                }
            }
            "frame" => {
                let items = gather(&mut core.s.graph);
                self.vp.frame(&mut core.s, &items, v.as_str() == Some("all"));
                self.write_camera(core);
            }
            "home" => {
                let k = self.vp.cam.kind;
                self.vp.cam = crate::camera::Camera::new(k);
                self.write_camera(core);
            }
            "dpr" => self.dpr = v.as_f64().unwrap_or(1.0),
            "background" => {
                let g = v.as_f64().unwrap_or(0.36) as f32;
                self.background = [g, g, g, 1.0];
            }
            "manipSize" => self.vp.manip_scale = v.as_f64().unwrap_or(1.0),
            "axisOrientation" => self.vp.axis_mode = v.as_str().unwrap_or("world").to_string(),
            "curveDegree" => self.vp.curve_degree = v.as_i64().unwrap_or(3),
            "toolTab" => self.ts.tab = v.as_bool().unwrap_or(false),
            // Enter: makes the curve; the tool stays for the next.
            "finishTool" => {
                let items = gather(&mut core.s.graph);
                self.tool_finish(core, &items);
                if matches!(self.vp.tool, Tool::CurveCv | Tool::CurveEp) && self.vp.curve_points.len() >= 2 {
                    let pts: Vec<[f64; 3]> = self.vp.curve_points.drain(..).map(|p| p.to_array()).collect();
                    let flag = if self.vp.tool == Tool::CurveEp { "editPoint" } else { "point" };
                    let _ = core.exec("curve", json!({"degree": self.vp.curve_degree, flag: pts}));
                } else {
                    self.vp.curve_points.clear();
                }
            }
            "undoToolPoint" => {
                self.tool_back();
                self.vp.curve_points.pop();
            }
            "brushAdjust" => self.brush_adjust = v.as_bool().unwrap_or(false),
            "smoothCage" => self.vp.smooth_cage = v.as_bool().unwrap_or(false),
            "pivotMode" => self.vp.pivot_mode = v.as_bool().unwrap_or(false),
            "snap" => {
                self.vp.snap = match v.as_str() {
                    Some("grid") => 1,
                    Some("point") => 2,
                    _ => 0,
                }
            }
            _ => {}
        }
        self.needs_redraw = true;
    }

    pub fn get(&self, key: &str) -> Value {
        match key {
            "camera" => json!(self.cam_node),
            "tool" => json!(format!("{:?}", self.vp.tool).to_lowercase()),
            "wireframe" => json!(self.vp.wireframe),
            "wireOnShaded" => json!(self.vp.wire_on_shaded),
            "grid" => json!(self.show_grid),
            "dragging" => json!(self.vp.drag.is_some() || self.nav.is_some() || self.marquee.is_some()),
            _ => Value::Null,
        }
    }
}

/// The joint tool's click: a joint under the selected one, the parent turned to aim at it.
pub fn place_joint(core: &mut Core, at: DVec3) {
    let g = &core.s.graph;
    let parent = g.selection().last().copied().filter(|&id| aztek_scene::rig::is_joint(g, id)).map(|id| g.path(id));
    let Ok(v) = core.exec("joint", json!({"position": [at.x, at.y, at.z]})) else { return };
    if let Some(p) = parent {
        let _ = core.exec_quiet("joint", json!({"args": [p], "edit": true, "orientJoint": "xyz", "secondaryAxisOrient": "yup"}), aztek_ops::CallSource::Script);
        if let Some(j) = v.as_str() {
            let _ = core.exec_quiet("joint", json!({"args": [j], "edit": true, "orientJoint": "xyz", "secondaryAxisOrient": "yup"}), aztek_ops::CallSource::Script);
        }
    }
}

/// Maya's click modifiers: Shift toggles, Ctrl deselects, Ctrl+Shift adds.
pub fn select_picked(core: &mut Core, picked: Vec<String>, mods: i32) {
    let (shift, ctrl) = (mods & SHIFT != 0, mods & CTRL != 0);
    let comps = components::mode(&core.s.graph) != ComponentMode::Object;
    if picked.is_empty() {
        if !shift && !ctrl {
            if comps {
                // Clicking nothing in a component mode clears the components and keeps the objects hilited.
                let objs: Vec<String> = core.s.graph.selection().iter().map(|&id| core.s.graph.path(id)).collect();
                let _ = core.exec("select", json!({"args": objs}));
                components::set_all(&mut core.s.graph, Default::default(), Default::default(), Default::default());
            } else {
                let _ = core.exec("select", json!({"clear": true}));
            }
        }
        return;
    }
    let call = if ctrl && shift {
        json!({"args": picked, "add": true})
    } else if ctrl {
        json!({"args": picked, "deselect": true})
    } else if shift {
        json!({"args": picked, "toggle": true})
    } else {
        json!({"args": picked})
    };
    let _ = core.exec("select", call);
}

#[allow(dead_code)]
fn v3(v: DVec3) -> Vec3 {
    v.as_vec3()
}
