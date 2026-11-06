//! Maya commands scripts lean on that have no Aztek counterpart of their own: polyEvaluate, listConnections,
//! objectType, polyListComponentConversion, filterExpand, polyCreateFacet, polyAverageVertex, polyMoveVertex.

use aztek_geo::Index;
use aztek_ops::{execute, fail, objects, ArgType, CallSource, CmdResult, CommandDef, Commands, FlagDef};
use glam::DVec3;
use serde_json::{json, Value};

use crate::components::{self, ComponentMode};
use crate::graph::SelectMode;
use crate::object_sets::Member;
use crate::plug::{NodeId, PlugRef};
use crate::session::Session;

type Cmd = CommandDef<Session>;

fn f(name: &str, short: &str, ty: ArgType, doc: &str) -> FlagDef {
    FlagDef::new(name, short, ty, Value::Null, doc)
}

fn kind_name(k: ComponentMode) -> &'static str {
    match k {
        ComponentMode::Vertex => "vtx",
        ComponentMode::Edge => "e",
        ComponentMode::Face => "f",
        ComponentMode::Object => "",
    }
}

fn ranges(ids: &[Index]) -> String {
    let mut v = ids.to_vec();
    v.sort_unstable();
    v.dedup();
    let mut parts = Vec::new();
    let mut i = 0;
    while i < v.len() {
        let mut j = i;
        while j + 1 < v.len() && v[j + 1] == v[j] + 1 {
            j += 1;
        }
        parts.push(if j > i { format!("{}:{}", v[i], v[j]) } else { v[i].to_string() });
        i = j + 1;
    }
    parts.join(",")
}

/// Component strings for ids: one per range, or one per id when flat.
fn strings(path: &str, kind: ComponentMode, ids: &[Index], flat: bool) -> Vec<String> {
    let k = kind_name(kind);
    if flat {
        let mut v = ids.to_vec();
        v.sort_unstable();
        v.dedup();
        return v.into_iter().map(|i| format!("{path}.{k}[{i}]")).collect();
    }
    ranges(ids).split(',').filter(|s| !s.is_empty()).map(|r| format!("{path}.{k}[{r}]")).collect()
}

/// The arguments (or the selection, components included) as members.
fn members(s: &Session, a: &aztek_ops::Args) -> CmdResult<Vec<Member>> {
    let g = &s.graph;
    let mut names = a.named();
    if names.is_empty() {
        names = components::component_strings(g, false);
        if names.is_empty() {
            names = g.selection().iter().map(|&i| g.path(i)).collect();
        }
    }
    crate::object_sets::parse(g, &names).map_err(aztek_ops::CommandError::new)
}

pub fn register(c: &mut Commands<Session>) {
    c.register(
        Cmd::new(
            "polyEvaluate",
            "Counts on meshes or components: -v vertices, -e edges, -f faces, -t triangles, -uv UVs, -s shells, -b bounding \
             box. With no flag, a dictionary of the counts.",
            |s, a| {
                let ms = members(s, a)?;
                let g = &mut s.graph;
                let mut tot = [0i64; 6];
                let mut lo = DVec3::splat(f64::MAX);
                let mut hi = DVec3::splat(f64::MIN);
                for m in &ms {
                    let Some(mesh) = components::as_mesh(g, m.node) else { continue };
                    let Some(cage) = crate::modifiers::cage(g, mesh) else { continue };
                    if m.kind != ComponentMode::Object {
                        let i = match m.kind {
                            ComponentMode::Vertex => 0,
                            ComponentMode::Edge => 1,
                            _ => 2,
                        };
                        tot[i] += m.ids.len() as i64;
                        continue;
                    }
                    tot[0] += (0..cage.point_capacity()).filter(|&p| cage.point_valid(p)).count() as i64;
                    tot[1] += (0..cage.edge_capacity()).filter(|&e| cage.edge_valid(e)).count() as i64;
                    let faces: Vec<Index> = (0..cage.face_capacity()).filter(|&f| cage.face_valid(f)).collect();
                    tot[2] += faces.len() as i64;
                    tot[3] += faces.iter().map(|&f| cage.face_size(f) as i64 - 2).sum::<i64>();
                    let t = crate::uv_ops::topo(&cage);
                    tot[4] += t.maps.len() as i64;
                    tot[5] += t.shells.len() as i64;
                    let w = crate::rig::world(g, crate::dag::transform_of(g, mesh));
                    for p in 0..cage.point_capacity() {
                        if cage.point_valid(p) {
                            let q = w.transform_point3(cage.position(p));
                            lo = lo.min(q);
                            hi = hi.max(q);
                        }
                    }
                }
                let pick = [("vertex", 0), ("edge", 1), ("face", 2), ("triangle", 3), ("uvcoord", 4), ("shell", 5)];
                for (flag, i) in pick {
                    if a.bool(flag) {
                        return Ok(json!(tot[i]));
                    }
                }
                if a.bool("boundingBox") {
                    return Ok(json!([[lo.x, hi.x], [lo.y, hi.y], [lo.z, hi.z]]));
                }
                Ok(json!({"vertex": tot[0], "edge": tot[1], "face": tot[2], "triangle": tot[3], "uvcoord": tot[4], "shell": tot[5]}))
            },
        )
        .positional("meshes or components (the selection when none)")
        .flag(f("vertex", "v", ArgType::Bool, "Vertex count."))
        .flag(f("edge", "e", ArgType::Bool, "Edge count."))
        .flag(f("face", "f", ArgType::Bool, "Face count."))
        .flag(f("triangle", "t", ArgType::Bool, "Triangle count."))
        .flag(f("uvcoord", "uv", ArgType::Bool, "UV count."))
        .flag(f("shell", "s", ArgType::Bool, "Shell count (UV shells)."))
        .flag(f("boundingBox", "b", ArgType::Bool, "World bounding box ((xmin, xmax), (ymin, ymax), (zmin, zmax))."))
        .undoable(false),
    );

    c.register(
        Cmd::new(
            "listConnections",
            "Nodes (or plugs with -p) connected to the objects or plugs: -s sources, -d destinations (both by default), \
             -type filters by node type, -c pairs each with the plug on this side.",
            |s, a| {
                let g = &s.graph;
                let src = a.get("source").and_then(Value::as_bool).unwrap_or(true);
                let dst = a.get("destination").and_then(Value::as_bool).unwrap_or(true);
                let plugs = a.bool("plugs");
                let pairs = a.bool("connections");
                let ty = a.string("type", "");
                let mut out: Vec<Value> = Vec::new();
                for name in objects(s, a) {
                    let (node, attr): (NodeId, Option<PlugRef>) = if name.contains('.') {
                        let p = g.plug(&name)?;
                        (p.node, Some(p))
                    } else {
                        match g.find(&name)? {
                            Some(n) => (n, None),
                            None => fail!("listConnections: no object named {name}"),
                        }
                    };
                    for (d, sp) in g.connections() {
                        let mut hits = Vec::new();
                        if dst && sp.node == node && attr.as_ref().is_none_or(|p| p.attr == sp.attr) {
                            hits.push((sp, d));
                        }
                        if src && d.node == node && attr.as_ref().is_none_or(|p| p.attr == d.attr) {
                            hits.push((d, sp));
                        }
                        for (mine, other) in hits {
                            let Some(on) = g.node(other.node) else { continue };
                            if !ty.is_empty() && !on.ty().is_a(&ty) {
                                continue;
                            }
                            if pairs {
                                out.push(json!(g.plug_name(&mine)));
                            }
                            let v = if plugs { json!(g.plug_name(&other)) } else { json!(g.path(other.node)) };
                            if pairs || !out.contains(&v) {
                                out.push(v);
                            }
                        }
                    }
                }
                Ok(Value::Array(out))
            },
        )
        .positional("objects or plugs")
        .flag(f("source", "s", ArgType::Bool, "Include upstream connections (default on)."))
        .flag(f("destination", "d", ArgType::Bool, "Include downstream connections (default on)."))
        .flag(f("plugs", "p", ArgType::Bool, "Return plugs rather than nodes."))
        .flag(f("connections", "c", ArgType::Bool, "Pair each with the plug on this side."))
        .flag(f("type", "t", ArgType::String, "Only nodes of this type."))
        .flag(f("skipConversionNodes", "scn", ArgType::Bool, "Accepted for Maya scripts."))
        .undoable(false),
    );

    c.register(
        Cmd::new("objectType", "The node type of an object (objectType -isType checks one).", |s, a| {
            let Some(name) = a.pos_str(0) else { fail!("objectType: name an object") };
            let g = &s.graph;
            let id = match g.find(name)? {
                Some(i) => i,
                None => fail!("objectType: no object named {name}"),
            };
            let t = g.node(id).unwrap().ty().name.clone();
            if let Some(is) = a.get("isType").and_then(Value::as_str) {
                return Ok(json!(t == is));
            }
            Ok(json!(t))
        })
        .positional("object")
        .flag(f("isType", "i", ArgType::String, "Whether it is of this type."))
        .undoable(false),
    );

    c.register(
        Cmd::new(
            "polyListComponentConversion",
            "Convert components without selecting them: -fv/-fe/-ff from, -tv/-te/-tf to (-in for only those fully inside). \
             Returns component strings.",
            |s, a| {
                let ms = members(s, a)?;
                let g = &mut s.graph;
                let to = if a.bool("toVertex") {
                    ComponentMode::Vertex
                } else if a.bool("toEdge") {
                    ComponentMode::Edge
                } else if a.bool("toFace") {
                    ComponentMode::Face
                } else {
                    fail!("polyListComponentConversion: give -tv, -te or -tf")
                };
                let inside = a.bool("internal");
                let mut out = Vec::new();
                for m in ms {
                    let Some(mesh) = components::as_mesh(g, m.node) else { continue };
                    let Some(cage) = crate::modifiers::cage(g, mesh) else { continue };
                    let path = components::display_path(g, mesh);
                    let ids: Vec<Index> = if m.kind == ComponentMode::Object {
                        let n = match to {
                            ComponentMode::Vertex => cage.point_capacity(),
                            ComponentMode::Edge => cage.edge_capacity(),
                            _ => cage.face_capacity(),
                        };
                        (0..n)
                            .filter(|&i| match to {
                                ComponentMode::Vertex => cage.point_valid(i),
                                ComponentMode::Edge => cage.edge_valid(i),
                                _ => cage.face_valid(i),
                            })
                            .collect()
                    } else {
                        components::convert(&cage, m.kind, &m.ids, to, inside)
                    };
                    out.extend(strings(&path, to, &ids, false));
                }
                Ok(json!(out))
            },
        )
        .positional("components or meshes (the selection when none)")
        .flag(f("fromVertex", "fv", ArgType::Bool, "From vertices (read from the arguments here)."))
        .flag(f("fromEdge", "fe", ArgType::Bool, "From edges."))
        .flag(f("fromFace", "ff", ArgType::Bool, "From faces."))
        .flag(f("toVertex", "tv", ArgType::Bool, "To vertices."))
        .flag(f("toEdge", "te", ArgType::Bool, "To edges."))
        .flag(f("toFace", "tf", ArgType::Bool, "To faces."))
        .flag(f("internal", "in", ArgType::Bool, "Only components wholly inside."))
        .undoable(false),
    );

    c.register(
        Cmd::new(
            "filterExpand",
            "The selection (or arguments) flattened and filtered by selection mask: -sm 31 vertices, 32 edges, 34 faces, 12 \
             meshes. Returns None when nothing matches, as Maya does.",
            |s, a| {
                let ms = members(s, a)?;
                let g = &s.graph;
                let masks: Vec<i64> = match a.get("selectionMask") {
                    Some(Value::Array(v)) => v.iter().filter_map(Value::as_i64).collect(),
                    Some(v) => v.as_i64().into_iter().collect(),
                    None => vec![31, 32, 34, 12],
                };
                let mut out = Vec::new();
                for m in ms {
                    let wanted = match m.kind {
                        ComponentMode::Vertex => 31,
                        ComponentMode::Edge => 32,
                        ComponentMode::Face => 34,
                        ComponentMode::Object => 12,
                    };
                    if !masks.contains(&wanted) {
                        continue;
                    }
                    if m.kind == ComponentMode::Object {
                        if components::as_mesh(g, m.node).is_some() {
                            out.push(g.path(crate::dag::transform_of(g, m.node)));
                        }
                        continue;
                    }
                    let path = components::display_path(g, m.node);
                    out.extend(strings(&path, m.kind, &m.ids, a.get("expand").and_then(Value::as_bool).unwrap_or(true)));
                }
                Ok(if out.is_empty() { Value::Null } else { json!(out) })
            },
        )
        .positional("items (the selection when none)")
        .flag(f("selectionMask", "sm", ArgType::Any, "Mask number(s)."))
        .flag(f("expand", "ex", ArgType::Bool, "One item per component (default on)."))
        .undoable(false),
    );

    c.register(
        Cmd::new(
            "polyCreateFacet",
            "Create a mesh of one face through the points given with -p (repeat it, or a list). Returns [transform].",
            |s, a| {
                let pts: Vec<Value> = match a.get("point") {
                    Some(Value::Array(v)) if v.first().is_some_and(Value::is_array) => v.clone(),
                    Some(Value::Array(v)) => v.chunks(3).map(|c| json!(c)).collect(),
                    _ => fail!("polyCreateFacet: give the points with -p"),
                };
                if pts.len() < 3 {
                    fail!("polyCreateFacet: a face needs three points");
                }
                let faces = json!([(0..pts.len()).collect::<Vec<_>>()]);
                let r = execute(s, "createMesh", &json!({"points": pts, "faces": faces}), CallSource::Script)?;
                Ok(json!([r]))
            },
        )
        .flag(f("point", "p", ArgType::List, "The face's points, in order.").multi(3))
        .flag(f("constructionHistory", "ch", ArgType::Bool, "Accepted for Maya scripts.")),
    );

    c.register(
        Cmd::new(
            "polyAverageVertex",
            "Average Vertices: each selected vertex (or every vertex) moves toward the middle of its neighbours, -i times.",
            |s, a| {
                let ms = members(s, a)?;
                let iterations = a.int("iterations", 1).max(1);
                s.graph.open_chunk("Average Vertices");
                let r = (|| -> CmdResult<()> {
                    for m in ms {
                        let g = &mut s.graph;
                        let Some(mesh) = components::as_mesh(g, m.node) else { continue };
                        let Some(cage) = crate::modifiers::cage(g, mesh) else { continue };
                        let pts: Vec<Index> = if m.kind == ComponentMode::Object {
                            (0..cage.point_capacity()).filter(|&p| cage.point_valid(p)).collect()
                        } else {
                            components::convert(&cage, m.kind, &m.ids, ComponentMode::Vertex, false)
                        };
                        let mut pos: Vec<DVec3> = (0..cage.point_capacity()).map(|p| if cage.point_valid(p) { cage.position(p) } else { DVec3::ZERO }).collect();
                        for _ in 0..iterations {
                            let prev = pos.clone();
                            for &p in &pts {
                                let e0 = cage.point_edge(p);
                                if e0 == aztek_geo::INVALID {
                                    continue;
                                }
                                let mut sum = DVec3::ZERO;
                                let mut n = 0.0;
                                let mut e = e0;
                                loop {
                                    sum += prev[cage.edge_other_point(e, p) as usize];
                                    n += 1.0;
                                    e = cage.edge_next_around(e, p);
                                    if e == e0 || n > 64.0 {
                                        break;
                                    }
                                }
                                pos[p as usize] = prev[p as usize] * 0.5 + sum / n * 0.5;
                            }
                        }
                        let mut moved = (*cage).clone();
                        for &p in &pts {
                            moved.set_position(p, pos[p as usize]);
                        }
                        match crate::history::tweak_node(g, mesh)? {
                            Some(tw) => crate::history::set_tweak(g, tw, &moved)?,
                            None => {
                                let path = components::display_path(g, mesh);
                                let all: Vec<Value> = (0..moved.point_capacity()).filter(|&p| moved.point_valid(p)).map(|p| {
                                    let q = moved.position(p);
                                    json!([q.x, q.y, q.z])
                                }).collect();
                                execute(s, "setMeshPoints", &json!({"args": [path], "points": all}), CallSource::Script)?;
                            }
                        }
                    }
                    Ok(())
                })();
                s.graph.close_chunk();
                r?;
                Ok(Value::Null)
            },
        )
        .positional("vertices or meshes (the selection when none)")
        .flag(f("iterations", "i", ArgType::Int, "Passes (1).")),
    );

    c.register(
        Cmd::new(
            "polyMoveVertex",
            "Move vertices by -t (translate) in world space, as Maya's polyMoveVertex.",
            |s, a| {
                let ms = members(s, a)?;
                let t = a.vec3("translate").unwrap_or([0.0; 3]);
                let mut items = Vec::new();
                for m in ms {
                    let path = components::display_path(&s.graph, m.node);
                    if m.kind == ComponentMode::Object {
                        items.push(format!("{path}.vtx[*]"));
                    } else {
                        items.extend(strings(&path, m.kind, &m.ids, false));
                    }
                }
                execute(s, "select", &json!({"args": items}), CallSource::Script)?;
                execute(s, "moveComponents", &json!({"by": t}), CallSource::Script)
            },
        )
        .positional("vertices")
        .flag(f("translate", "t", ArgType::Vec3, "World offset.")),
    );
    let _ = SelectMode::Replace;

    c.register(
        Cmd::new(
            "pointPosition",
            "The world position of a point: a vertex, a joint or transform (its pivot), or a locator. -l for local space.",
            |s, a| {
                let Some(n) = a.named().into_iter().next() else { fail!("pointPosition: name a point") };
                let g = &mut s.graph;
                g.evaluate();
                let local = a.bool("local");
                if components::is_component_string(&n) {
                    let targets = crate::skin::points_named(g, &[n.clone()]);
                    let Some((mesh, ids)) = targets.into_iter().next() else { fail!("pointPosition: no point {n}") };
                    let Some(&p) = ids.first() else { fail!("pointPosition: no point {n}") };
                    let Some(m) = components::mesh_of(g, mesh) else { fail!("pointPosition: no point {n}") };
                    let w = crate::rig::world(g, crate::dag::transform_of(g, mesh));
                    let q = if local { m.position(p) } else { w.transform_point3(m.position(p)) };
                    return Ok(json!([q.x, q.y, q.z]));
                }
                let id = crate::rig::find(g, &n, "pointPosition")?;
                let q = crate::rig::world(g, crate::dag::transform_of(g, id)).w_axis.truncate();
                Ok(json!([q.x, q.y, q.z]))
            },
        )
        .undoable(false)
        .positional("a point")
        .flag(f("world", "w", ArgType::Bool, "World space (the default)."))
        .flag(f("local", "l", ArgType::Bool, "Object space.")),
    );
}

/// xform on points: pCube1.vtx[0:3]. A query returns x, y, z per point, world space with -ws; a set moves them all to the
/// position, or by it with -r.
pub(crate) fn xform_components(s: &mut Session, a: &aztek_ops::Args, named: &[String]) -> CmdResult {
    use glam::DVec3;
    let g = &mut s.graph;
    g.evaluate();
    let targets = crate::skin::points_named(g, named);
    let ws = a.bool("worldSpace");
    if a.bool("query") {
        let mut out = Vec::new();
        for (mesh, ids) in &targets {
            let Some(m) = crate::components::mesh_of(g, *mesh) else { continue };
            let w = crate::rig::world(g, crate::dag::transform_of(g, *mesh));
            for &p in ids {
                if !m.point_valid(p) {
                    continue;
                }
                let q = if ws { w.transform_point3(m.position(p)) } else { m.position(p) };
                out.extend([json!(q.x), json!(q.y), json!(q.z)]);
            }
        }
        return Ok(Value::Array(out));
    }
    let Some(t) = a.vec3("translation").map(DVec3::from_array) else { fail!("xform: on components only -t is available") };
    let rel = a.bool("relative");
    for (mesh, ids) in targets {
        let Some(m) = crate::modifiers::cage(g, mesh) else { continue };
        let w = crate::rig::world(g, crate::dag::transform_of(g, mesh));
        let inv = w.inverse();
        let moves: Vec<(u32, DVec3)> = ids
            .into_iter()
            .filter(|&p| m.point_valid(p))
            .map(|p| {
                let at = if rel {
                    if ws { m.position(p) + inv.transform_vector3(t) } else { m.position(p) + t }
                } else if ws {
                    inv.transform_point3(t)
                } else {
                    t
                };
                (p, at)
            })
            .collect();
        crate::mesh_edit::transform::set_points(g, mesh, &moves)?;
    }
    Ok(Value::Null)
}
