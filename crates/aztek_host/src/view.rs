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

pub struct View {
    pub gpu: Gpu,
    pub r: Renderer,
    pub vp: Viewport,
    /// Physical pixels per logical one.
    pub dpr: f64,
    nav: Option<(i32, DVec2)>,
    marquee: Option<DVec2>,
    joint_press: Option<DVec2>,
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
        let name = self.vp.cam.kind.name();
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

    /// kind: 0 press, 1 move, 2 release. button: 1 left, 2 middle, 3 right (0 on moves). buttons: held, bit per button.
    /// Returns whether the view should redraw.
    pub fn mouse(&mut self, core: &mut Core, kind: i32, button: i32, px: DVec2, mods: i32, buttons: i32) -> bool {
        let items = gather(&mut core.s.graph);
        let alt = mods & ALT != 0;
        match kind {
            0 => {
                if alt {
                    self.nav = Some((button, px));
                    return false;
                }
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
                if button == 1 && self.vp.tool == Tool::Joint {
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
                    return true;
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
                        place_joint(core, self.vp.place_point(px));
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

    pub fn wheel(&mut self, delta: f64) {
        self.vp.cam.dolly(delta * 0.5);
    }

    pub fn set(&mut self, core: &mut Core, key: &str, v: &Value) {
        match key {
            "tool" => {
                self.vp.tool = match v.as_str().unwrap_or("") {
                    "move" => Tool::Move,
                    "rotate" => Tool::Rotate,
                    "scale" => Tool::Scale,
                    "joint" => Tool::Joint,
                    _ => Tool::Select,
                }
            }
            "wireframe" => self.vp.wireframe = v.as_bool().unwrap_or(false),
            "wireOnShaded" => self.vp.wire_on_shaded = v.as_bool().unwrap_or(false),
            "grid" => self.show_grid = v.as_bool().unwrap_or(true),
            "camera" => {
                if let Some(k) = v.as_str().and_then(CamKind::from_name) {
                    self.vp.cam = crate::camera::Camera::new(k);
                    self.vp.reset_grid(&mut self.gpu);
                }
            }
            "frame" => {
                let items = gather(&mut core.s.graph);
                self.vp.frame(&mut core.s, &items, v.as_str() == Some("all"));
            }
            "home" => {
                let k = self.vp.cam.kind;
                self.vp.cam = crate::camera::Camera::new(k);
            }
            "dpr" => self.dpr = v.as_f64().unwrap_or(1.0),
            "background" => {
                let g = v.as_f64().unwrap_or(0.36) as f32;
                self.background = [g, g, g, 1.0];
            }
            "manipSize" => self.vp.manip_scale = v.as_f64().unwrap_or(1.0),
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
            "camera" => json!(self.vp.cam.kind.name()),
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
