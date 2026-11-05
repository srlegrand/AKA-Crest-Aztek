//! Rigging: joints and orientation, skinning, constraints, IK.

use aztek_scene::Session;
use serde_json::{json, Value};

fn run(s: &mut Session, name: &str, call: Value) -> Value {
    s.run(name, call).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn pos(s: &mut Session, j: &str) -> [f64; 3] {
    let v = run(s, "xform", json!({"args": [j], "query": true, "worldSpace": true, "translation": true}));
    let a = v.as_array().unwrap();
    [a[0].as_f64().unwrap(), a[1].as_f64().unwrap(), a[2].as_f64().unwrap()]
}

fn jpos(s: &mut Session, j: &str) -> [f64; 3] {
    let v = run(s, "joint", json!({"args": [j], "query": true}));
    let a = v.as_array().unwrap();
    [a[0].as_f64().unwrap(), a[1].as_f64().unwrap(), a[2].as_f64().unwrap()]
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-4)
}

fn chain(s: &mut Session) {
    run(s, "select", json!({"clear": true}));
    run(s, "joint", json!({"position": [0, 0, 0]}));
    run(s, "joint", json!({"position": [2, 0, -0.5]}));
    run(s, "joint", json!({"position": [4, 0, 0]}));
    run(s, "joint", json!({"args": ["joint1"], "edit": true, "orientJoint": "xyz", "secondaryAxisOrient": "yup", "children": true}));
}

#[test]
fn joints_keep_their_places_when_oriented() {
    let mut s = Session::new();
    chain(&mut s);
    assert!(near(jpos(&mut s, "joint2"), [2.0, 0.0, -0.5]), "{:?}", jpos(&mut s, "joint2"));
    assert!(near(jpos(&mut s, "joint3"), [4.0, 0.0, 0.0]));
    // joint1's X now points at joint2: rotating it about its own Y swings joint2 round.
    run(&mut s, "setAttr", json!({"args": ["joint1.rotateZ", 90.0]}));
    let p = jpos(&mut s, "joint2");
    assert!(p[1] > 2.0, "{p:?}");
}

#[test]
fn skin_follows_the_joints() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({"width": 4, "subdivisionsX": 4}));
    run(&mut s, "move", json!({"args": ["pCube1"], "to": [2, 0, 0]}));
    chain(&mut s);
    run(&mut s, "skinCluster", json!({"args": ["joint1", "pCube1"]}));
    let before = run(&mut s, "meshInfo", json!({"args": ["pCube1"]}))["boundsMax"][1].as_f64().unwrap();
    run(&mut s, "setAttr", json!({"args": ["joint1.rotateZ", 45.0]}));
    let after = run(&mut s, "meshInfo", json!({"args": ["pCube1"]}))["boundsMax"][1].as_f64().unwrap();
    assert!(after > before + 0.5, "{before} {after}");
    let inf = run(&mut s, "skinCluster", json!({"args": ["skinCluster1"], "query": true, "influence": true}));
    assert_eq!(inf.as_array().unwrap().len(), 3);
}

#[test]
fn constraints_drive_objects() {
    let mut s = Session::new();
    run(&mut s, "spaceLocator", json!({}));
    run(&mut s, "polyCube", json!({}));
    run(&mut s, "move", json!({"args": ["locator1"], "to": [3, 1, 0]}));
    run(&mut s, "pointConstraint", json!({"args": ["locator1", "pCube1"]}));
    assert!(near(pos(&mut s, "pCube1"), [3.0, 1.0, 0.0]), "{:?}", pos(&mut s, "pCube1"));
    run(&mut s, "move", json!({"args": ["locator1"], "to": [-1, 2, 5]}));
    assert!(near(pos(&mut s, "pCube1"), [-1.0, 2.0, 5.0]));
    run(&mut s, "polySphere", json!({}));
    run(&mut s, "move", json!({"args": ["pSphere1"], "to": [0, 0, 2]}));
    run(&mut s, "parentConstraint", json!({"args": ["locator1", "pSphere1"], "maintainOffset": true}));
    assert!(near(pos(&mut s, "pSphere1"), [0.0, 0.0, 2.0]));
    run(&mut s, "move", json!({"args": ["locator1"], "to": [0, 2, 5]}));
    assert!(near(pos(&mut s, "pSphere1"), [1.0, 0.0, -3.0 + 5.0]), "{:?}", pos(&mut s, "pSphere1"));
}

#[test]
fn ik_reaches_the_handle() {
    let mut s = Session::new();
    chain(&mut s);
    let h = run(&mut s, "ikHandle", json!({"startJoint": "joint1", "endEffector": "joint3"}));
    assert_eq!(h[0], "ikHandle1");
    run(&mut s, "move", json!({"args": ["ikHandle1"], "to": [3, 1, 0]}));
    let p = jpos(&mut s, "joint3");
    assert!(near(p, [3.0, 1.0, 0.0]), "{p:?}");
    // Bent toward the pole vector (-z, where joint2 started).
    assert!(jpos(&mut s, "joint2")[2] < 0.0);
    let base = jpos(&mut s, "joint1");
    assert!(near(base, [0.0, 0.0, 0.0]));
}

#[test]
fn rig_animation_and_uvs_survive_save_and_load() {
    use aztek_scene::scene_file;
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({"width": 4, "subdivisionsX": 4}));
    run(&mut s, "move", json!({"args": ["pCube1"], "to": [2, 0, 0]}));
    chain(&mut s);
    run(&mut s, "skinCluster", json!({"args": ["joint1", "pCube1"]}));
    run(&mut s, "polyAutoProjection", json!({"args": ["pCube1"]}));
    run(&mut s, "ikHandle", json!({"startJoint": "joint1", "endEffector": "joint3"}));
    run(&mut s, "setKeyframe", json!({"args": ["ikHandle1"], "attribute": "ty", "time": 1, "value": 0}));
    run(&mut s, "setKeyframe", json!({"args": ["ikHandle1"], "attribute": "ty", "time": 10, "value": 2}));
    run(&mut s, "currentTime", json!({"args": [10]}));
    let before = run(&mut s, "meshData", json!({"args": ["pCube1"]}));
    let text = serde_json::to_string(&scene_file::serialize(&s.graph, false)).unwrap();
    run(&mut s, "newScene", json!({}));
    scene_file::deserialize(&mut s.graph, &serde_json::from_str(&text).unwrap()).unwrap();
    run(&mut s, "currentTime", json!({"args": [10]}));
    let after = run(&mut s, "meshData", json!({"args": ["pCube1"]}));
    let pts = |v: &Value| v["points"].as_array().unwrap().iter().flat_map(|p| p.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect::<Vec<_>>()).collect::<Vec<f64>>();
    let (a, b) = (pts(&before), pts(&after));
    assert_eq!(a.len(), b.len());
    assert!(a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-6));
}
