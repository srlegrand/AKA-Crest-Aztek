# Aztek to Rust: porting notes

Target stack: Rust core, `ash` (Vulkan) viewport, `egui` UI. Order agreed: scene graph first.

## Ported

Milestone 1 (scene graph) and milestone 2 (all of `src/geo`).

| C++ | Rust | Notes |
|---|---|---|
| `src/ops/Undo.{h,cpp}` | `crates/aztek_undo` | `UndoStack<C>`, generic over the context actions edit. Chunks, cancel, rollback, amend, reopen, jump, limits, byte budget with shared buffers counted once, state ids. |
| `src/scene/Plug.{h,cpp}` | `aztek_scene::plug` | `PlugValue` enum, `AttrDef`, `PlugRef` (same ordering), flags, JSON conversion, attr def JSON, data kind registry. |
| `src/scene/Node.{h,cpp}` | `aztek_scene::node` | `NodeType`, type registry with owners and file aliases, `Node`, `NodeBehavior` (the virtuals), `PlaybackExtra`. |
| `src/scene/Naming.{h,cpp}` | `aztek_scene::naming` | Case styles, underscore rules, `NamingPolicy`. |
| `src/scene/Transform.{h,cpp}` | `aztek_scene::transform` | Transform and locator types, Maya pivot model, Euler conversion for all six orders, `euler_near`, `set_local_matrix`, `effectively_visible` (with the inherit visibility pref). |
| `src/scene/Graph.{h,cpp}` | `aztek_scene::graph` | All of it: naming and paths, hierarchy, plugs, locks, driven-set hooks and held values, connections, arrays and grouped arrays, user attrs (add/edit/move/delete), dirty propagation, Kahn-level parallel evaluation, `evaluate_upstream`, `DeferMeshes`, display turns, cached playback, selection, listeners, batches. |
| `src/ops/Command.{h,cpp}` | `crates/aztek_ops` | Command registry, flag normalising and clamping, nested execute, undo chunks, execute listeners and gate, Python echo. The registry lives in the session instead of a global. |
| `src/scene/SceneCommands.cpp` | `aztek_scene::commands` | All 25 commands in that file, plus the undo commands from `ops::registerBuiltins`. `aztek_scene::Session` is the context they run against. |
| `src/scene/TransformCommands.cpp`, `TransformOps.{h,cpp}` | `aztek_scene::commands`, `aztek_scene::transform_ops` | move, rotate, scale, setPivot, resetPivot, zeroPivotRotation, zeroPivot, matchPivot, pivot. `orientPivot` waits for meshes; `bakeIntoGeometry` is in `object_ops`. |
| `src/geo/Mesh.{h,cpp}`, `Attribute.{h,cpp}` | `crates/aztek_geo` | The whole mesh: BMesh topology, Euler operators, compact, append, validate, the bulk polygon-soup builder. Vector attributes are `[f32; N]` arrays instead of glm vectors. |
| `src/geo/Topology.{h,cpp}` | `aztek_geo::topology` | Edge loops and rings, boundary loops, edge chains, face components, region boundaries, element conversions. |
| `src/geo/Primitives.{h,cpp}` | `aztek_geo::primitives` | All eleven primitives with their options, cap fills, partial sweeps, UV layout, hard edges and seams. Results match the C++ to the bit: glm's `mix` is kept in its own form (`aztek_geo::mix`). |
| `src/geo/Normals.{h,cpp}` | `aztek_geo::normals` | Face, point, split and weighted normals, `CornerNormalPlan`, hard edges by angle, MikkTSpace tangents (single call and chunked parallel, same result). MikkTSpace itself is the `mikktspace` crate, a translation of the reference C. |
| `src/geo/Triangulate.{h,cpp}` | `aztek_geo::triangulate` | Quad splitting with mirror-stable ties, ear clipping, constrained Delaunay flips, shared-diagonal guards. |
| `src/geo/RenderMesh.{h,cpp}` | `aztek_geo::render_mesh` | Per-material sections, split corners, the vertex layout the renderer uploads. |
| `src/geo/Bvh.{h,cpp}` | `aztek_geo::bvh` | Build, refit, raycast, occlusion, closest point, overlap queries, the flat export, barycentric interpolation. Queries return `Option` instead of a bool and an out parameter. |
| `src/geo/ColorSets.{h,cpp}` | `aztek_geo::colors` | Colour sets, half-precision rounding (the `half` crate), settle and adopt. |
| `src/geo/ObjIO`, `PlyIO`, `StlIO`, `IoCommon.h`, `PointWeld` | `aztek_geo::{obj, ply, stl, io_common, point_weld}` | Readers and writers with every option. `std::to_chars`/`from_chars` are emulated so numbers print and parse as the C++ does. File headers say Aztek. |
| `src/geo/Symmetry.{h,cpp}`, `SymmetryRadial.cpp` | `aztek_geo::symmetry` | Planes, radial symmetry, topological and persistent maps, enforce, symmetrize, snap. |
| `src/geo/FoldEdges.{h,cpp}` | `aztek_geo::fold_edges` | Fold angles, convexity, edge matching. |
| `src/geo/Ops.h`, `OpsBasic`, `OpsExtrude`, `OpsBevel`, `OpsLoop`, `OpsKnife`, `OpsGridFill`, `Subdivide`, `SplitNgons`, `Bridge`, `OpsCommon.h` | `aztek_geo::ops` | Every mesh op: delete, extrude, inset, bevel, edge loops and rings, knife, grid fill, bridge, merge, collapse, dissolve, poke, spin, fill, flip, conform, triangulate, quadrangulate, mirror, separate, combine, symmetrize, subdivide with maps and stencils, n-gon splitting. |
| `src/geo/Reduce.{h,cpp}` | `aztek_geo::reduce` | Quadric edge-collapse reduction with wedges, feature lines, symmetry and quads. |
| `src/geo/Conform.{h,cpp}` | `aztek_geo::conform` | Shape selections, straighten, circularize, rectangularize, spherize, cylinderize, flatten, align, conform to mesh, interior follow. |
| `src/geo/LoopTools.{h,cpp}` | `aztek_geo::loop_tools` | Offset edge loop, edit edge flow, flow selections. |
| `src/geo/CrossSection.{h,cpp}` | `aztek_geo::cross_section` | Planar cuts, chains, section centres. |
| `src/geo/BoundingShapes.{h,cpp}` | `aztek_geo::bounding` | Minimum-volume box, minimum sphere, cylinder and capsule fits, convex hull mesh. |
| `src/geo/MultiBox.{h,cpp}` | `aztek_geo::multibox` | Voxelized box decomposition. |
| `src/geo/Heightfield.{h,cpp}` | `aztek_geo::heightfield` | Grids, sampling, mesh conversion both ways, resampling. |
| `src/geo/ComponentPaths.{h,cpp}` | `aztek_geo::component_paths` | Face loops, point/edge/face paths, contained faces. |
| `src/geo/ArrayMesh.{h,cpp}`, `ArrayTransform.h` | `aztek_geo::array` | Layout maths and the welded array mesh. |
| `src/geo/SmartJoin.{h,cpp}` | `aztek_geo::smart_join` | Limb to body joins with confidence scoring. |
| `src/geo/Lasso.h` | `aztek_geo::lasso` | Screen-space lasso tests. |
| `src/scene/MeshData.{h,cpp}` | `aztek_scene::mesh_data` | Mesh to and from the scene document. Mesh plugs now carry `aztek_geo::Mesh`; the placeholder trait is gone. The packed form waits for the binary scene file. |
| `src/io/SceneFile.cpp` + `References.cpp::instantiate` | `aztek_scene::scene_file` | The graph part of the document: nodes, parents, stored values, user attrs, elements, locks, connections, selection, time. JSON only. |
| `src/core/PrefStore`, `Paths`, `Workspace`, `MainQueue` | `crates/aztek_core` | Layered prefs (defaults, studio folders with locks and an offline cache, user, workspace), the IO thread, contributed defaults; path storing and resolving; the workspace (`.aztek/`); the main-thread queue. Config lives in `Aztek` (`AZTEK_CONFIG_DIR`, `AZTEK_STUDIO_PREFS`). |
| `src/scene/AttrBuilders.h` | `aztek_scene::attr_defs` | |
| `src/scene/MeshNodes.{h,cpp}` | `aztek_scene::mesh_nodes` | The mesh node, every primitive generator with upAxis and file defaults, symmetry settings, placing on a surface, the primitive commands, smoothPreview, meshInfo, meshData, setMeshPoints, createMesh, importObj, exportObj. |
| `src/scene/Components.{h,cpp}`, `SelectPaths.cpp` | `aztek_scene::components` | Component mode and selection (state on the graph), conversion, symmetry expansion, point providers, the selection commands and path selection. |
| `src/scene/Isolation.{h,cpp}` | `aztek_scene::isolation` | Isolation sets, visibility masks, clipped loops, smooth preview face maps, inheritance. |
| `src/scene/Modifiers.{h,cpp}`, `ModifierCommands.cpp` | `aztek_scene::modifiers` | The stack, the eight built-in modifier kinds, mesh sources, edit plugs and cages, export, the modifier commands. |
| `src/scene/Instancing.{h,cpp}` | `aztek_scene::instancing` | Instances and references, promote, make unique, the instancing commands. |
| `src/scene/LastOp.{h,cpp}` | `aztek_scene::last_op` | Adjust last op: entries, drags, carried moves, placements, make non-destructive. |
| `src/scene/ComponentQuery.cpp` | `aztek_scene::component_query` | meshInfo stats, meshCheck, findComponents, selectComponentsWhere, componentInfo. |
| `src/scene/MeshEdit*.cpp`, `MeshEditInternal.h` | `aztek_scene::mesh_edit` | Every component edit command (extrude, inset, bevel, loops, merge, weld, bridge, grid fill, triangulate, reduce, mirror, separate, combine, extract, soft edges, normals, knife, symmetry), selection centres, normals and frames, and moving, rotating and scaling components with soft selection and symmetry. |
| `src/scene/SoftSelect.{h,cpp}` | `aztek_scene::soft_select` | Falloff curve, presets, user presets, softSelectCurve. |
| `src/scene/MeshFileCommands.cpp` | `aztek_scene::mesh_files` | importStl, exportStl, importPly, exportPly with units and up axis. |
| `src/scene/ObjectSets.{h,cpp}` | `aztek_scene::object_sets` | Object and component sets, binding by id and name, remapping after edits, the set commands. |
| `src/scene/ComponentPick.{h,cpp}` | `aztek_scene::component_pick` | Screen projection with near-plane clipping, nearby components and centres, fill triangles, topology callouts, selection masks. |
| `src/scene/ExtrudeModifier.{h,cpp}` | `aztek_scene::extrude_modifier` | The live extrude, its component memory (ids, centres, edge refs) and remapping. |
| `src/scene/SplitRingsModifier.{h,cpp}` | `aztek_scene::split_rings_modifier` | The live split rings modifier, its edge memory and the edit/apply/cancel selection commands. |
| `src/scene/AutoBevel.{h,cpp}` | `aztek_scene::auto_bevel` | The auto bevel modifier, its overrides and edge sets, and the bevel edge commands. |
| `src/scene/ConformCommands.cpp`, `ConformTargets.h`, `SphereState.h` | `aztek_scene::conform_commands` | polyStraighten, polyCircularize, polySpherize, polyConformShape, polyConformToObject, polyEditEdgeFlow; conform target providers; the last sphere (on the graph). |
| `src/scene/ObjectOps.cpp` | `aztek_scene::object_ops` | centerPivot, freezeTransforms, bakePivot, hide, showLastHidden, toggleVisibility, show, ungroup, resetTransforms, unfrozenTransforms, `bake_into_geometry`. Joints are refused until the rig port. |
| `src/scene/NodeBounds.{h,cpp}` | `aztek_scene::node_bounds` | |
| `src/scene/DisplayLayers.{h,cpp}` | `aztek_scene::display_layers` | The layer node, membership, display types and colour, order, the eight layer commands. `effectively_visible` reads them. |
| `src/scene/Repeat.{h,cpp}` | `aztek_scene::repeat` | |
| `src/uv/Chart`, `UvCommon.h`, `OptimizeMath.h` | `crates/aztek_uv`: `chart`, `common` | Islands, wedges, triangulation, seams from uvs, the solver geometry (frames, Jacobians, 2x2 SVD, flip-free steps). |
| `src/uv/Unwrap`, `Bff` | `aztek_uv::{unwrap, bff}` | LSCM, Tutte, SLIM (coupled and split), the chart unwrap, Boundary First Flattening with cones and cuts. |
| `src/uv/Metrics`, `Layout`, `Project` | `aztek_uv::{metrics, layout, project}` | Distortion, overlaps, texel density; hull, orient, straighten, fit; every projection with fitted frames and seam placement. |
| (Eigen's SimplicialLDLT) | `aztek_uv::sparse` | AMD ordering and simplicial LDLT through `faer`; same role, not the same rounding. |
| `src/scene/MeshBoolean.{h,cpp}` | `aztek_scene::mesh_boolean` | Manifold booleans through `manifold-rust` (a pure Rust port of Manifold 3.5), seam tagging, welding, fillet, n-gon split, the quad band stitch. |
| `src/scene/Booleans.cpp`, `BooleanModifier.cpp` | `aztek_scene::booleans` | polyBoolean (baked and live) and the boolean modifier. |
| `src/scene/Duplicate.{h,cpp}` | `aztek_scene::duplicate` | duplicate (runs, modes, groups), duplicateWithTransform, instance, fixups. The last pairs live on the graph. |

Tests: `cargo test --release` runs 419 (4 perf cases are `#[ignore]`d; run them with `--ignored`).
- `aztek_scene/tests/graph.rs`: `test_graph.cpp` and `test_node_paths.cpp` against the graph API, plus locks, user attrs, arrays, delete/undo, listeners.
- `aztek_scene/tests/commands.rs`: the same C++ tests through commands, as the C++ runs them, plus compounds, pickWalk after delete, undo history. Cases needing joints, scene files on disk or references wait for those modules.
- `aztek_geo/tests/`: every file in `tests/geo` (mesh, topology, primitives, normals, triangulate, bvh, obj, stl/ply, bevel, knife, radial symmetry, bridge, ops, loop tools, cross section, bounding, multibox), the reduce cases of `tests/retopo/test_corpus.cpp` with the shared corpus (`corpus/mod.rs` is `Corpus.h`), and the geometry cases of `test_conform.cpp`, `test_array_modifier.cpp`, `test_select_paths.cpp`, `test_smart_join.cpp` and `test_select_strokes.cpp` (lasso). `common/mod.rs` is `TestUtil.h`.
- `aztek_uv/tests/uv.rs`: the cases of `tests/uv/test_uv.cpp` that need charts, unwrapping, projections, layout and metrics only; `src/sparse.rs` has its own.
- `aztek_core/tests/core.rs`: `test_prefs.cpp` (less studio restrictions) and `test_paths.cpp` (less the image plane case).
- `aztek_scene/tests/`: `meshes.rs` (`test_meshes.cpp`, `test_primitive_axis.cpp`, `test_component_query.cpp`), `modstack.rs` (`test_modstack.cpp`, the command case of `test_select_paths.cpp`), `isolation.rs`, `extrude.rs`, `knife.rs`, `soft_select.rs`, `symmetry.rs`, `radial_symmetry.rs`, `object_sets.rs` (less referencing), `modifiers.rs`, `auto_bevel.rs`, `conform.rs` (command cases), `modeling.rs` (less joints and collision bounds), `modeling_defaults.rs`, `instancing.rs` (less files and the HUD), `create_select.rs`, `display_layers.rs` (less files), `duplicate.rs` (less the manipulator and option box), `booleans.rs` (less the quad band, which needs retopo). Cases that save files to disk go through the scene document until `saveScene`/`openScene` are ported; the binary and ASCII encodings case waits for io.

## Deliberate differences (no behaviour change intended)

- No `Graph::current()`. The graph is an owned value; the undo stack lives inside it and is swapped out while it replays.
- Hooks and listeners are `Arc<dyn Fn>` that receive the graph as an argument.
- `Graph::Batch` is `begin_batch`/`end_batch` or `batch(|g| ..)`.
- Parallel evaluation takes each level's nodes out of the map, computes them with rayon, and puts them back.
- `matrix_to_euler` uses one parity-signed formula for all six orders instead of glm's six `extractEulerAngle*` calls. Same angles away from gimbal lock; at gimbal lock it puts the whole angle in the first-applied axis.
- `find` returns `Result<Option<NodeId>>` (ambiguity is `GraphError::Ambiguous`) instead of a pointer or a throw.
- Where the C++ iterates an `unordered_map` and the order reaches the result, the port uses a `BTreeMap` or a sorted order, so results are the same on every run. Where it uses `std::sort` with ties that matter, the port uses a stable sort.
- Grid fill starts its rim walk at the first rim corner instead of the first entry of an `unordered_map`.
- Option structs that held a `const SymmetryMap*` hold `Option<&SymmetryMap>`; out parameters became return values or tuples (`flow_selection_of_points`, `mesh_to_heightfield`, `insert_support_grid`).
- `meshToHeightfield` rasterizes in parallel with an atomic max per sample instead of row-band mutexes; the result is the same max.
- serde_json is built with `float_roundtrip`, so doubles survive the scene document bit-exactly as they do in the C++.
- `std::uniform_real_distribution<double>` over `mt19937` is reproduced in the test corpus (two 32-bit draws), so the noisy test meshes match the C++ ones.

## Stand-ins for systems not ported yet

| C++ dependency | Stand-in |
|---|---|
| `geo::colors::adopt` on mesh load | Colour sets keep what the file says, unadopted |
| `spaces::activeFrame` | `Graph::set_active_frame_fn` |
| `anim::playDirection` (hideOnPlayback) | `display_layers::set_play_direction`, for the animation player to call |
| `sim::sampling()` | `Graph::set_sampling` |
| `ops::currentArgs()` (no name flag) | `Graph::set_command_default_names`, set by the session as commands run |
| `materials::rename` (keeps material slots in step) | `Graph::rename` |
| `rigel::poseControllers`, `rig::isJoint` and joint pivots | Edits run as they are; joints don't exist yet |
| `PrefStore` in the graph code (playback memory, up axis) | `Graph::set_playback_memory_mb`; `Graph::up_axis` |
| `events::send` (TimeChanged, NodeCreated, ...) | Graph listeners; no global event bus yet |
| `achievements::unlock("NAME_AND_SHAME")` | Dropped |
| `objectSets::meshRemapped` after component edits | `modifiers::add_remap_hook`; object sets register there when ported |
| `fields::modulateSoftSelection` | `mesh_edit::transform::set_soft_modulator`; fields register there when ported |
| `curve::splinesOf` for polyBridge's Path | `mesh_edit::set_curve_path_provider`; curves register there when ported |
| `core/Progress` (progress scopes and worker runs around file import and export) | Runs inline |
| `ops::options` overrides (option box ranges, labels, sections, hidden flags) | Not yet; `last_op::editable_flags` takes the hidden and adjust lists as arguments. The overrides ConformCommands and Duplicate register are left out until then. |
| `retopo::quadRemesh` for the boolean quad band | `mesh_boolean::set_quad_remesher`; without one the band falls back to the n-gon split |
| `ops::addExecuteListener` / `UndoStack` listeners for adjust last op | `CommandHost::top_level_started/finished` and `last_op::sync` after every command, undo and redo the session runs |

## Found in the C++ while porting

`NamingPolicy::fromJson` (Naming.cpp) appends parsed rules to the default-constructed `rules`, which already hold `joint: snake` and `*: Pascal`. Because `styleFor` takes the first match, a workspace's own rules (and its `"*"` style) never apply. The port keeps this behaviour; fixing it is one line (`p.rules.clear()` before the loop) in both.

`uv::project` (Project.cpp), cylindrical and spherical: a corner of a face around the axis (an n-gon cap) lying exactly opposite the seam gets `wrapAngle` of exactly plus or minus pi, so which side of the wrap it lands on depends on the last bit of the frame. Re-projecting through a frame's own explicit values can then move that corner by a whole wrap. `test_uv.cpp`'s "explicit values reproduce the fit" case passes in the C++ only because its fitted seam falls elsewhere; the port's test counts a whole wrap as equal.

## Out of scope

- The Unreal plugin (`ue/`) and the app's live link to it (`src/link`). The transform's `linkId` attribute is gone; scene files that hold it load without a warning. Unreal-style game export (naming, collision prefixes, LODs) stays in scope.
- Third-party libraries (OpenUSD, Alembic, Manifold and the like) are used as libraries, through Rust crates or bindings, not ported.

## Not ported

Everything else in `src` and `extensions`. By size (C++ lines): `ui` 185k (to egui), `scene` remainder 69k (anim, rig, deformers, modifiers, materials, cameras, lights, sims), `curve` 35k, `gn` 29k, `render` 25k (to ash), `uv`, `tex`, `bake`, `retopo`, the IO formats (FBX, Maya, glTF, USD), the command layer (`ops`), Python, extensions (116k), and the extensions.

## Progress

Since 05/11/2026 the target is Maya's behaviour (PARITY.md), and Lila is a parts bin: what is ported below is reused
where it fits that model. PARITY.md tracks progress from here.

About 55,600 of roughly 650,000 in-scope production C++ lines are ported (about 9%), plus their tests. `src/geo` is complete; `src/uv` is under way (charts, unwrap, BFF, metrics, layout, projections); the scene's mesh layer (mesh nodes, components, modifiers, instancing, isolation, every component edit, conform tools), object ops, display layers, duplicate and the prefs core are in.

As of 06/11/2026 new work is written to Maya's behaviour rather than ported: animation curves, the UV commands and
editor, joints, skinning, constraints, IK and deformers are Aztek's own, on Lila's uv and geo code where it fits. The
egui shell was replaced by a Qt Widgets shell (`app/`). See PARITY.md for status.
