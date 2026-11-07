//! The Move, Rotate and Scale tool options: mode numbers, what a drag's command becomes under each option, the custom
//! axes and soft select's falloff modes.

use aztek_host::axis_orient;
use aztek_host::camera::CamKind;
use aztek_host::ctx_cmds::axis_name;
use aztek_host::viewport::{Drag, Handle, Tool, Viewport};
use aztek_host::Core;
use glam::{DVec2, DVec3};
use serde_json::{json, Value};

fn view(tool: Tool) -> Viewport {
    let mut vp = Viewport::new(CamKind::Persp);
    vp.rect = [0.0, 0.0, 1000.0, 800.0];
    vp.tool = tool;
    vp
}

fn drag(handle: Handle, components: bool, pivot: DVec3, targets: &[&str]) -> Drag {
    Drag {
        handle,
        start: DVec2::new(500.0, 400.0),
        pivot,
        applied: false,
        components,
        choose_axis: false,
        targets: targets.iter().map(|s| s.to_string()).collect(),
        snap_points: Vec::new(),
        snap_curves: Vec::new(),
    }
}

/// The modes as the 2024 command reference numbers them.
#[test]
fn mode_numbers_follow_the_reference() {
    for (m, name) in [(0, "object"), (1, "parent"), (2, "world"), (3, "normal"), (4, "rotationAxis"), (5, "liveObject"), (6, "custom"), (10, "component")] {
        assert_eq!(axis_name("manipMoveContext", m), name);
        assert_eq!(axis_name("manipScaleContext", m), name);
    }
    for (m, name) in [(0, "object"), (1, "world"), (2, "gimbal"), (3, "custom"), (10, "component")] {
        assert_eq!(axis_name("manipRotateContext", m), name);
    }
}

#[test]
fn context_flags_take_the_references_short_names() {
    let mut c = Core::new();
    for (flag, v) in [("pcp", "1"), ("twk", "1"), ("spp", "1"), ("spo", "1"), ("pin", "1"), ("xn", "1"), ("vis", "1")] {
        c.mel_checked(&format!("manipMoveContext -e -{flag} {v} Move;")).unwrap_or_else(|e| panic!("{flag}: {e}"));
    }
    c.mel_checked("manipScaleContext -e -pns 0 -ump 1 Scale;").unwrap();
    c.mel_checked("manipRotateContext -e -ucp 1 -mt 1 Rotate;").unwrap();
}

#[test]
fn normal_orientation_moves_each_point_along_its_own_normal() {
    let mut c = Core::new();
    c.mel("polySphere -sa 8 -sh 8; select pSphere1.vtx[0:7];");
    let mut vp = view(Tool::Move);
    vp.axis_mode = "normal".into();
    vp.update_frame(&mut c.s);
    let d = drag(Handle::Axis(2), true, DVec3::ZERO, &[]);
    let px = d.start + DVec2::new(0.0, -50.0);
    let (cmd, call) = vp.drag_call(&d, px, 1.0).unwrap();
    assert_eq!(cmd, "moveComponents");
    let uvn = call["uvn"].as_array().expect("uvn");
    let n = uvn[2].as_f64().unwrap();
    assert!(n.abs() > 1e-3, "{call}");
    assert!(uvn[0].as_f64().unwrap().abs() < 1e-9 && uvn[1].as_f64().unwrap().abs() < 1e-9, "only N: {call}");
    // They go apart: the ring opens or shrinks, which one offset for all could not do.
    let p = |c: &mut Core, i: i32| {
        let v = c.mel(&format!("xform -q -ws -t pSphere1.vtx[{i}];"));
        DVec3::new(v[0].as_f64().unwrap(), v[1].as_f64().unwrap(), v[2].as_f64().unwrap())
    };
    let (a0, b0) = (p(&mut c, 0), p(&mut c, 4));
    c.exec("moveComponents", json!({"uvn": [0.0, 0.0, 0.5]})).unwrap();
    let (a1, b1) = (p(&mut c, 0), p(&mut c, 4));
    assert!((a1 - b1).length() > (a0 - b0).length() + 0.1);
}

#[test]
fn move_normal_commands_run() {
    let mut c = Core::new();
    c.mel("polyCube; select pCube1.f[1];");
    c.mel_checked("polyMoveFacet -ltz 0.5 pCube1.f[1];").unwrap();
    c.mel_checked("moveVertexAlongDirection -n 0.25 pCube1.vtx[2];").unwrap();
    let v = c.mel("xform -q -ws -t pCube1.vtx[2];");
    assert!(v[1].as_f64().unwrap() > 1.0, "{v}");
}

#[test]
fn step_snap_relative_and_absolute() {
    let mut c = Core::new();
    c.mel("polyCube;");
    let mut vp = view(Tool::Move);
    vp.update_frame(&mut c.s);
    vp.step_move = 1.0;
    let pivot = DVec3::new(0.3, 0.0, 0.0);
    let d = drag(Handle::Axis(0), false, pivot, &["pCube1"]);
    let s = vp.screen();
    let dir = (s.project(pivot + DVec3::X).unwrap() - s.project(pivot).unwrap()).normalize();
    let upp = vp.cam.units_per_px_at(pivot, 800.0, vp.aspect());
    let px = d.start + dir * (1.2 / upp);
    let by = |vp: &Viewport| vp.drag_call(&d, px, 1.0).unwrap().1["by"][0].as_f64().unwrap();
    let rel = by(&vp);
    assert!((rel - rel.round()).abs() < 1e-9, "whole steps of the offset: {rel}");
    vp.snap_relative = false;
    let abs = by(&vp);
    assert!(((abs + 0.3) - (abs + 0.3).round()).abs() < 1e-9, "lands on a whole step: {abs}");
}

#[test]
fn j_held_steps_with_no_step_set() {
    let mut c = Core::new();
    c.mel("polyCube;");
    let mut vp = view(Tool::Move);
    vp.update_frame(&mut c.s);
    vp.discrete_hold = true;
    let d = drag(Handle::Axis(0), false, DVec3::ZERO, &["pCube1"]);
    let v = vp.drag_call(&d, d.start + DVec2::new(170.0, 40.0), 1.0).unwrap().1["by"][0].as_f64().unwrap();
    assert!((v - v.round()).abs() < 1e-9, "{v}");
}

#[test]
fn preserve_children_goes_on_the_command() {
    let mut c = Core::new();
    c.mel("polyCube; polySphere; parent pSphere1 pCube1;");
    let mut vp = view(Tool::Move);
    vp.update_frame(&mut c.s);
    vp.preserve_children = true;
    let d = drag(Handle::Axis(0), false, DVec3::ZERO, &["pCube1"]);
    let (cmd, call) = vp.drag_call(&d, d.start + DVec2::new(60.0, 0.0), 1.0).unwrap();
    assert_eq!(cmd, "move");
    assert_eq!(call["preserveChildPosition"], json!(true));
    c.exec(&cmd, call).unwrap();
    let w: Value = c.mel("xform -q -ws -t pCube1|pSphere1;");
    assert!(w[0].as_f64().unwrap().abs() < 1e-9, "the child stayed: {w}");
}

#[test]
fn prevent_negative_scale() {
    let mut vp = view(Tool::Scale);
    let d = drag(Handle::Center, false, DVec3::ZERO, &["a"]);
    let px = d.start + DVec2::new(-300.0, 0.0);
    let f = |vp: &Viewport| vp.drag_call(&d, px, 1.0).unwrap().1["by"][0].as_f64().unwrap();
    assert!(f(&vp) > 0.0);
    vp.prevent_negative = false;
    assert!(f(&vp) < 0.0);
}

#[test]
fn shift_middle_drag_takes_the_axis_it_runs_along() {
    let mut c = Core::new();
    c.mel("polyCube;");
    let mut vp = view(Tool::Move);
    vp.update_frame(&mut c.s);
    let s = vp.screen();
    let o = s.project(DVec3::ZERO).unwrap();
    for i in 0..3 {
        let mut a = DVec3::ZERO;
        a[i] = 1.0;
        let delta = s.project(a).unwrap() - o;
        assert_eq!(vp.axis_along(DVec3::ZERO, delta), Handle::Axis(i));
    }
}

#[test]
fn custom_axes_from_a_face_an_edge_and_angles() {
    let mut c = Core::new();
    c.mel("polyCube; select pCube1.f[1];");
    let e = axis_orient::from_selection(&mut c.s, "face").unwrap();
    let f = axis_orient::frame_from_radians(e);
    assert!(f.z_axis.dot(DVec3::Y) > 0.999, "{f}");
    c.mel("select pCube1.e[0];");
    let f = axis_orient::frame_from_radians(axis_orient::from_selection(&mut c.s, "edge").unwrap());
    assert!(f.x_axis.dot(DVec3::X).abs() > 0.999, "{f}");
    let mut vp = view(Tool::Move);
    vp.axis_mode = "custom".into();
    vp.custom_frame = axis_orient::frame_from_radians([0.0, 0.0, std::f64::consts::FRAC_PI_2]);
    vp.update_frame(&mut c.s);
    assert!(vp.frame.x_axis.dot(DVec3::Y) > 0.999);
}

#[test]
fn rotation_axis_orientation_reads_the_rotate_axis() {
    let mut c = Core::new();
    c.mel("polyCube; setAttr pCube1.rotateAxisZ 90;");
    let mut vp = view(Tool::Move);
    vp.axis_mode = "rotationAxis".into();
    vp.update_frame(&mut c.s);
    assert!(vp.frame.x_axis.dot(DVec3::Y) > 0.999, "{}", vp.frame);
}

#[test]
fn soft_select_falloff_modes_are_kept() {
    let mut c = Core::new();
    for (i, name) in ["volume", "surface", "global", "object"].into_iter().enumerate() {
        c.mel(&format!("softSelect -e -ssf {i};"));
        assert_eq!(c.mel("softSelect -q -ssf;"), json!(i), "{name}");
        assert_eq!(c.s.run("softSelectCurve", json!({})).unwrap()["falloffMode"], name);
    }
    c.mel("softSelect -e -ssf 0;");
}
