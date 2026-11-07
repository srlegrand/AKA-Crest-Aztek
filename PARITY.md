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

Polygon command set, as of 06/11/2026 (`crates/aztek_scene/src/mesh_edit/maya_poly*`, `tests/maya_poly.rs`): the reference's
command names run natively with all their flags, taken from the reference's tables. Component arguments are read by
the command and leave the selection alone; commands return their history node, or [transform, node]. Query and edit
reach each node's attributes by the reference's names, short names included (`polyExtrudeFace1.ltz`). Primitives:
`polyCube`, `polyPlane`, `polySphere`, `polyCylinder`, `polyCone`, `polyTorus`, `polyDisc`, `polyPipe` answer to the
reference's attribute names, old names as aliases; `polyPrism`, `polyPyramid`, `polyHelix`, `polyPlatonicSolid`,
`polyPrimitive`, `polySoccerBall`, `polySuperShape` are new. New or now native with a node: extrude facet, edge and
vertex (with divisions, local and world moves, along a curve), `polyMoveVertex/Edge/Facet`, `polyChipOff`, `polyBevel3`,
`polySplitRing`, `polyMergeVertex/Edge/Facet`, `polyDelVertex/Edge/Facet`, `polyCollapseEdge/Facet`, `polyChamferVtx`,
`polyCircularize`, `polyConnectComponents`, `polySplitVertex`, `polySplitEdge`, `polyDetachEdges`, `polySubdivideFacet/Edge`,
`polyPoke`, `polyWedgeFace`, `polyDuplicateEdge`, `polyFlipEdge`, `polyHole`, `polyAverageVertex`, `polySplit` (`-ep -fp
-ip -pc`), `polyProjectCurve`, `polyAppend`, `polyCreateFacet`, `polySmooth`, `polyReduce` (`-p -trm -vct -tct`),
`polyRemesh`, `polyRetopo`, `polyMirrorFace`, `polyUnite`, `polySeparate`, `polyCBoolOp`, `polyBoolOp`, `polyBooleanCmd`,
`transferAttributes` (live), `polyNormal`, `polyNormalPerVertex`, `polySetToFaceNormal`, `polyAverageNormal`,
`polyColorPerVertex`, `polyColorSet`, `polyUVSet`, `polyMoveUV`, `polyMergeUV`, `polyLayoutUV`, `polyCopyUV`, `polyForceUV`,
`polyContourProjection`. Without nodes: `polyInfo`, `polySelect`, `polySelectConstraint` (state, `-q`, `-rs`, `-pp`),
`polySelectSp`, `polyListComponentConversion`, `filterExpand`, `polyCleanup`, `polyCleanupArgList`, `polyClipboard`,
`polyPasteFace`, `polyOptions` (a table for scripts), `meshReorder`, `polyPinUV`.

Falls short of the reference: `polySeparate` makes one node per shell rather than one with several outputs;
`polyExtrudeFacet -inc` keeps the curve's samples, so a later change to the curve doesn't follow; `polyWedgeFace` and
`polyMirrorFace` have no smoothing; `polySmooth` on part of a mesh relaxes the new points rather than running Catmull-Clark,
`-mth 1` ignores `-ps -ro`, and the `osd` flags are accepted only; `polyHole` marks faces but they still draw;
`polyContourProjection` is a planar projection unfolded; `polyCreateFacet` and `polyAppend` take no holes; `-roundCap` is
accepted and the ends stay flat; `polySubdivideFacet -m 1` is one fan per level, from memory, not checked against the
original. Mixed `-ep` and `-fp` run edges first, since flags don't keep their order across kinds.

Still destructive (they bake the history before them): target weld, symmetrize, bridge along a path, `meshReorder`,
`polyPinUV`, `polyClipboard -ps`, `polyCleanup` when it fixes, `polyTransfer` of vertices and colours, edits under radial
symmetry, vertex moves with symmetry on (the symmetry pairs live on the stored mesh). Node names that differ from Maya's where Aztek's edit has no Maya twin:
`polySplitRingsFace/Edge`, `polyOffsetLoop`, `polyMergeCenter`, `polyDissolveVertex/Edge`, `polyBevelFace`,
`polyBridgeFace/Edge`, `polyGridFillVertex/Edge/Face`, `polyConnectVertex/Edge`, `polyDuplicateFaces`,
`polyWeightedNormals`, `polyUnlockNormals`, `polySpinEdge`.

Next: the remaining destructive edits.

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

Animation in full, as of 06/11/2026: tangents as Maya holds them, a vector of seconds and value, with angles in value per
second; weighted curves are cubic Beziers in time and value (`keyTangent -wt -iw -ow -ix -iy -ox -oy -l -wl -unify -r`),
unweighted ones Hermite. `keyframe`, `setKeyframe` (`-i` keeps the curve's shape), `cutKey -o`, `selectKey` with their
remaining flags. `rotationInterpolation`, `filterCurve` (euler, butterworth, gaussian, keyReducer, keySync, peakRemover,
resample, simplify), `simplify`, `bufferCurve`, `keyframeStats`, `mute`, `setAttr -k`. `motionPath`, `pathAnimation`,
`snapshot`, `motionTrail` (drawn in the viewport). `animLayer` with blend node chains, keys resolved through the stack,
`mergeAnimLayers`. `character`, `currentCharacters`, `clip`, `clipSchedule`, `pose`. `playbackOptions`, `play`,
`timeControl`, `currentUnit`, ghosting as scene state. Graph Editor menus: infinities, tangents, Euler filter, simplify,
resample, buffer curves, stacked and normalized views, isolate, copy, paste, snap, retime hotkeys.

Not yet: tangent handles dragged in the Graph Editor; ghosts drawn; `flow` needs the lattice deformer; `-cp` keys no
control points, which have no channels of their own; the Trax Editor. Bank amount and the unweighted-to-weighted
conversion of fixed tangents are our reading, not checked against Maya, as are the tangent formulas beyond what its
documentation states. Clips play through a `clipChannel` node per channel, which Maya does not have.

UVs, as of 06/11/2026: maps (`pCube1.map[3]`) and shells derived from the per-corner UVs and seams; `polyProjection`
(planar, cylindrical, spherical), `polyAutoProjection`, `u3dUnfold`, `u3dLayout`, `polyMapCut`, `polyMapSew`,
`polyMapSewMove`, `polyEditUV` (query too), `polyNormalizeUV`, `polyFlipUV`, `polyRotateUVs`, each a history node
(`polyPlanarProj1`, `polyTweakUV1`, ...). UV Editor.

Skeletons, IK and constraints, as of 06/11/2026: `joint` with its query and edit flags (limits, degrees of freedom,
preferred angles, zero scale orient), joint labels, segment scale compensate through `inverseScale`, `mirrorJoint`,
`insertJoint`, `removeJoint`, `connectJoint`, `disconnectJoint`, `reroot`. `ikHandle` query and edit, a real `ikEffector`,
`ikSolver`, `ikSystem`, `.ikBlend`, `.twist`, `.roll`, chains of any length, solving from the preferred angles within
joint limits. Handle `weight`, `poWeight`, `priority`, `stickiness` and joint `stiffness` are stored but don't change the
solve. Constraints: parent, point, orient, scale, aim, `poleVectorConstraint`, `geometryConstraint`, `normalConstraint`,
`pointOnPolyConstraint` with `-e`, `-q`, `-rm`, skip flags, offsets, rest position, `interpType`, `nodeState` and weight
attrs `<target>W0`. Not yet: `tangentConstraint` (needs curves), spline IK, an Insert Joint drag tool. Constraint nodes
are not DAG children of what they drive, as they are in Maya: the graph orders whole nodes, so that would be a cycle.

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
`curve1BaseWire` shape.

Deform and skin weights, as of 06/11/2026. `blendShape`: `.weight[i]` with target aliases (`aliasAttr`, `listAttr -m`), `-t`, `-w`,
`-ib`, `-ibt`, `-tc`, `-o`, `-par`, `-foc`, `-e -rm -t`, `-rtd`, `-q -w -wc -t -g -en -o -tc`, live targets baked on delete, per-point
base and target weights (`.inputTarget[0].baseWeights[v]`, `.inputTargetGroup[t].targetWeights[v]`), `sculptTarget`, `invertShape`,
Shape Editor. `sculpt`, `jiggle` (stepped a frame at a time, seen frames kept), `shrinkWrap`, `morph`, `wrinkle`, `proximityWrap`,
`solidify`, `pointOnCurveDeformer` (`deformer -type pointOnCurve`), `textureDeformer` (procedural value plus `.texture`), `curveWarp`.
`artAttrCtx`, `artSetToolAndSelectAttr`, the Paint Attribute tool with its Tool Settings page and Deform > Paint Weights. Skin:
`-bm 2` real heat diffusion with `-hmf`, `-sm 2` with `.blendWeights` and `skinPercent -dqb`, `deformerWeights` (XML, JSON; index,
nearest, barycentric, bilinear, over), `weightHammerVerts`, `copySkinWeights -uv`, `transferBetweenInfluences`, `-ai -ug`,
`substituteGeometry`, `-mjm`, `removeUnusedInfluences`, `-nw 2`, Hold shown in the paint tool. `pCubeShape1Orig` as a real
intermediate shape (`listHistory -pdo`, `deformableShape`, Outliner Display > Intermediate Objects), `cluster1Set` as real membership
(`sets`, `deformer -e -g`, Edit Membership Tool).

Differences and gaps: `blendShape` origin defaults to local, where the documentation says world and the option box sends local.
`-par` makes no parallelBlender node: the blendShape sits last, measured from the original, which adds the same way. wrinkle is one
node with a handle, not a cluster over wires, and creases by normal. solidify fits the whole mesh, so several shells count as one.
shrinkWrap's `.projection` enum order is from memory and needs checking against Maya. `copySkinWeights -uv` reads the one UV set.
`transferBetweenInfluences`, `attrPaintStamp` and `pointOnCurveDeformer` are Aztek's own names. A skinCluster has no set, so
membership doesn't limit a skin. `.ma` doesn't carry the new deformers. Not done: `poseInterpolator`.

Files, as of 06/11/2026: `.ma` read and write (`ma_file.rs`) and the `file` command (`-new`, `-open`, `-save`, `-rename`,
`-import`, `-exportAll`, `-exportSelected`, `-type`, `-q -sn -ex -type`). Reads transforms, joints, locators, meshes
(`.vt .ed .fc`, UV sets, `.pt`, hard edges), poly creators, `polyTweak` over stored geometry, animCurves (T and U kinds,
tangents, infinities), skinCluster, dagPose, constraints, RP and SC ikHandle, display layers and user attributes. Other
history is taken from stored geometry; what cannot be represented is a warning naming its node type. Writes the same,
meshes baked. Written from knowledge of the format with no Maya to check against; `.kit`/`.kot` codes, `.kix`/`.kiy`
units and `.xm` xform order are the parts to check first. Not handled: references, instances, NURBS, blendShape and
cluster (baked with a warning), multiple shapes sharing a skinCluster.

Python API, as of 06/11/2026: `aztek.api.OpenMaya` and `aztek.api.OpenMayaAnim`, also as `maya.api.*`: value classes
and arrays, `MGlobal`, `MSelectionList`, `MObject`, `MDagPath`, `MPlug`, modifiers, function sets for nodes, transforms,
meshes (rays and closest points included), curves, sets, attributes, data and components, the selection, DAG, graph and
mesh iterators, `MFnSkinCluster`, `MAnimControl`, `MFnAnimCurve`. Python plug-ins with `loadPlugin`, `unloadPlugin` and
`pluginInfo`: `MPxCommand` (runs from `cmds` and MEL, `undoIt` and `redoIt` on the undo stack) and `MPxNode` for plain
dependency nodes. Differences from Maya: only dependency nodes can come from plug-ins (no `MPxDeformerNode` or
`MPxLocatorNode`), mesh attributes don't reach `compute`, compound attributes can't be arrays, one UV set (`map1`), one
path per DAG node, `setKeyable` only on added attributes, `MFnTransform` has no shear, and cycles are refused node by
node, not plug by plug. MEL can't pass flags to a plug-in command with no syntax. `MRampAttribute` isn't there.

NURBS surfaces, as of 06/11/2026: `nurbsSurface` shape, CVs as `surface1.cv[u][v]` (select, move, undo, tweaks on top of
history), drawn shaded with isoparms and a CV hull in vertex mode, `selectType -cv`, `displaySmoothness` and the 1, 2, 3
keys. `sphere`, `nurbsCube`, `cylinder`, `cone`, `torus`, `nurbsPlane` with `make*` nodes. `loft`, `revolve`, `extrude`
and `planar` make NURBS, polygons with `-po 1`; `boundary`, `square`, `singleProfileBirailSurface`, `rebuildSurface`,
`reverseSurface`, `insertKnotSurface`, `detachSurface`, `attachSurface`, `offsetSurface`, `closeSurface`, `nurbsToPoly`
(`nurbsTessellate`), `duplicateCurve` for isoparms, `pointOnSurface`, and the `surfaceInfo`, `closestPointOnSurface` and
`pointOnSurfaceInfo` nodes. Differs from Maya: no trims, so `planar` makes a surface only for a rectangle and otherwise
polygons with a warning; non-rational, so a sphere is Maya's cubic approximation only; `-ut` and `-tol` on primitives
are accepted and ignored; `loft` takes degrees 1 to 3; `attachSurface` copies its second surface rather than following
it. None of this has been compared with a running Maya: the sphere's CV values, which way U runs on `loft` and `extrude`,
the direction flags' numbering and the bare commands' axis of 1 0 0 are from memory of Maya's files and docs.

Utility nodes, expressions and core commands, as of 06/11/2026: `multiplyDivide`, `plusMinusAverage`, `condition`,
`clamp`, `setRange`, `reverse`, `blendColors`, `blendTwoAttr`, `remapValue`, `distanceBetween`, `angleBetween`,
`vectorProduct`, `pairBlend`, `decomposeMatrix`, `composeMatrix`, `multMatrix` and the other matrix nodes, the quaternion
nodes, `blendMatrix`, `aimMatrix`, `pickMatrix`, the float, colour and modern math nodes, `closestPointOnMesh`
(`utility_nodes.rs`). Connections depend on plugs rather than nodes, so `pCube1.tx -> node -> pCube1.ty` runs; array
elements have components (`input3D[0].input3Dx`) and can drive. `expression` node and command (`expression.rs`) over real
connections. Transforms: `offsetParentMatrix`, `shear`, limits and `transformLimits`, `inheritsTransform`, inverse
matrices, `rotateQuaternion`, bounding box attributes, `hiddenInOutliner`. UUIDs and namespaces. Full flag sets for the
core commands (`maya_attrs.rs`, `maya_xforms.rs`, `maya_scene.rs`).

Differences from Maya in these: `group` goes under the world, not the members' shared parent, because existing tests and
tools rely on it. `src.t -> dst.r` followed by `dst.r -> src.s` is refused as a cycle, which Maya allows, for the same
reason. `parentMatrix` read through a connection includes `offsetParentMatrix` (getAttr gives Maya's value). Expression
numbers are all doubles, so `1/2` is 0.5. `parent -add` makes an instance through `instance`. `closestPointOnMesh` leaves
`parameterU` and `parameterV` at 0. Limits apply when the node computes, so a plug reading a limited channel directly can
see the value before the limit. The defaults of `multiplyDivide.input1`, `condition.colorIfTrue` and `floatMath.floatB`
were written from memory with no Maya to check against. In MEL a 0 or 1 after a
switch of move, rotate or scale is a value, not the switch's own, so `move -r 1 0 0` reads as it should.

`.ma` files keep utility nodes and expressions, and read Maya's slot spelling of an expression (`.O[0] = .I[0] * 2;`). On
reading, a `unitConversion` is seen through, its factor dropped, since Aztek's angles are degrees throughout.

Not yet in these: `polyEvaluate -b2` and `-bc2`, `select` of `pCube1.vtx[*]`; `displayCull`, `snapMode` and the display attributes (`displayHandle`, `displayLocalAxis` and so on) hold their values
but the viewport does not draw from them.

Not yet: weight painting, lattice, wrap, wire, delta mush, spline IK, set driven keys, NURBS curves, the dope sheet,
the node editor, `.ma` files, the OpenMaya subset.

Interface, as of 06/11/2026: named actions callable from hotkeys, MEL and Python (`runTimeCommand`, `nameCommand`,
`hotkey`), an editable hotkey table in the Hotkey Editor, saved in settings. Status line: selection scope, masks
(`selectType`), snap toggles beside the X, C and V hold keys (grid, curve, point, view plane, live), lock and highlight
selection, symmetry dropdown, soft select, numeric input and select by name. Selection: lasso, paint selection, Tab-drag,
Shift+double-click loops and rings, Shortest Edge Path Tool, All by Type, Similar, Shell, Shell Border, Quick Select Sets.
Manipulators: plane handles, Ctrl-click constraints, view ring, trackball, numeric readout, J discrete rotate, step snap,
component scale along the manipulator's axes, B-drag sizing the falloff; W, E, R, Q + left press menus; right-click menus
by object type with history inputs and Paint Weights on a joint. Hotbox with zones. Shelves saved between sessions, Shelf
Editor, middle-drag from the Script Editor, Ctrl+Shift+click on a menu item, `shelf_*.mel` import; Curves / Surfaces and
Sculpting shelves. Windows: Attribute Spread Sheet, Component Editor with skin weights, Connection Editor, Hypergraph,
Namespace Editor (listing only), Channel Control, Modeling Toolkit, Shape Editor,
workspaces, main pane only, panel layouts, saved layouts, a panel holding an outliner or editor. Panel menus: cameras,
`lookThru`, bookmarks, camera tools, gates, safe action; flat, bounding box, X-Ray and its joint and component kinds,
backface culling; Show filters, `isolateSelect`. Heads up display and `headsUpDisplay`; `polyOptions`. In-View Editor and
the channel box's virtual slider on INPUTS. Attribute Editor sections by node type, sliders, Notes, Copy Tab. Outliner
filters and icons. Layer editor V, P, T, R, colour, reorder. Script Editor tabs, highlighting, line numbers, save and
source. File, Edit, Modify and Display menus; Preferences saved and `optionVar`; time units by name; playback speed and
looping in real time.

Not yet in the interface: Topology symmetry (World X, Y, Z act in object space: `setSymmetry` has no world mode), Cut,
curve Edit Point and Curve Point modes, UV and Vertex Face component modes in the marking menu, manipulator handles for
history nodes (T shows their numbers in the view instead), per-object display toggles are not saved with the scene,
Outliner reorder between siblings, linear units and Z up. Roll stays in the view, not the
camera node.
