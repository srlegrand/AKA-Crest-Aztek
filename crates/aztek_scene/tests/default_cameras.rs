//! The four cameras every scene starts with, against what a new scene saved by Maya holds.

use aztek_scene::Session;
use serde_json::{json, Value};

fn run(s: &mut Session, name: &str, call: Value) -> Value {
    s.run(name, call).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn scene() -> Session {
    let mut s = Session::new();
    aztek_scene::cameras::make_defaults(&mut s.graph);
    s
}

fn numbers(v: Value) -> Vec<f64> {
    match v {
        Value::Array(a) => a.into_iter().flat_map(numbers).collect(),
        Value::Bool(b) => vec![b as i32 as f64],
        v => vec![v.as_f64().unwrap_or_else(|| panic!("{v}"))],
    }
}

#[test]
fn names_shapes_and_values_are_those_of_a_new_scene() {
    let mut s = scene();
    assert_eq!(run(&mut s, "ls", json!({"type": "camera"})), json!(["perspShape", "topShape", "frontShape", "sideShape"]));
    for (cam, t, r, ortho, coi, rnd, home) in [
        ("persp", [28.0, 21.0, 28.0], [-27.938352729602379, 45.0, 0.0], 0.0, 44.82186966202994, 1.0, "viewSet -p %camera"),
        ("top", [0.0, 1000.1, 0.0], [-90.0, 0.0, 0.0], 1.0, 1000.1, 0.0, "viewSet -t %camera"),
        ("front", [0.0, 0.0, 1000.1], [0.0, 0.0, 0.0], 1.0, 1000.1, 0.0, "viewSet -f %camera"),
        ("side", [1000.1, 0.0, 0.0], [0.0, 90.0, 0.0], 1.0, 1000.1, 0.0, "viewSet -s %camera"),
    ] {
        let mut num = |p: String| numbers(run(&mut s, "getAttr", json!({"args": [p]})));
        assert_eq!(num(format!("{cam}.t")), t, "{cam}");
        assert_eq!(num(format!("{cam}.r")), r, "{cam}");
        assert_eq!(num(format!("{cam}.v")), [0.0], "{cam} is hidden");
        let sh = format!("{cam}Shape");
        assert_eq!(num(format!("{sh}.o")), [ortho], "{sh}.o");
        assert_eq!(num(format!("{sh}.coi")), [coi], "{sh}.coi");
        assert_eq!(num(format!("{sh}.rnd")), [rnd], "{sh}.rnd");
        assert_eq!(num(format!("{sh}.ow")), [30.0], "{sh}.ow");
        assert_eq!(run(&mut s, "getAttr", json!({"args": [format!("{sh}.imn")]})), json!(cam));
        assert_eq!(run(&mut s, "getAttr", json!({"args": [format!("{sh}.den")]})), json!(format!("{cam}_depth")));
        assert_eq!(run(&mut s, "getAttr", json!({"args": [format!("{sh}.man")]})), json!(format!("{cam}_mask")));
        assert_eq!(run(&mut s, "getAttr", json!({"args": [format!("{sh}.hc")]})), json!(home));
    }
}

#[test]
fn startup_cameras_stay_until_a_script_says_otherwise() {
    let mut s = scene();
    assert_eq!(run(&mut s, "camera", json!({"args": ["persp"], "q": true, "sc": true, "queried": ["startupCamera"]})), json!(true));
    let e = s.run("delete", json!({"args": ["persp"]})).unwrap_err().to_string();
    assert!(e.contains("startup"), "{e}");
    assert!(s.run("delete", json!({"args": ["topShape"]})).is_err());
    run(&mut s, "camera", json!({"args": ["persp"], "e": true, "sc": false}));
    run(&mut s, "delete", json!({"args": ["persp"]}));
    assert_eq!(run(&mut s, "objExists", json!({"args": ["persp"]})), json!(false));
    run(&mut s, "newScene", json!({}));
    aztek_scene::cameras::make_defaults(&mut s.graph);
    assert_eq!(run(&mut s, "objExists", json!({"args": ["persp"]})), json!(true), "a new scene has it again");
}

#[test]
fn a_saved_scene_keeps_what_the_camera_was_set_to() {
    let mut s = scene();
    run(&mut s, "setAttr", json!({"args": ["perspShape.fl", 85.0]}));
    run(&mut s, "setAttr", json!({"args": ["perspShape.dfg", true]}));
    run(&mut s, "setAttr", json!({"args": ["persp.tx", 3.0]}));
    let (text, w) = aztek_scene::ma_file::write_text(&mut s, "a.ma", None);
    assert!(w.is_empty(), "{w:?}");
    assert!(text.contains("setAttr \".hc\" -type \"string\" \"viewSet -t %camera\""), "{text}");
    assert!(text.contains("setAttr \".rnd\" no"), "{text}");
    let mut t = scene();
    aztek_scene::ma_file::read_text(&mut t, &text, false);
    assert_eq!(numbers(run(&mut t, "getAttr", json!({"args": ["perspShape.fl"]}))), [85.0]);
    assert_eq!(numbers(run(&mut t, "getAttr", json!({"args": ["perspShape.dfg"]}))), [1.0]);
    assert_eq!(numbers(run(&mut t, "getAttr", json!({"args": ["persp.tx"]}))), [3.0]);
    assert_eq!(run(&mut t, "ls", json!({"type": "camera"})).as_array().unwrap().len(), 4, "still only the four");
}
