//! Keyframes: setKeyframe, playback through currentTime, holds and autokey, keyframe queries and edits, cutKey.

use aztek_scene::Session;
use serde_json::{json, Value};

fn run(s: &mut Session, name: &str, call: Value) -> Value {
    s.run(name, call).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn tx(s: &mut Session) -> f64 {
    run(s, "getAttr", json!({"args": ["pCube1.translateX"]})).as_f64().unwrap()
}

#[test]
fn keys_animate_and_interpolate() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({}));
    run(&mut s, "currentTime", json!({"args": [1]}));
    let n = run(&mut s, "setKeyframe", json!({"args": ["pCube1"]}));
    assert_eq!(n, json!(10));
    run(&mut s, "currentTime", json!({"args": [11]}));
    run(&mut s, "setAttr", json!({"args": ["pCube1.translateX", 10.0]}));
    run(&mut s, "setKeyframe", json!({"args": ["pCube1"], "attribute": "translateX"}));
    run(&mut s, "currentTime", json!({"args": [1]}));
    assert!(tx(&mut s).abs() < 1e-9);
    run(&mut s, "currentTime", json!({"args": [6]}));
    assert!((tx(&mut s) - 5.0).abs() < 1e-6, "{}", tx(&mut s));
    run(&mut s, "currentTime", json!({"args": [20]}));
    assert!((tx(&mut s) - 10.0).abs() < 1e-9);
    let kc = run(&mut s, "keyframe", json!({"args": ["pCube1"], "attribute": "tx", "query": true, "queried": ["keyframeCount"]}));
    assert_eq!(kc, json!(2));
    let tc = run(&mut s, "keyframe", json!({"args": ["pCube1.tx"], "query": true, "queried": ["timeChange"]}));
    assert_eq!(tc, json!([1.0, 11.0]));
    assert_eq!(run(&mut s, "findKeyframe", json!({"args": ["pCube1"], "which": "next", "time": 1})), json!(11));
}

#[test]
fn unkeyed_changes_hold_until_time_moves() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({}));
    run(&mut s, "setKeyframe", json!({"args": ["pCube1"], "attribute": "tx", "time": 1, "value": 0}));
    run(&mut s, "setKeyframe", json!({"args": ["pCube1"], "attribute": "tx", "time": 10, "value": 4}));
    run(&mut s, "currentTime", json!({"args": [1]}));
    run(&mut s, "move", json!({"args": ["pCube1"], "to": [3, 0, 0]}));
    assert!((tx(&mut s) - 3.0).abs() < 1e-9);
    run(&mut s, "currentTime", json!({"args": [2]}));
    run(&mut s, "currentTime", json!({"args": [1]}));
    assert!(tx(&mut s).abs() < 1e-9);
    run(&mut s, "autoKeyframe", json!({"state": true}));
    run(&mut s, "move", json!({"args": ["pCube1"], "to": [2, 0, 0]}));
    run(&mut s, "currentTime", json!({"args": [2]}));
    run(&mut s, "currentTime", json!({"args": [1]}));
    assert!((tx(&mut s) - 2.0).abs() < 1e-9);
    run(&mut s, "autoKeyframe", json!({"state": false}));
}

#[test]
fn cut_key_and_delete_clean_up_curves() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({}));
    run(&mut s, "setKeyframe", json!({"args": ["pCube1"], "time": 1}));
    let curves = run(&mut s, "ls", json!({"type": "animCurveTL"}));
    assert_eq!(curves.as_array().unwrap().len(), 3);
    run(&mut s, "cutKey", json!({"args": ["pCube1"], "attribute": "translate"}));
    assert_eq!(run(&mut s, "ls", json!({"type": "animCurveTL"})).as_array().unwrap().len(), 0);
    run(&mut s, "delete", json!({"args": ["pCube1"]}));
    assert_eq!(run(&mut s, "ls", json!({"type": "animCurveTU"})).as_array().unwrap().len(), 0);
    assert_eq!(run(&mut s, "ls", json!({"type": "animCurveTA"})).as_array().unwrap().len(), 0);
}

#[test]
fn tangents_and_infinity() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({}));
    for (t, v) in [(1, 0.0), (10, 10.0), (20, 0.0)] {
        run(&mut s, "setKeyframe", json!({"args": ["pCube1"], "attribute": "ty", "time": t, "value": v}));
    }
    // Auto tangents are flat at the peak: nothing overshoots.
    let mut max: f64 = 0.0;
    for f in 1..=20 {
        run(&mut s, "currentTime", json!({"args": [f]}));
        max = max.max(run(&mut s, "getAttr", json!({"args": ["pCube1.ty"]})).as_f64().unwrap());
    }
    assert!((max - 10.0).abs() < 1e-9);
    run(&mut s, "keyTangent", json!({"args": ["pCube1"], "attribute": "ty", "inTangentType": "linear", "outTangentType": "linear"}));
    run(&mut s, "currentTime", json!({"args": [15]}));
    assert!((run(&mut s, "getAttr", json!({"args": ["pCube1.ty"]})).as_f64().unwrap() - 5.0).abs() < 1e-9);
    run(&mut s, "setAttr", json!({"args": ["pCube1_translateY.postInfinity", 3]}));
    run(&mut s, "currentTime", json!({"args": [29]}));
    assert!((run(&mut s, "getAttr", json!({"args": ["pCube1.ty"]})).as_f64().unwrap() - 10.0).abs() < 1e-9);
}
