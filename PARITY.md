# Aztek: behaviour target

Aztek follows Maya's behaviour for modelling, UVs, rigging and animation, plus viewport navigation, manipulators, the
script editor and preferences. Rendering, dynamics, Bifrost and the rest are out of scope. The product carries none of
Autodesk's names, icons or branding; Maya is the behavioural reference only.

Lila's C++ is now a parts bin: a piece is ported where it matches what Maya does, and rewritten to Maya's behaviour
where it does not.

## Reference and checks

- Maya's published command reference (flags, short names, create/query/edit modes, return values) and node reference
  (node types, attribute long and short names, defaults) are the spec.
- Every command and node gets tests written from that spec. Behaviour the reference does not pin down is checked
  against Maya by Simon and recorded in the test.

## Scene model

- DAG with transform and shape nodes: `pCube1` (transform) over `pCubeShape1` (mesh). Shapes are DAG leaves.
  `listRelatives -s`, instancing (one shape under several transforms) and DAG paths (`|grp|pCube1|pCubeShape1`) behave
  as Maya's.
- Dependency graph: plugs, connections, dirty propagation and lazy evaluation (the existing graph core).
- Construction history: creation nodes (`polyCube1.output -> pCubeShape1.inMesh`) and one poly node per edit
  (`polyExtrudeFace1`, `polySplitRing1`, `polyTweak1`, ...) inserted upstream of the shape, each with
  `inputPolymesh`, `output` and `inputComponents`. The `constructionHistory` toggle and `-ch` flags decide whether an
  edit adds a node or writes the shape's own mesh. `delete -ch`, `bakePartialHistory`, `polyEvaluate`.
- Component addressing: `pCube1.vtx[0:7]`, `.e[]`, `.f[]`, `.map[]`, `.vtxFace[][]`, `*`, `[1:3]`; `ls -fl`,
  `filterExpand`, selection of components through the transform or the shape.
- Default naming: `pCube1`, `pCubeShape1`, `polyCube1`, `joint1`, `ikHandle1`, `skinCluster1`, ... with Maya's
  numbering rules.
- Attribute names, short names and defaults as Maya's (`translateX`/`tx`, `rotateOrder`, `jointOrient`,
  `segmentScaleCompensate`, `inheritsTransform`, `displayHandle`, ...).

## Commands and scripting

- Command layer with Maya's create, query (`-q`) and edit (`-e`) modes, flag names and short names, multi-use flags,
  and Maya's return values.
- Embedded CPython. The official module is `aztek.cmds`; a `maya` package shim maps `maya.cmds`, `maya.mel` and
  `maya.api.OpenMaya` onto Aztek's, so simple scripts run unchanged.
- MEL: an interpreter for the subset scripts use (procedures, variables, arrays, control flow, command syntax,
  backticks, `eval`).
- OpenMaya API 2.0 subset: `MSelectionList`, `MDagPath`, `MObject`, `MFnMesh`, `MFnTransform`, `MFnDependencyNode`,
  `MPoint`/`MVector`/`MMatrix`/`MTransformationMatrix`, `MItMeshVertex`/`Edge`/`Polygon`, `MGlobal`, and Python
  plugins through `MFnPlugin` with `MPxCommand` and simple `MPxNode`s. Compiled C++ plugins cannot load.
- Script editor with Python and MEL tabs, history, echo all commands.

## Order of work

1. Scene model: transform and shape split, construction history nodes, naming, component addressing.
2. Command modes (create, query, edit) in the command layer; Maya signatures on the modelling and UV commands already
   ported, Lila's implementations behind them.
3. Modelling and UV commands to complete Maya's Mesh, Edit Mesh, Mesh Tools and UV menus.
4. Rigging: joints (orient, mirror, insert, segment scale compensate), IK (`ikSCsolver`, `ikRPsolver`, spline IK, pole
   vectors), constraints (point, orient, parent, scale, aim, pole vector), `skinCluster` with weight painting and
   mirroring, `blendShape`, set driven keys, `nurbsCurve` controls.
5. Animation: `animCurve` nodes, keyframes and tangents, time slider, graph editor, playback.
6. Python with `aztek.cmds` and the `maya` shim, then MEL, then the OpenMaya subset.
7. Application: ash viewport, egui shell; Alt-drag navigation, QWERTY tools and manipulators, marking menus, hotbox,
   channel box, attribute editor, outliner, shelf, preferences.
8. Files: Aztek's own scene format; `.ma` read and write for exchange.

## What carries over from the port so far

Kept as is: `aztek_geo`, `aztek_uv`, `aztek_undo`, `aztek_core`, the command infrastructure in `aztek_ops`, and the
graph, plug, node and transform core of `aztek_scene` (Maya's pivot model, rotate orders, locks, connections,
evaluation, undo).

Reworked to the scene model above: Lila's mesh node (a transform carrying a mesh), its baked edits and modifier stack,
its command names and flags, and its naming. The edit algorithms behind them stay. Lila's live modifier stack and live
booleans remain available as extras where they do not conflict with Maya's model.

## Status

Step 1 (scene model), as of 05/11/2026:

- Transform and shape split: every mesh is a `transform` over a `mesh` shape (`pCube1` / `pCubeShape1`); `locator`
  is a shape under its transform. Names and selections take either; edits resolve the transform to its shape, and
  results and component strings name the transform, as Maya does.
- Construction history: component edits insert a node per edit after the generator (`polyCube1 -> polyExtrudeFace1
  -> pCubeShape1`), its settings the command's flags and its components in `inputComponents` (`f[0:3]`). Changing a
  setting, or the generator's, re-runs the chain. A mesh with no history gets an intermediate `polySurfaceShape1` to
  feed the first node. Vertex moves on a mesh with history go into a `polyTweak`. `delete -ch` bakes and removes the
  history; deleting an object takes its history; `constructionHistory -toggle` turns recording off; `listHistory`
  lists the chain; primitives take `constructionHistory=False`. History saves and loads.
- Edits with history nodes: extrude (faces, edges), inset, bevel, bridge (not along a path curve), delete components,
  insert edge loop (`polySplitRing`), split rings, offset loop, merge, merge to centre, collapse, dissolve, poke, spin
  edge, fill hole (`polyCloseBorder`), grid fill, flip (`polyNormal`), conform, triangulate, quadrangulate
  (`polyQuad`), soft/hard edge, duplicate faces, connect, weighted normals, lock and unlock normals.
- Component strings: `select("pCube1.f[0:2]")` with add, deselect and toggle; `ls(sl=True)` returns the current
  mode's components as ranges, `flatten=True` one per item.

Still destructive (they bake the history before them): knife, target weld, symmetrize, combine and separate, bridge
along a path, edits under radial symmetry, vertex moves with symmetry on (the symmetry pairs live on the stored
mesh), and the commands not yet looked at. Node names that differ from Maya's where Aztek's edit has no Maya twin:
`polySplitRingsFace/Edge`, `polyOffsetLoop`, `polyMergeCenter`, `polyDissolveVertex/Edge`, `polyBevelFace`,
`polyBridgeFace/Edge`, `polyGridFillVertex/Edge/Face`, `polyConnectVertex/Edge`, `polyDuplicateFaces`,
`polyWeightedNormals`, `polyUnlockNormals`, `polySpinEdge`.

Next: the remaining destructive edits, then step 2 (query and edit modes, Maya flag names and defaults).

Application, as of 06/11/2026: Qt Widgets shell over the Rust core (`cmake --build build`, see README.md). Menus by
menu set (Modeling, Rigging, Animation), status line, shelves (Poly Modeling, General, Rigging, Animation, UV Editing,
Custom), toolbox and layouts, four views with model panel menus, outliner, channel box with Maya's channel colours,
picked channels and the middle-drag virtual slider, layer editor, attribute editor, script editor, command line, help
line, time and range sliders with key ticks, playback, marking menus, hotbox, option boxes, preferences, Maya's
hotkeys. Viewport: Alt navigation, picking with pre-selection highlight, marquee, edge loops on double-click, move,
rotate and scale manipulators, smooth preview, soft selection falloff colours, joints, IK handles, locators and
deformer handles.

Animation, as of 06/11/2026: `animCurveTL/TA/TU` nodes with auto, spline, clamped, linear, flat, step, plateau and
fixed tangents and infinity modes; `setKeyframe`, `keyframe` (query and edit), `cutKey`, `keyTangent`,
`findKeyframe`, `currentTime` (frames), `playbackOptions`, `autoKeyframe`; changed channels hold until the time moves,
as in Maya. Graph Editor with key picking, dragging, tangent buttons and stats.

UVs, as of 06/11/2026: maps (`pCube1.map[3]`) and shells derived from the per-corner UVs and seams; `polyProjection`
(planar, cylindrical, spherical), `polyAutoProjection`, `u3dUnfold`, `u3dLayout`, `polyMapCut`, `polyMapSew`,
`polyMapSewMove`, `polyEditUV` (query too), `polyNormalizeUV`, `polyFlipUV`, `polyRotateUVs`, each a history node
(`polyPlanarProj1`, `polyTweakUV1`, ...). UV Editor.

Rigging and deformers, as of 06/11/2026: `joint` (create, edit with orient, query), `orientJoint`, the joint tool,
`skinCluster` (closest distance weights, unbind, influence query), `skinPercent`, `parentConstraint`,
`pointConstraint`, `orientConstraint`, `scaleConstraint`, `aimConstraint` (with maintain offset), `ikHandle` (RP and SC
solvers, pole vector, twist), `nonLinear` (bend, twist, squash, flare, sine, wave) with handles, `cluster`,
`blendShape` with a weight per target. Everything saves and loads.

MEL, as of 06/11/2026: a real interpreter (`crates/aztek_host/src/mel/`). Comments, all five types with arrays and
coercions, globals, top-level variables that stay between runs, operators, control flow, procedures (array arguments by
reference, local to their file), command and function syntax, backticks, `eval`, `source` and auto-sourcing from
`AZTEK_SCRIPT_PATH` or `MAYA_SCRIPT_PATH`, `catch`, `python`, `whatIs`, `exists`, `about`, and the standard string, math
and array functions. Multi-use flags read their arity; query mode gives a flag a value only by a per-command table.
Differs from Maya: unknown procedures and undeclared variables fail when reached, not before the script starts; errors
carry a line number only for syntax errors; an array of one element converts to a scalar on assignment.

Deform menu, as of 06/11/2026: `lattice` (`ffd`, `lattice`, `baseLattice`; `ffd1Lattice.pt[s][t][u]` select, move and list with
`ls -fl`; local influence, outside modes, freeze mapping, `-e -remap`, `-e -resetLattice`), `softMod` (selection or centre,
volume or surface, `falloffCurve` ramp), `deltaMush`, `tension`, `wrap` (with `deformer -type wrap` and `wrap -e -inf`), `wire`,
`deformer` (`-type`, `-q -g`, `-q -gi`, `-e -g`, `-e -rm -g`), `percent`, `reorderDeformers`, `listHistory -type`. Deleting a deformer
closes the chain and takes its handle or lattice. Differences: a deformer on several meshes is one chain node per mesh with the
first node's settings connected in, so `ls` shows ffd1 and ffd2 where Maya has one node. `wire` follows the driver's points as a
polyline, so a NURBS curve follows its control points until a sampler is added; the base wire is data on the node, with no
`curve1BaseWire` shape. Deformer sets are not made; `percent` holds membership as weights.

Files, as of 06/11/2026: `.ma` read and write (`ma_file.rs`) and the `file` command (`-new`, `-open`, `-save`, `-rename`,
`-import`, `-exportAll`, `-exportSelected`, `-type`, `-q -sn -ex -type`). Reads transforms, joints, locators, meshes
(`.vt .ed .fc`, UV sets, `.pt`, hard edges), poly creators, `polyTweak` over stored geometry, animCurves (T and U kinds,
tangents, infinities), skinCluster, dagPose, constraints, RP and SC ikHandle, display layers and user attributes. Other
history is taken from stored geometry; what cannot be represented is a warning naming its node type. Writes the same,
meshes baked. Written from knowledge of the format with no Maya to check against; `.kit`/`.kot` codes, `.kix`/`.kiy`
units and `.xm` xform order are the parts to check first. Not handled: references, instances, NURBS, blendShape and
cluster (baked with a warning), multiple shapes sharing a skinCluster.

