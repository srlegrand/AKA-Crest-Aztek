//! The rest of the Deform menu: lattice, softMod, deltaMush, tension, wrap, wire, and the general deformer commands.

use aztek_scene::Session;
use serde_json::{json, Value};

fn run(s: &mut Session, name: &str, call: Value) -> Value {
    s.run(name, call).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// World position of a mesh's point.
fn point(s: &mut Session, comp: &str) -> [f64; 3] {
    let v = run(s, "pointPosition", json!({"args": [comp]}));
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap(), v[2].as_f64().unwrap()]
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6)
}

fn names(v: &Value) -> Vec<&str> {
    v.as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect()
}

fn bounds(s: &mut Session, m: &str) -> (Vec<f64>, Vec<f64>) {
    let i = run(s, "meshInfo", json!({"args": [m]}));
    let v = |k: &str| i[k].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect::<Vec<f64>>();
    (v("worldBoundsMin"), v("worldBoundsMax"))
}

/// Saves and loads the scene in place.
fn round_trip(s: &mut Session) {
    let doc = aztek_scene::scene_file::serialize(&s.graph, false);
    let text = serde_json::to_string(&doc).unwrap();
    let doc: Value = serde_json::from_str(&text).unwrap();
    s.graph.clear();
    aztek_scene::scene_file::deserialize(&mut s.graph, &doc).unwrap();
    s.graph.evaluate();
}

// ---------------------------------------------------------------------------------------------------------------------
// Lattice

#[test]
fn lattice_makes_maya_nodes_and_changes_nothing_until_moved() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({}));
    let before = point(&mut s, "pCube1.vtx[0]");
    let r = run(&mut s, "lattice", json!({"args": ["pCube1"], "divisions": [2, 5, 2], "objectCentered": true}));
    assert_eq!(r, json!(["ffd1", "ffd1Lattice", "ffd1Base"]));
    for (node, ty) in [("ffd1", "ffd"), ("ffd1LatticeShape", "lattice"), ("ffd1BaseShape", "baseLattice"), ("ffd1Lattice", "transform")] {
        assert_eq!(run(&mut s, "nodeType", json!({"args": [node]})), json!(ty), "{node}");
    }
    assert!(near(point(&mut s, "pCube1.vtx[0]"), before));
    let h = run(&mut s, "listHistory", json!({"args": ["pCube1"]}));
    assert!(names(&h).contains(&"ffd1"), "{h}");
    assert_eq!(run(&mut s, "listHistory", json!({"args": ["pCube1"], "type": "ffd"})), json!(["ffd1"]));
    assert_eq!(run(&mut s, "listHistory", json!({"args": ["pCube1"], "type": "geometryFilter"})), json!(["ffd1"]));
    assert_eq!(run(&mut s, "ls", json!({"type": "geometryFilter"})), json!(["ffd1"]));
    for (attr, value) in [("localInfluenceS", 2.0), ("lit", 2.0), ("liu", 2.0), ("outsideLattice", 0.0), ("envelope", 1.0)] {
        assert_eq!(run(&mut s, "getAttr", json!({"args": [format!("ffd1.{attr}")]})).as_f64().unwrap(), value, "{attr}");
    }
    assert_eq!(run(&mut s, "lattice", json!({"args": ["ffd1"], "query": true, "queried": ["divisions"]})), json!([2, 5, 2]));
}

#[test]
fn lattice_points_are_components() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({}));
    run(&mut s, "lattice", json!({"args": ["pCube1"], "divisions": [2, 3, 2]}));
    run(&mut s, "select", json!({"args": ["ffd1Lattice.pt[1][2][0]"]}));
    assert_eq!(run(&mut s, "ls", json!({"selection": true})), json!(["ffd1Lattice.pt[1][2][0]"]));
    run(&mut s, "select", json!({"args": ["ffd1Lattice.pt[0:1][*][*]"]}));
    assert_eq!(run(&mut s, "ls", json!({"selection": true})), json!(["ffd1Lattice.pt[0:1][0:2][0:1]"]));
    let flat = run(&mut s, "ls", json!({"selection": true, "flatten": true}));
    assert_eq!(flat.as_array().unwrap().len(), 12);
    assert_eq!(flat[0], json!("ffd1Lattice.pt[0][0][0]"));
    assert_eq!(flat[1], json!("ffd1Lattice.pt[0][0][1]"));
    assert_eq!(flat[11], json!("ffd1Lattice.pt[1][2][1]"));
    let by_name = run(&mut s, "ls", json!({"args": ["ffd1Lattice.pt[1][*][0]"], "flatten": true}));
    assert_eq!(names(&by_name), ["ffd1Lattice.pt[1][0][0]", "ffd1Lattice.pt[1][1][0]", "ffd1Lattice.pt[1][2][0]"]);
    // Corner of the unit lattice, in world space.
    assert!(near(point(&mut s, "ffd1Lattice.pt[1][2][1]"), [0.5, 0.5, 0.5]));
    assert!(s.run("select", json!({"args": ["ffd1Lattice.pt[9][0][0]"]})).is_err());
}

#[test]
fn moving_lattice_points_moves_the_mesh_with_undo() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({}));
    run(&mut s, "lattice", json!({"args": ["pCube1"]}));
    // The top four corners of a 2 2 2 lattice carry the cube's top four points; the bottom stays.
    run(&mut s, "select", json!({"args": ["ffd1Lattice.pt[*][1][*]"]}));
    run(&mut s, "moveComponents", json!({"by": [0, 2, 0]}));
    let (lo, hi) = bounds(&mut s, "pCube1");
    assert!((hi[1] - 2.5).abs() < 1e-6 && (lo[1] + 0.5).abs() < 1e-6, "{lo:?} {hi:?}");
    assert!(s.undo());
    let (_, hi) = bounds(&mut s, "pCube1");
    assert!((hi[1] - 0.5).abs() < 1e-6, "{hi:?}");
    assert!(s.redo());
    let (_, hi) = bounds(&mut s, "pCube1");
    assert!((hi[1] - 2.5).abs() < 1e-6, "{hi:?}");
    run(&mut s, "setAttr", json!({"args": ["ffd1.envelope", 0.0]}));
    let (_, hi) = bounds(&mut s, "pCube1");
    assert!((hi[1] - 0.5).abs() < 1e-6, "{hi:?}");
    run(&mut s, "setAttr", json!({"args": ["ffd1.envelope", 0.5]}));
    let (_, hi) = bounds(&mut s, "pCube1");
    assert!((hi[1] - 1.5).abs() < 1e-6, "{hi:?}");
    run(&mut s, "setAttr", json!({"args": ["ffd1.envelope", 1.0]}));
    run(&mut s, "lattice", json!({"args": ["ffd1"], "edit": true, "resetLattice": true}));
    let (_, hi) = bounds(&mut s, "pCube1");
    assert!((hi[1] - 0.5).abs() < 1e-6, "{hi:?}");
}

#[test]
fn undoing_a_lattice_restores_the_chain() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({}));
    run(&mut s, "lattice", json!({"args": ["pCube1"]}));
    assert!(s.undo());
    assert!(!run(&mut s, "objExists", json!({"args": ["ffd1"]})).as_bool().unwrap());
    assert!(!run(&mut s, "objExists", json!({"args": ["ffd1Lattice"]})).as_bool().unwrap());
    let h = run(&mut s, "listHistory", json!({"args": ["pCube1"]}));
    assert_eq!(names(&h), ["pCubeShape1", "polyCube1"]);
    assert!(s.redo());
    assert!(names(&run(&mut s, "listHistory", json!({"args": ["pCube1"]}))).contains(&"ffd1"));
}

#[test]
fn lattice_transform_against_its_base_deforms() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({}));
    run(&mut s, "lattice", json!({"args": ["pCube1"], "name": "boxy"}));
    assert!(run(&mut s, "objExists", json!({"args": ["boxyLattice"]})).as_bool().unwrap());
    run(&mut s, "setAttr", json!({"args": ["boxyLattice.tx", 3.0]}));
    let (lo, hi) = bounds(&mut s, "pCube1");
    assert!((lo[0] - 2.5).abs() < 1e-6 && (hi[0] - 3.5).abs() < 1e-6, "{lo:?} {hi:?}");
    // Moving the base the same way puts it back.
    run(&mut s, "setAttr", json!({"args": ["boxyBase.tx", 3.0]}));
    let (lo, _) = bounds(&mut s, "pCube1");
    assert!((lo[0] + 0.5).abs() < 1e-6, "{lo:?}");
}

#[test]
fn outside_lattice_modes() {
    let mut s = Session::new();
    // A plane 4 across with points every 1, in a lattice 2 across: the middle is inside, the rim outside.
    run(&mut s, "polyPlane", json!({"width": 4, "depth": 4, "subdivisionsX": 4, "subdivisionsZ": 4}));
    run(&mut s, "lattice", json!({"args": ["pPlane1"], "objectCentered": false, "scale": [2, 2, 2]}));
    run(&mut s, "setAttr", json!({"args": ["ffd1Lattice.ty", 1.0]}));
    // A point inside moves with the lattice; a point well outside doesn't.
    let inside = "pPlane1.vtx[12]";
    let outside = "pPlane1.vtx[0]";
    assert!(near(point(&mut s, inside), [0.0, 1.0, 0.0]));
    assert!(point(&mut s, outside)[1].abs() < 1e-6);
    run(&mut s, "lattice", json!({"args": ["ffd1"], "edit": true, "outsideLattice": 1}));
    assert!((point(&mut s, outside)[1] - 1.0).abs() < 1e-6);
    run(&mut s, "lattice", json!({"args": ["ffd1"], "edit": true, "outsideLattice": 2, "outsideFalloffDist": 0.5}));
    // vtx 10 sits 1 cm past the box, half a lattice width: on the edge of a falloff of 0.5, half way along one of 1.
    assert!(point(&mut s, outside)[1].abs() < 1e-6);
    let past = "pPlane1.vtx[10]";
    assert!(point(&mut s, past)[1].abs() < 1e-6, "{:?}", point(&mut s, past));
    run(&mut s, "lattice", json!({"args": ["ffd1"], "edit": true, "outsideFalloffDist": 1.0}));
    let y = point(&mut s, past)[1];
    assert!((y - 0.5).abs() < 1e-6, "{y}");
}

#[test]
fn lattice_local_influence_keeps_straight_lattices_straight_and_reaches_further() {
    let mut s = Session::new();
    run(&mut s, "polyCylinder", json!({"height": 4, "subdivisionsHeight": 8}));
    let (lo0, hi0) = bounds(&mut s, "pCylinder1");
    run(&mut s, "lattice", json!({"args": ["pCylinder1"], "divisions": [2, 5, 2], "ldivisions": [2, 4, 2]}));
    assert_eq!(run(&mut s, "getAttr", json!({"args": ["ffd1.lit"]})).as_i64().unwrap(), 4);
    let (lo, hi) = bounds(&mut s, "pCylinder1");
    assert!((hi[1] - hi0[1]).abs() < 1e-6 && (lo[0] - lo0[0]).abs() < 1e-6, "{lo:?} {hi:?}");
    // Full local influence: the top control points pull everything some way along.
    run(&mut s, "select", json!({"args": ["ffd1Lattice.pt[*][4][*]"]}));
    run(&mut s, "moveComponents", json!({"by": [1, 0, 0]}));
    let mid = "pCylinder1.vtx[80]";
    let moved_wide = point(&mut s, mid)[0];
    run(&mut s, "setAttr", json!({"args": ["ffd1.lit", 2]}));
    let moved_narrow = point(&mut s, mid)[0];
    assert!(moved_wide > moved_narrow, "{moved_wide} {moved_narrow}");
}

#[test]
fn lattice_divisions_change_keeps_the_shape() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({}));
    run(&mut s, "lattice", json!({"args": ["pCube1"]}));
    run(&mut s, "select", json!({"args": ["ffd1Lattice.pt[*][1][*]"]}));
    run(&mut s, "moveComponents", json!({"by": [0, 1, 0]}));
    let (_, hi) = bounds(&mut s, "pCube1");
    run(&mut s, "setAttr", json!({"args": ["ffd1LatticeShape.tDivisions", 4]}));
    let (_, hi2) = bounds(&mut s, "pCube1");
    assert!((hi[1] - hi2[1]).abs() < 1e-6, "{hi:?} {hi2:?}");
    run(&mut s, "lattice", json!({"args": ["ffd1"], "edit": true, "divisions": [3, 3, 3]}));
    assert_eq!(run(&mut s, "lattice", json!({"args": ["ffd1Lattice"], "query": true, "queried": ["divisions"]})), json!([3, 3, 3]));
    let flat = run(&mut s, "ls", json!({"args": ["ffd1Lattice.pt[*][*][*]"], "flatten": true}));
    assert_eq!(flat.as_array().unwrap().len(), 27);
}

#[test]
fn lattice_freeze_mapping_and_remap() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({}));
    run(&mut s, "cluster", json!({"args": ["pCube1"]}));
    run(&mut s, "lattice", json!({"args": ["pCube1"], "freezeMapping": true}));
    assert_eq!(run(&mut s, "lattice", json!({"args": ["ffd1"], "query": true, "queried": ["freezeMapping"]})), json!(true));
    run(&mut s, "select", json!({"args": ["ffd1Lattice.pt[*][1][*]"]}));
    run(&mut s, "moveComponents", json!({"by": [0, 1, 0]}));
    let (_, hi) = bounds(&mut s, "pCube1");
    assert!((hi[1] - 1.5).abs() < 1e-6, "{hi:?}");
    // The cluster carries the points well clear of the lattice; frozen, they keep the top's offset.
    run(&mut s, "setAttr", json!({"args": ["cluster1Handle.tx", 5.0]}));
    let (lo, hi) = bounds(&mut s, "pCube1");
    assert!((lo[0] - 4.5).abs() < 1e-6 && (hi[1] - 1.5).abs() < 1e-6, "{lo:?} {hi:?}");
    // Mapped again out there, every point is outside the lattice and only the cluster moves it.
    run(&mut s, "lattice", json!({"args": ["ffd1"], "edit": true, "remap": true}));
    let (_, hi) = bounds(&mut s, "pCube1");
    assert!((hi[1] - 0.5).abs() < 1e-6, "{hi:?}");
}

#[test]
fn lattice_survives_save_and_load() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({}));
    run(&mut s, "lattice", json!({"args": ["pCube1"], "divisions": [2, 3, 2]}));
    run(&mut s, "select", json!({"args": ["ffd1Lattice.pt[*][2][*]"]}));
    run(&mut s, "moveComponents", json!({"by": [0, 2, 0]}));
    let (_, hi) = bounds(&mut s, "pCube1");
    round_trip(&mut s);
    let (_, hi2) = bounds(&mut s, "pCube1");
    assert!((hi[1] - hi2[1]).abs() < 1e-6 && (hi2[1] - 2.5).abs() < 1e-6, "{hi:?} {hi2:?}");
    assert!(near(point(&mut s, "ffd1Lattice.pt[0][2][0]"), [-0.5, 2.5, -0.5]));
    run(&mut s, "setAttr", json!({"args": ["ffd1.envelope", 0.0]}));
    let (_, hi) = bounds(&mut s, "pCube1");
    assert!((hi[1] - 0.5).abs() < 1e-6);
}

#[test]
fn deleting_the_lattice_or_the_ffd_takes_the_lot_and_restores_the_mesh() {
    for doomed in ["ffd1", "ffd1Lattice", "ffd1Base"] {
        let mut s = Session::new();
        run(&mut s, "polyCube", json!({}));
        run(&mut s, "lattice", json!({"args": ["pCube1"]}));
        run(&mut s, "setAttr", json!({"args": ["ffd1Lattice.tx", 3.0]}));
        run(&mut s, "delete", json!({"args": [doomed]}));
        for gone in ["ffd1", "ffd1Lattice", "ffd1Base"] {
            assert!(!run(&mut s, "objExists", json!({"args": [gone]})).as_bool().unwrap(), "{doomed}: {gone}");
        }
        let (lo, _) = bounds(&mut s, "pCube1");
        assert!((lo[0] + 0.5).abs() < 1e-6, "{doomed}: {lo:?}");
        assert_eq!(names(&run(&mut s, "listHistory", json!({"args": ["pCube1"]}))), ["pCubeShape1", "polyCube1"]);
        assert!(s.undo());
        let (lo, _) = bounds(&mut s, "pCube1");
        assert!((lo[0] - 2.5).abs() < 1e-6, "{doomed} undo: {lo:?}");
    }
}

#[test]
fn delete_history_bakes_the_lattice_and_removes_it() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({}));
    run(&mut s, "lattice", json!({"args": ["pCube1"]}));
    run(&mut s, "setAttr", json!({"args": ["ffd1Lattice.tx", 3.0]}));
    run(&mut s, "delete", json!({"args": ["pCube1"], "constructionHistory": true}));
    assert!(!run(&mut s, "objExists", json!({"args": ["ffd1Lattice"]})).as_bool().unwrap());
    let (lo, _) = bounds(&mut s, "pCube1");
    assert!((lo[0] - 2.5).abs() < 1e-6, "{lo:?}");
}

// ---------------------------------------------------------------------------------------------------------------------
// The general commands

#[test]
fn percent_sets_and_queries_weights() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({}));
    run(&mut s, "cluster", json!({"args": ["pCube1"]}));
    let (rest0, rest7) = (point(&mut s, "pCube1.vtx[0]"), point(&mut s, "pCube1.vtx[7]"));
    run(&mut s, "percent", json!({"args": ["cluster1", "pCube1.vtx[0:3]"], "value": 0.5}));
    assert_eq!(run(&mut s, "percent", json!({"args": ["cluster1", "pCube1.vtx[0]", "pCube1.vtx[7]"], "query": true, "queried": ["value"]})), json!([0.5, 1.0]));
    run(&mut s, "setAttr", json!({"args": ["cluster1Handle.ty", 2.0]}));
    assert!(near(point(&mut s, "pCube1.vtx[0]"), [rest0[0], rest0[1] + 1.0, rest0[2]]));
    assert!(near(point(&mut s, "pCube1.vtx[7]"), [rest7[0], rest7[1] + 2.0, rest7[2]]));
    run(&mut s, "percent", json!({"args": ["cluster1", "pCube1.vtx[0]"], "value": 0.5, "multiply": true}));
    assert_eq!(run(&mut s, "percent", json!({"args": ["cluster1", "pCube1.vtx[0]"], "query": true, "queried": ["value"]})), json!([0.25]));
    assert!(s.undo());
    assert_eq!(run(&mut s, "percent", json!({"args": ["cluster1", "pCube1.vtx[0]"], "query": true, "queried": ["value"]})), json!([0.5]));
    round_trip(&mut s);
    assert_eq!(run(&mut s, "percent", json!({"args": ["cluster1", "pCube1.vtx[0]"], "query": true, "queried": ["value"]})), json!([0.5]));
}

#[test]
fn percent_weighs_a_lattice() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({}));
    run(&mut s, "lattice", json!({"args": ["pCube1"]}));
    run(&mut s, "percent", json!({"args": ["ffd1", "pCube1.vtx[0]"], "value": 0.0}));
    let v0 = point(&mut s, "pCube1.vtx[0]");
    run(&mut s, "setAttr", json!({"args": ["ffd1Lattice.tx", 3.0]}));
    assert!(near(point(&mut s, "pCube1.vtx[0]"), v0));
    let v1 = point(&mut s, "pCube1.vtx[1]");
    assert!((v1[0] - 3.5).abs() < 1e-6, "{v1:?}");
}

#[test]
fn deformer_command_makes_queries_adds_and_removes() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({}));
    run(&mut s, "polySphere", json!({"radius": 0.4}));
    assert_eq!(run(&mut s, "deformer", json!({"args": ["pCube1"], "type": "ffd", "name": "box"})), json!(["box"]));
    assert_eq!(run(&mut s, "deformer", json!({"args": ["box"], "query": true, "geometry": true})), json!(["pCubeShape1"]));
    run(&mut s, "deformer", json!({"args": ["box"], "edit": true, "geometry": "pSphere1"}));
    assert_eq!(run(&mut s, "deformer", json!({"args": ["box"], "query": true, "geometry": true})), json!(["pCubeShape1", "pSphereShape1"]));
    assert_eq!(run(&mut s, "deformer", json!({"args": ["box"], "query": true, "geometryIndices": true})), json!([0, 1]));
    // Both meshes answer to the one lattice and the one envelope.
    run(&mut s, "setAttr", json!({"args": ["boxLattice.tx", 3.0]}));
    let moved = bounds(&mut s, "pSphere1");
    run(&mut s, "setAttr", json!({"args": ["box.envelope", 0.0]}));
    let rest = bounds(&mut s, "pSphere1");
    assert!((moved.0[0] - rest.0[0] - 3.0).abs() < 1e-6, "{moved:?} {rest:?}");
    let (lo, _) = bounds(&mut s, "pCube1");
    assert!((lo[0] + 0.5).abs() < 1e-6, "{lo:?}");
    run(&mut s, "setAttr", json!({"args": ["box.envelope", 1.0]}));
    let (lo, _) = bounds(&mut s, "pCube1");
    assert!((lo[0] - 2.5).abs() < 1e-6, "{lo:?}");
    run(&mut s, "deformer", json!({"args": ["box"], "edit": true, "remove": true, "geometry": "pCube1"}));
    assert_eq!(run(&mut s, "deformer", json!({"args": ["box"], "query": true, "geometry": true})), json!(["pSphereShape1"]));
    let (lo, _) = bounds(&mut s, "pCube1");
    assert!((lo[0] + 0.5).abs() < 1e-6, "{lo:?}");
    assert!(s.undo());
    let (lo, _) = bounds(&mut s, "pCube1");
    assert!((lo[0] - 2.5).abs() < 1e-6, "{lo:?}");
    assert!(s.run("deformer", json!({"args": ["pCube1"], "type": "noSuchThing"})).is_err());
}

#[test]
fn lattice_covers_several_meshes_as_one() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({}));
    run(&mut s, "polyCube", json!({}));
    run(&mut s, "setAttr", json!({"args": ["pCube2.tx", 2.0]}));
    let r = run(&mut s, "lattice", json!({"args": ["pCube1", "pCube2"]}));
    assert_eq!(r, json!(["ffd1", "ffd1Lattice", "ffd1Base"]));
    assert_eq!(run(&mut s, "lattice", json!({"args": ["ffd1"], "query": true, "geometry": true})), json!(["pCubeShape1", "pCubeShape2"]));
    run(&mut s, "setAttr", json!({"args": ["ffd1Lattice.ty", 1.0]}));
    assert!((bounds(&mut s, "pCube1").1[1] - 1.5).abs() < 1e-6);
    assert!((bounds(&mut s, "pCube2").1[1] - 1.5).abs() < 1e-6);
}

#[test]
fn before_and_front_of_chain_place_the_deformer() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({}));
    run(&mut s, "cluster", json!({"args": ["pCube1"], "name": "first"}));
    run(&mut s, "cluster", json!({"args": ["pCube1"], "name": "second"}));
    run(&mut s, "lattice", json!({"args": ["pCube1"], "before": true, "name": "mid"}));
    run(&mut s, "lattice", json!({"args": ["pCube1"], "frontOfChain": true, "name": "top"}));
    run(&mut s, "lattice", json!({"args": ["pCube1"], "after": true, "name": "end"}));
    let h = run(&mut s, "listHistory", json!({"args": ["pCube1"], "type": "geometryFilter"}));
    assert_eq!(names(&h), ["end", "second", "mid", "first", "top"]);
}

#[test]
fn reorder_swaps_and_the_chain_still_evaluates() {
    let mut s = Session::new();
    run(&mut s, "polyCube", json!({}));
    run(&mut s, "cluster", json!({"args": ["pCube1"]}));
    run(&mut s, "lattice", json!({"args": ["pCube1"]}));
    run(&mut s, "cluster", json!({"args": ["pCube1"], "name": "last"}));
    let order = |s: &mut Session| run(s, "listHistory", json!({"args": ["pCube1"], "type": "geometryFilter"}));
    assert_eq!(names(&order(&mut s)), ["last", "ffd1", "cluster1"]);
    // The lattice stretches X by moving a side; the cluster then carries the points. Order changes the answer.
    run(&mut s, "select", json!({"args": ["ffd1Lattice.pt[1][*][*]"]}));
    run(&mut s, "moveComponents", json!({"by": [1, 0, 0]}));
    run(&mut s, "setAttr", json!({"args": ["cluster1Handle.tx", 5.0]}));
    let (lo, hi) = bounds(&mut s, "pCube1");
    // Cluster first: the points leave the lattice, which only holds inside points.
    assert!((lo[0] - 4.5).abs() < 1e-6 && (hi[0] - 5.5).abs() < 1e-6, "{lo:?} {hi:?}");
    run(&mut s, "reorderDeformers", json!({"args": ["cluster1", "ffd1", "pCube1"]}));
    assert_eq!(names(&order(&mut s)), ["last", "cluster1", "ffd1"]);
    let (lo, hi) = bounds(&mut s, "pCube1");
    // Lattice first: stretched, then carried.
    assert!((lo[0] - 4.5).abs() < 1e-6 && (hi[0] - 6.5).abs() < 1e-6, "{lo:?} {hi:?}");
    // Across one between them, keeping it in place.
    run(&mut s, "reorderDeformers", json!({"args": ["last", "ffd1"]}));
    assert_eq!(names(&order(&mut s)), ["ffd1", "cluster1", "last"]);
    assert!(s.undo());
    assert_eq!(names(&order(&mut s)), ["last", "cluster1", "ffd1"]);
    round_trip(&mut s);
    assert_eq!(names(&order(&mut s)), ["last", "cluster1", "ffd1"]);
    let (_, hi) = bounds(&mut s, "pCube1");
    assert!((hi[0] - 6.5).abs() < 1e-6, "{hi:?}");
}
