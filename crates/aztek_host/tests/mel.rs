//! MEL-style command lines, Maya spellings included.

use aztek_host::Core;
use serde_json::{json, Value};

/// The playback range is process-wide, as in Maya: tests that set it take turns.
static RANGE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn mel(c: &mut Core, src: &str) -> Value {
    c.mel(src)
}

fn errors(c: &mut Core) -> Vec<String> {
    c.take_log().as_array().unwrap().iter().filter(|l| l["kind"] == 2).map(|l| l["text"].as_str().unwrap().to_string()).collect()
}

fn faces(c: &mut Core, m: &str) -> i64 {
    c.s.run("meshInfo", json!({"args": [m]})).unwrap()["faces"].as_i64().unwrap()
}

#[test]
fn maya_spellings_run() {
    let mut c = Core::new();
    assert_eq!(mel(&mut c, "polyCube -w 2 -h 3 -d 4 -sx 2;")[0], "pCube1");
    assert_eq!(faces(&mut c, "pCube1"), 10);
    mel(&mut c, "polyExtrudeFacet -ltz 0.5 -kft 1 pCube1.f[0];");
    assert!(errors(&mut c).is_empty());
    assert_eq!(faces(&mut c, "pCube1"), 14);
    mel(&mut c, "polyPlane -w 4 -h 2 -sx 3 -sy 2;");
    assert_eq!(faces(&mut c, "pPlane1"), 6);
    mel(&mut c, "move -r 1 2 3 pPlane1; select -r pPlane1;");
    let t = c.s.run("getAttr", json!({"args": ["pPlane1.translate"]})).unwrap();
    assert_eq!(t, json!([1.0, 2.0, 3.0]));
    mel(&mut c, "move 5 0 0 pPlane1;");
    assert_eq!(c.s.run("getAttr", json!({"args": ["pPlane1.translateX"]})).unwrap(), json!(5.0));
    mel(&mut c, "delete pCube1.f[0];");
    assert!(errors(&mut c).is_empty());
    assert_eq!(faces(&mut c, "pCube1"), 13);
    mel(&mut c, "select -cl; select -add pCube1; select -tgl pPlane1;");
    assert_eq!(c.s.run("ls", json!({"selection": true})).unwrap(), json!(["pCube1", "pPlane1"]));
    mel(&mut c, "nonsense -x;");
    assert_eq!(errors(&mut c).len(), 1);
}

#[test]
fn query_flags_and_keys() {
    let _g = RANGE_LOCK.lock().unwrap();
    let mut c = Core::new();
    mel(&mut c, "polyCube -w 2;");
    assert_eq!(mel(&mut c, "polyCube -q -w pCube1;"), json!(2.0));
    mel(&mut c, "polyCube -e -w 3 pCube1;");
    assert_eq!(mel(&mut c, "polyCube -q -w pCube1;"), json!(3.0));
    mel(&mut c, "playbackOptions -min 5 -max 50;");
    assert_eq!(mel(&mut c, "playbackOptions -q -min;"), json!(5));
    assert_eq!(mel(&mut c, "playbackOptions -q -max;"), json!(50));
    mel(&mut c, "currentTime 10; setKeyframe -at tx pCube1; currentTime 20; setKeyframe -at tx -v 4 pCube1;");
    assert_eq!(mel(&mut c, "currentTime -q;"), json!(20));
    assert_eq!(mel(&mut c, "keyframe -q -kc pCube1;"), json!(2));
    assert_eq!(mel(&mut c, "keyframe -at tx -q -tc pCube1;"), json!([10.0, 20.0]));
    assert!(errors(&mut c).is_empty());
}


#[test]
fn save_keeps_the_time_range() {
    let _g = RANGE_LOCK.lock().unwrap();
    let mut c = Core::new();
    mel(&mut c, "polyCube; playbackOptions -min 10 -max 40; currentTime 12;");
    let path = std::env::temp_dir().join("aztek_range_test.azk");
    c.save(path.to_str().unwrap()).unwrap();
    mel(&mut c, "playbackOptions -min 1 -max 120; currentTime 1;");
    c.open(path.to_str().unwrap()).unwrap();
    assert_eq!(mel(&mut c, "playbackOptions -q -min;"), json!(10));
    assert_eq!(mel(&mut c, "currentTime -q;"), json!(12));
    let warnings: Vec<String> = c.take_log().as_array().unwrap().iter().filter(|l| l["kind"] == 4 || l["kind"] == 2).map(|l| l["text"].as_str().unwrap().to_string()).collect();
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn joint_tool_chain() {
    let mut c = Core::new();
    mel(&mut c, "select -cl;");
    for p in [[-0.435, -0.333, 0.0], [-0.071, 0.213, 0.0], [0.475, 0.395, 0.0]] {
        aztek_host::view::place_joint(&mut c, glam::DVec3::from_array(p));
    }
    for j in ["joint1", "joint2", "joint3"] {
        eprintln!("{j} {} jo {}", mel(&mut c, &format!("xform -q -ws -t {j};")), mel(&mut c, &format!("getAttr {j}.jointOrient;")));
    }
    c.s.graph.evaluate();
    for it in aztek_host::viewport::rig_items(&c.s.graph) {
        let (a, b) = it.segs[0];
        eprintln!("{} {:?} {:?} n={}", it.node, a, b, it.segs.len());
    }
}

#[test]
fn rig_commands_take_maya_spellings() {
    let mut c = Core::new();
    c.mel("joint -p 2 0 0 -n L_arm; joint -p 4 1 0 -n L_hand; select -cl;");
    assert_eq!(c.mel("mirrorJoint -mirrorYZ -mirrorBehavior -searchReplace \"L_\" \"R_\" L_arm;"), json!(["R_arm", "R_hand"]));
    c.mel("joint -e -lx -90 90 -lsx on off L_arm;");
    assert_eq!(c.mel("joint -q -lx L_arm;"), json!([-90.0, 90.0]));
    assert_eq!(c.mel("joint -q -lsx L_arm;"), json!([true, false]));
    assert_eq!(c.mel("joint -q -rad L_arm;"), json!([1.0]));

    c.mel("spaceLocator -n a; spaceLocator -n b; polyCube;");
    c.mel("move -a 1 2 3 a;");
    assert_eq!(c.mel("pointConstraint -st x -st y a pCube1;"), json!(["pCube1_pointConstraint1"]));
    assert_eq!(c.mel("getAttr pCube1.translate;"), json!([0.0, 0.0, 3.0]));
    c.mel("polySphere; pointConstraint -sk x -sk y a pSphere1;");
    assert_eq!(c.mel("getAttr pSphere1.translate;"), json!([0.0, 0.0, 3.0]));
    c.mel("polyCone; parentConstraint -w 1 a b pCone1;");
    c.mel("parentConstraint -e -w 0.5 a pCone1;");
    assert_eq!(c.mel("parentConstraint -q -w a pCone1;"), json!(0.5));
    assert_eq!(c.mel("parentConstraint -q -tl pCone1;"), json!(["a", "b"]));
    assert_eq!(c.mel("parentConstraint -q -wal pCone1;"), json!(["aW0", "bW1"]));
    c.mel("parentConstraint -e -rm a pCone1;");
    assert_eq!(c.mel("parentConstraint -q -tl pCone1;"), json!(["b"]));

    assert_eq!(c.mel("ikHandle -sj L_arm -ee L_hand -sol ikSCsolver -n h;"), json!(["h", "effector1"]));
    assert_eq!(c.mel("ikHandle -q -sj h;"), json!("L_arm"));
    assert_eq!(c.mel("ikHandle -q -ee h;"), json!("effector1"));
    assert_eq!(c.mel("ikHandle -q -jl h;"), json!(["L_arm"]));
    assert_eq!(c.mel("ikHandle -q -sol h;"), json!("ikSCsolver"));
    c.mel("ikHandle -e -p 4 h;");
    assert_eq!(c.mel("ikHandle -q -p h;"), json!(4));
    c.mel("ikSystem -e -sol 0;");
    assert_eq!(c.mel("ikSystem -q -sol;"), json!(false));
}
