//! Wiring real rigs use, read from .ma text laid out as Maya writes it: constraints to pivots and with driven weights, follicles,
//! deformed curves and surfaces, mirrored setups. Each of these was found wrong opening a production rig.

use aztek_scene::ma_file::read_text;
use aztek_scene::Session;
use serde_json::{json, Value};

const HEAD: &str = "//Maya ASCII 2020 scene\nrequires maya \"2020\";\ncurrentUnit -l centimeter -a degree -t film;\n";

fn load(body: &str) -> (Session, Vec<String>) {
    let mut s = Session::new();
    let w = read_text(&mut s, &format!("{HEAD}{body}"), false);
    s.graph.evaluate();
    (s, w)
}

fn get(s: &mut Session, plug: &str) -> Vec<f64> {
    match s.run("getAttr", json!({"args": [plug]})).unwrap_or_else(|e| panic!("{plug}: {e}")) {
        Value::Array(a) => a.iter().map(|x| x.as_f64().unwrap()).collect(),
        v => vec![v.as_f64().unwrap_or_else(|| panic!("{plug}: {v}"))],
    }
}

fn near(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
}

fn assert_near(got: Vec<f64>, want: &[f64], what: &str) {
    assert!(near(&got, want), "{what}: got {got:?}, want {want:?}");
}

/// The fourth column of a world matrix.
fn world_at(s: &mut Session, node: &str) -> Vec<f64> {
    get(s, &format!("{node}.worldMatrix"))[12..15].to_vec()
}

fn constraint(name: &str, parent: &str, target: &str, alias: &str, driven_outputs: bool) -> String {
    let mut t = format!(
        "createNode parentConstraint -n \"{name}\" -p \"{parent}\";\n\taddAttr -dcb 0 -ci true -k true -sn \"w0\" -ln \"{alias}\" -dv 1 -min 0 -at \"double\";\n\tsetAttr \".erp\" yes;\n"
    );
    t.push_str(&format!("connectAttr \"{target}.t\" \"{name}.tg[0].tt\";\nconnectAttr \"{target}.rp\" \"{name}.tg[0].trp\";\nconnectAttr \"{target}.pm\" \"{name}.tg[0].tpm\";\nconnectAttr \"{name}.w0\" \"{name}.tg[0].tw\";\n"));
    if driven_outputs {
        t.push_str(&format!("connectAttr \"{name}.ctx\" \"{parent}.tx\";\nconnectAttr \"{name}.cty\" \"{parent}.ty\";\nconnectAttr \"{name}.ctz\" \"{parent}.tz\";\nconnectAttr \"{name}.crx\" \"{parent}.rx\";\nconnectAttr \"{name}.cry\" \"{parent}.ry\";\nconnectAttr \"{name}.crz\" \"{parent}.rz\";\n"));
    }
    t
}

#[test]
fn constraints_go_to_the_targets_rotate_pivot() {
    let body = format!(
        "createNode transform -n \"tgt\";\n\tsetAttr \".rp\" -type \"double3\" 1 2 3 ;\n\tsetAttr \".sp\" -type \"double3\" 1 2 3 ;\ncreateNode transform -n \"obj\";\n{}",
        constraint("obj_parentConstraint1", "obj", "tgt", "tgtW0", true)
    );
    let (mut s, w) = load(&body);
    assert!(w.is_empty(), "{w:?}");
    assert_near(get(&mut s, "obj.translate"), &[1.0, 2.0, 3.0], "at the pivot");
}

#[test]
fn a_set_driven_key_on_a_target_weight_is_kept() {
    let body = format!(
        "createNode transform -n \"a\";\n\tsetAttr \".t\" -type \"double3\" 5 0 0 ;\ncreateNode transform -n \"sw\";\n\taddAttr -ci true -sn \"Follow\" -ln \"Follow\" -at \"double\";\n\tsetAttr -k on \".Follow\" 0;\ncreateNode transform -n \"obj\";\n\tsetAttr \".t\" -type \"double3\" 0 7 0 ;\n{}\tsetAttr \".rst\" -type \"double3\" 0 7 0 ;\ncreateNode animCurveUU -n \"w_curve\";\n\tsetAttr -s 2 \".ktv[0:1]\"  0 0 1 1;\nconnectAttr \"sw.Follow\" \"w_curve.i\";\nconnectAttr \"w_curve.o\" \"obj_parentConstraint1.w0\";\n",
        constraint("obj_parentConstraint1", "obj", "a", "aW0", true)
    );
    let (mut s, _) = load(&body);
    assert_near(get(&mut s, "obj.translate"), &[0.0, 7.0, 0.0], "weight 0: at rest");
    s.run("setAttr", json!({"args": ["sw.Follow", 1.0]})).unwrap();
    assert_near(get(&mut s, "obj.translate"), &[5.0, 0.0, 0.0], "weight 1: on the target");
}

#[test]
fn a_constraint_stays_on_its_target_under_a_mirrored_parent() {
    // Rotation part of mirror X is 180 about X with scale -1 on every axis, which is how the offset was worked out.
    let body = format!(
        "createNode transform -n \"mirror\";\n\tsetAttr \".s\" -type \"double3\" -1 1 1 ;\ncreateNode transform -n \"tgt\";\ncreateNode transform -n \"grp\" -p \"mirror\";\n{}\tsetAttr \".tg[0].tor\" -type \"double3\" 180 0 0 ;\n",
        constraint("grp_parentConstraint1", "grp", "tgt", "tgtW0", true)
    );
    let (mut s, _) = load(&body);
    assert_near(get(&mut s, "grp.rotate"), &[0.0, 0.0, 0.0], "rest under a mirror");
}

#[test]
fn aim_constraints_work_in_the_parents_frame() {
    // The same setup with and without a mirror gives the same rotate: the mirror cancels out.
    let aim = |parent: &str| {
        format!(
            "createNode transform -n \"holder\";\n\tsetAttr \".s\" -type \"double3\" {parent} ;\ncreateNode transform -n \"tgt\" -p \"holder\";\n\tsetAttr \".t\" -type \"double3\" 0 0 5 ;\ncreateNode transform -n \"obj\" -p \"holder\";\ncreateNode aimConstraint -n \"obj_aimConstraint1\" -p \"obj\";\n\taddAttr -dcb 0 -ci true -sn \"w0\" -ln \"tgtW0\" -dv 1 -at \"double\";\n\tsetAttr \".a\" -type \"double3\" 1 0 0 ;\n\tsetAttr \".u\" -type \"double3\" 0 1 0 ;\n\tsetAttr \".wu\" -type \"double3\" 0 1 0 ;\n\tsetAttr \".wut\" 3;\nconnectAttr \"tgt.t\" \"obj_aimConstraint1.tg[0].tt\";\nconnectAttr \"tgt.pm\" \"obj_aimConstraint1.tg[0].tpm\";\nconnectAttr \"obj_aimConstraint1.w0\" \"obj_aimConstraint1.tg[0].tw\";\nconnectAttr \"holder.pim\" \"obj_aimConstraint1.cpim\";\nconnectAttr \"obj_aimConstraint1.crx\" \"obj.rx\";\nconnectAttr \"obj_aimConstraint1.cry\" \"obj.ry\";\nconnectAttr \"obj_aimConstraint1.crz\" \"obj.rz\";\n"
        )
    };
    let (mut plain, _) = load(&aim("1 1 1"));
    let (mut mirrored, _) = load(&aim("-1 1 1"));
    let a = get(&mut plain, "obj.rotate");
    assert_near(get(&mut mirrored, "obj.rotate"), &a, "same under a mirror");
    assert!((a[1] + 90.0).abs() < 1e-3, "aims along Z: {a:?}");
}

#[test]
fn a_world_up_object_is_connected() {
    let body = "createNode transform -n \"tgt\";\n\tsetAttr \".t\" -type \"double3\" 5 0 0 ;\ncreateNode transform -n \"up\";\n\tsetAttr \".r\" -type \"double3\" 90 0 0 ;\ncreateNode transform -n \"obj\";\ncreateNode aimConstraint -n \"obj_aimConstraint1\" -p \"obj\";\n\taddAttr -dcb 0 -ci true -sn \"w0\" -ln \"tgtW0\" -dv 1 -at \"double\";\n\tsetAttr \".a\" -type \"double3\" 1 0 0 ;\n\tsetAttr \".u\" -type \"double3\" 0 1 0 ;\n\tsetAttr \".wu\" -type \"double3\" 0 1 0 ;\n\tsetAttr \".wut\" 2;\nconnectAttr \"tgt.t\" \"obj_aimConstraint1.tg[0].tt\";\nconnectAttr \"obj_aimConstraint1.w0\" \"obj_aimConstraint1.tg[0].tw\";\nconnectAttr \"up.wm\" \"obj_aimConstraint1.wum\";\nconnectAttr \"obj_aimConstraint1.crx\" \"obj.rx\";\nconnectAttr \"obj_aimConstraint1.cry\" \"obj.ry\";\nconnectAttr \"obj_aimConstraint1.crz\" \"obj.rz\";\n";
    let (mut s, _) = load(body);
    assert_near(get(&mut s, "obj.rotate"), &[90.0, 0.0, 0.0], "up follows the object's rotation");
}

const PLANE: &str = "createNode transform -n \"sheet\";\ncreateNode nurbsSurface -n \"sheetShape\" -p \"sheet\";\ncreateNode makeNurbPlane -n \"makeNurbPlane1\";\n\tsetAttr \".ax\" -type \"double3\" 0 1 0 ;\n\tsetAttr \".w\" 2;\nconnectAttr \"makeNurbPlane1.os\" \"sheetShape.cr\";\n";

#[test]
fn a_follicle_drives_its_own_transform_with_u_v_and_the_normal_as_axes() {
    let body = format!(
        "{PLANE}createNode transform -n \"fol\";\n\tsetAttr \".it\" no;\ncreateNode follicle -n \"folShape\" -p \"fol\";\n\tsetAttr \".pu\" 0.75;\n\tsetAttr \".pv\" 0.5;\nconnectAttr \"sheetShape.l\" \"folShape.is\";\nconnectAttr \"sheetShape.wm\" \"folShape.iwm\";\nconnectAttr \"folShape.ot\" \"fol.t\";\nconnectAttr \"folShape.or\" \"fol.r\";\n"
    );
    let (mut s, w) = load(&body);
    assert!(w.is_empty(), "{w:?}");
    assert_near(get(&mut s, "fol.translate"), &[0.5, 0.0, 0.0], "at the UV");
    let m = get(&mut s, "fol.worldMatrix");
    assert_near(m[0..3].to_vec(), &[1.0, 0.0, 0.0], "X along U");
    assert_near(m[8..11].to_vec(), &[0.0, 1.0, 0.0], "Z the normal");
    s.run("setAttr", json!({"args": ["sheet.ty", 4.0]})).unwrap();
    assert_near(get(&mut s, "fol.translate"), &[0.5, 4.0, 0.0], "follows the surface");
}

#[test]
fn a_skinned_curve_follows_its_joint_through_the_deformer_chain() {
    let cv = "\t\t1 3 0 no 3\n\t\t4 0 1 2 3\n\t\t4\n\t\t0 0 0\n\t\t1 0 0\n\t\t2 0 0\n\t\t3 0 0\n\t\t;\n";
    let body = format!(
        "createNode joint -n \"j\";\ncreateNode transform -n \"c\";\ncreateNode nurbsCurve -n \"cShape\" -p \"c\";\ncreateNode nurbsCurve -n \"cShapeOrig\" -p \"c\";\n\tsetAttr \".io\" yes;\n\tsetAttr \".cc\" -type \"nurbsCurve\" \n{cv}createNode skinCluster -n \"sk\";\n\tsetAttr -s 4 \".wl\";\n\tsetAttr \".wl[0:3].w\"\n\t\t1 0 1\n\t\t1 0 1\n\t\t1 0 1\n\t\t1 0 1;\n\tsetAttr \".pm[0]\" -type \"matrix\" 1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1;\n\tsetAttr \".gm\" -type \"matrix\" 1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1;\n\tsetAttr \".mi\" 1;\ncreateNode tweak -n \"tw\";\n\tsetAttr -s 1 \".pl[0].cp[1]\" -type \"double3\" 0 0.5 0;\nconnectAttr \"j.wm\" \"sk.ma[0]\";\nconnectAttr \"cShapeOrig.ws\" \"tw.ip[0].ig\";\nconnectAttr \"tw.og[0]\" \"sk.ip[0].ig\";\nconnectAttr \"sk.og[0]\" \"cShape.cr\";\ncreateNode pointOnCurveInfo -n \"poc\";\n\tsetAttr \".pr\" 1;\ncreateNode transform -n \"loc\";\nconnectAttr \"cShape.ws\" \"poc.ic\";\nconnectAttr \"poc.p\" \"loc.t\";\n"
    );
    let (mut s, w) = load(&body);
    assert!(w.iter().all(|x| !x.contains("cShape")), "{w:?}");
    assert_near(get(&mut s, "loc.translate"), &[1.0, 0.5, 0.0], "the tweak applies");
    s.run("setAttr", json!({"args": ["j.ty", 2.0]})).unwrap();
    assert_near(get(&mut s, "loc.translate"), &[1.0, 2.5, 0.0], "follows the joint");
}

#[test]
fn something_can_drive_a_curves_control_points() {
    let cv = "\t\t1 1 0 no 3\n\t\t2 0 1\n\t\t2\n\t\t0 0 0\n\t\t1 0 0\n\t\t;\n";
    let body = format!(
        "createNode transform -n \"c\";\ncreateNode nurbsCurve -n \"cShape\" -p \"c\";\n\tsetAttr \".cc\" -type \"nurbsCurve\" \n{cv}createNode transform -n \"l\";\n\tsetAttr \".t\" -type \"double3\" 3 3 3 ;\ncreateNode locator -n \"lShape\" -p \"l\";\nconnectAttr \"lShape.wp\" \"cShape.cp[1]\";\ncreateNode pointOnCurveInfo -n \"poc\";\n\tsetAttr \".pr\" 1;\ncreateNode transform -n \"at\";\nconnectAttr \"cShape.ws\" \"poc.ic\";\nconnectAttr \"poc.p\" \"at.t\";\n"
    );
    let (mut s, _) = load(&body);
    assert_near(get(&mut s, "at.translate"), &[3.0, 3.0, 3.0], "the CV goes where the locator is");
}

#[test]
fn a_transform_geometry_moves_a_curve_with_no_stored_points() {
    let body = "createNode transform -n \"c\";\ncreateNode nurbsCurve -n \"cShape\" -p \"c\";\ncreateNode makeNurbCircle -n \"mc\";\n\tsetAttr \".nr\" -type \"double3\" 0 1 0 ;\ncreateNode transformGeometry -n \"tg\";\n\tsetAttr \".txf\" -type \"matrix\" 2 0 0 0 0 2 0 0 0 0 2 0 0 5 0 1;\nconnectAttr \"mc.oc\" \"tg.ig\";\nconnectAttr \"tg.og\" \"cShape.cr\";\ncreateNode curveInfo -n \"ci\";\nconnectAttr \"cShape.ws\" \"ci.ic\";\n";
    let (mut s, w) = load(body);
    assert!(w.iter().all(|x| !x.contains("no stored")), "{w:?}");
    let len = get(&mut s, "ci.arcLength")[0];
    assert!((len - 4.0 * std::f64::consts::PI).abs() < 0.2, "radius 2: {len}");
}

const LEG: &str = "createNode joint -n \"a\" -p \"mirror\";\n\tsetAttr \".t\" -type \"double3\" 1 0 0 ;\n\tsetAttr \".jo\" -type \"double3\" 0 0 -90 ;\ncreateNode joint -n \"b\" -p \"a\";\n\tsetAttr \".t\" -type \"double3\" 2 0 0 ;\n\tsetAttr \".pa\" -type \"double3\" 0 0 0 ;\n\tsetAttr \".jo\" -type \"double3\" 0 -20 0 ;\ncreateNode joint -n \"c\" -p \"b\";\n\tsetAttr \".t\" -type \"double3\" 2 0 0 ;\ncreateNode ikEffector -n \"eff\" -p \"b\";\ncreateNode ikRPsolver -n \"ikRPsolver\";\nconnectAttr \"c.tx\" \"eff.tx\";\nconnectAttr \"a.msg\" \"h.hsj\";\nconnectAttr \"eff.hp\" \"h.hee\";\nconnectAttr \"ikRPsolver.msg\" \"h.hsv\";\n";

#[test]
fn a_handle_with_no_stored_translate_sits_at_its_parent() {
    let body = format!(
        "createNode transform -n \"mirror\";\ncreateNode transform -n \"ctl\";\n\tsetAttr \".t\" -type \"double3\" 1 -3.9 0.5 ;\ncreateNode ikHandle -n \"h\" -p \"ctl\";\n{LEG}"
    );
    let (mut s, w) = load(&body);
    assert!(w.is_empty(), "{w:?}");
    assert_near(world_at(&mut s, "h"), &[1.0, -3.9, 0.5], "a handle's translate left out of the file is zero");
}

#[test]
fn a_chain_under_a_mirror_matches_its_unmirrored_twin() {
    // The pole vector is held in the start joint's parent space, so a mirrored parent leaves the local pose alone.
    let rig = |scale: &str, x: f64| {
        format!(
            "createNode transform -n \"mirror\";\n\tsetAttr \".s\" -type \"double3\" {scale} ;\ncreateNode transform -n \"pole\";\n\tsetAttr \".t\" -type \"double3\" {} -2 5 ;\ncreateNode transform -n \"ctl\";\n\tsetAttr \".t\" -type \"double3\" {} -3.5 1 ;\ncreateNode ikHandle -n \"h\" -p \"ctl\";\ncreateNode poleVectorConstraint -n \"pv1\" -p \"h\";\n\taddAttr -dcb 0 -ci true -k true -sn \"w0\" -ln \"poleW0\" -dv 1 -min 0 -at \"double\";\nconnectAttr \"pole.t\" \"pv1.tg[0].tt\";\nconnectAttr \"pv1.w0\" \"pv1.tg[0].tw\";\nconnectAttr \"pv1.ctx\" \"h.pvx\";\nconnectAttr \"pv1.cty\" \"h.pvy\";\nconnectAttr \"pv1.ctz\" \"h.pvz\";\n{LEG}",
            x * 1.0,
            x * 1.0
        )
    };
    let (mut l, _) = load(&rig("1 1 1", 1.0));
    let (mut r, _) = load(&rig("-1 1 1", -1.0));
    assert_near(get(&mut r, "h.poleVector"), &get(&mut l, "h.poleVector"), "same in the parent's space");
    assert_near(get(&mut r, "a.rotate"), &get(&mut l, "a.rotate"), "start joint");
    assert_near(get(&mut r, "b.rotate"), &get(&mut l, "b.rotate"), "middle joint");
    let (lw, rw) = (world_at(&mut l, "c"), world_at(&mut r, "c"));
    assert_near(rw.clone(), &[-lw[0], lw[1], lw[2]], "the end is the mirror image");
}

#[test]
fn spline_ik_keeps_its_root_on_the_curve() {
    let cv = "\t\t3 1 0 no 3\n\t\t6 0 0 0 1 1 1\n\t\t4\n\t\t0 5 0\n\t\t0 6 0\n\t\t0 7 0\n\t\t0 8 0\n\t\t;\n";
    let body = format!(
        "createNode joint -n \"a\";\n\tsetAttr \".jo\" -type \"double3\" 0 0 90 ;\ncreateNode joint -n \"b\" -p \"a\";\n\tsetAttr \".t\" -type \"double3\" 3 0 0 ;\ncreateNode ikEffector -n \"eff\" -p \"a\";\ncreateNode transform -n \"c\";\ncreateNode nurbsCurve -n \"cShape\" -p \"c\";\n\tsetAttr \".cc\" -type \"nurbsCurve\" \n{cv}createNode ikSplineSolver -n \"ikSplineSolver\";\ncreateNode ikHandle -n \"h\";\n\tsetAttr \".roc\" yes;\nconnectAttr \"b.tx\" \"eff.tx\";\nconnectAttr \"a.msg\" \"h.hsj\";\nconnectAttr \"eff.hp\" \"h.hee\";\nconnectAttr \"ikSplineSolver.msg\" \"h.hsv\";\nconnectAttr \"cShape.ws\" \"h.ic\";\n"
    );
    let (mut s, w) = load(&body);
    assert!(w.is_empty(), "{w:?}");
    assert_near(world_at(&mut s, "a"), &[0.0, 5.0, 0.0], "the root on the curve's start");
    // Anything that dirties the chain runs the sync, which must not lose the setting.
    s.run("setAttr", json!({"args": ["c.ty", 1.0]})).unwrap();
    assert_near(world_at(&mut s, "a"), &[0.0, 6.0, 0.0], "still on it after a sync");
}

#[test]
fn utility_names_are_the_ones_maya_writes() {
    let body = "createNode transform -n \"o\";\ncreateNode angleBetween -n \"ab\";\n\tsetAttr \".v2\" -type \"double3\" 0 1 0 ;\ncreateNode pairBlend -n \"pb\";\n\tsetAttr \".w\" 0;\n\tsetAttr \".itx1\" 4;\nconnectAttr \"pb.otx\" \"o.tx\";\nconnectAttr \"ab.a\" \"o.ry\";\ncreateNode transform -n \"sum\";\ncreateNode transform -n \"x\";\n\tsetAttr \".t\" -type \"double3\" 2 3 0 ;\ncreateNode blendWeighted -n \"bw\";\nconnectAttr \"x.tx\" \"bw.i[0]\";\nconnectAttr \"x.ty\" \"bw.i[1]\";\nconnectAttr \"bw.o\" \"sum.tx\";\n";
    let (mut s, w) = load(body);
    assert!(w.iter().all(|x| !x.contains("->")), "{w:?}");
    assert_near(get(&mut s, "o.rotateY"), &[90.0], "angleBetween.a is the angle");
    assert_near(get(&mut s, "o.translateX"), &[4.0], "pairBlend's numbered children");
    assert_near(get(&mut s, "sum.translateX"), &[5.0], "blendWeighted comes through");
}
