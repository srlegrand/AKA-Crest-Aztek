# Command reference

Generated from the command registry by `cargo run -p aztek_host --example gen_commands`; do not edit by hand.
Commands take Maya's flags, long or short, in MEL and in Python (`aztek.cmds` or `maya.cmds`).

321 commands.

## AppendToPolygonTool

The Append to Polygon tool.

## ArtPaintSkinWeightsTool

The paint skin weights tool.

## ArtPaintSkinWeightsToolOptions

The paint skin weights tool and its settings.

## AttributeEditor

Show the Attribute Editor.

## CVCurveTool

The CV curve tool: click to place CVs, Enter to finish.

## CreatePolygonTool

The Create Polygon tool.

## DopeSheetEditor

Open the Dope Sheet.

## EPCurveTool

The EP curve tool: click to place edit points, Enter to finish.

## FourViewLayout

Four views: top, persp, front, side.

## FrameAll

Frame everything in the active view.

## FrameSelected

Frame the selection in the active view.

## GraphEditor

Open the Graph Editor.

## JointTool

The joint tool: click to place joints.

## MoveTool

The move tool.

## MultiCutTool

The Multi-Cut tool.

## NodeEditorWindow

Open the Node Editor.

## OffsetEdgeLoopTool

The Offset Edge Loop tool.

## OutlinerWindow

Show the Outliner.

## PlaybackToggle

Start or stop playback.

## PolyCreaseTool

The Crease tool.

## PreferencesWindow

Open the Preferences.

## QuadDrawTool

The Quad Draw tool.

## RotateTool

The rotate tool.

## ScaleTool

The scale tool.

## ScriptEditor

Open the Script Editor.

## SculptMeshActivateBrushSculpt

The sculpt brush.

## SelectTool

The select tool.

## SetDrivenKeyOptions

Open the Set Driven Key window.

## SingleViewLayout

One view: the perspective.

## SlideEdgeTool

The Slide Edge tool.

## SplitEdgeRingTool

The Insert Edge Loop tool.

## TargetWeldTool

The Target Weld tool.

## TextureViewWindow

Open the UV Editor.

## ToggleChannelsLayers

Show or hide the Channel Box / Layer Editor.

## ToolSettingsWindow

Show the Tool Settings.

## UVEditor

Open the UV Editor.

## addAttr

Add an attribute to objects (Maya's addAttr): attributeType for a single value or, with multi, an array of them; dataType for strings and arrays (stringArray, doubleArray, Int32Array, vectorArray). attributeType='compound' groups the attributes then added with parent=; a multi compound gives each element every child, node.targets[2].weight. Compounds nest one level. Saved with the scene. Returns the new plugs.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `longName` | `ln` | The attribute's name. |
| `shortName` | `sn` | A short name. |
| `niceName` | `nn` | The label editors show. |
| `attributeType` | `at` | bool, long, float, double, double3, enum, matrix or compound (Aztek's int, vec3 too). |
| `parent` | `p` | The compound this attribute is a child of. |
| `numberOfChildren` | `nc` | Accepted for Maya scripts: a compound takes any number of children. |
| `dataType` | `dt` | string, stringArray, doubleArray, floatArray, Int32Array, vectorArray or matrix. |
| `multi` | `m` | An array of attributeType: any number of elements, attr[0], attr[1] ... |
| `defaultValue` | `dv` | Default value (each element's, for an array). |
| `minValue` | `min` | Lowest value it takes: lower values are clamped. |
| `maxValue` | `max` | Highest value it takes: higher values are clamped. |
| `softMinValue` | `smn` | Slider minimum. |
| `softMaxValue` | `smx` | Slider maximum. |
| `enumName` | `en` | Enum labels, 'low:medium:high'. |
| `keyable` | `k` | Show it in the channel box, where it can be keyed. |
| `hidden` | `h` | Hide it from editors. |
| `usedAsColor` | `uac` | A double3 holding a colour: editors show a swatch. |
| `usedAsFilename` | `uaf` | A string holding a file path: editors offer a file browser. |
| `category` |  | A heading the Attribute Editor groups it under. |
| `doc` |  | What it is for: editors show it as a tooltip. |

## addModifier

Add a modifier to the top of a mesh's stack. params sets its parameters. Returns its name. modifierKinds(docs=True) also lists kinds extensions add later. Kinds: - array: Repeats the mesh in a row, around a circle or in a grid, each copy stepped from the last. Use it for railings, stairs, tiles, gear teeth and bolts round a flange. Radial with weld on turns one modelled wedge into an exact N-fold whole. - autoBevel: Bevels the fold edges it finds live on its input by angle, side and length rules, plus edges picked by hand. Use it for game-art bevels that keep following the mesh as you edit it. - bevel: Bevels every edge sharper than an angle so hard corners catch the light. A quick live bevel; Auto Bevel has finer edge rules and hand-picked edges. - boolean: A boolean kept live: cuts, joins or intersects this mesh with a cutter mesh, so moving or editing the cutter updates the result. Stack one per cutter. Apply bakes it. - decimate: Reduces the triangle count while keeping the shape, silhouette, UVs and normals. Use it for LODs or to lighten scans and sculpts. - extrude: Extrudes faces, or grows strips off edges, kept live: the faces or edges selected when it was added follow later edits under it, and its thickness and offset stay adjustable. Without a selection it extrudes every face. - mirror: Adds a mirrored copy across a plane through the object's origin and welds the seam. Model one half of a symmetrical object and let this build the other. - solidify: Gives a surface thickness: an inner shell offset along the normals, joined to the outer one at open borders. Use it to turn planes and open shells into solid panels. - splitRings: Subdivides one way, kept live: loops across the rings of the edges or faces selected when it was added, curved to the surface. Without a selection it splits the whole mesh. - subdivision: Smooths the mesh with Catmull-Clark subdivision. Use it to turn a low-poly cage into a smooth high-poly surface. - triangulate: Splits every face into triangles. Use it to see, or lock in, the triangulation an engine will get. - weightedNormals: Weights vertex normals by face area so big flat faces shade flat and the bevels take the curvature. Use it on bevelled hard-surface game meshes.

Arguments: mesh

| Flag | Short | Description |
| --- | --- | --- |
| `kind` |  | Modifier kind, such as bevel or mirror (see Kinds). |
| `params` |  | {param: value} to set on the new modifier. |

## addToSet

Add members (the selection by default) to a set. Returns the set's members.

Arguments: set, members

## aimConstraint

Constrain the object named (or selected) last to the targets before it. -mo keeps the current offset; -w sets the targets' weight. Returns [constraint].

Arguments: targets, then the object to drive

| Flag | Short | Description |
| --- | --- | --- |
| `maintainOffset` | `mo` | Keep the object where it is now relative to the targets. |
| `weight` | `w` | Weight of each target (1). |
| `name` | `n` | Name of the constraint. |
| `aimVector` | `aim` | Accepted for Maya scripts (local X aims). |
| `upVector` | `u` | Accepted for Maya scripts (local Y up). |
| `worldUpType` | `wut` | Accepted for Maya scripts. |
| `skipTranslate` | `st` | Accepted for Maya scripts. |
| `skipRotate` | `sr` | Accepted for Maya scripts. |

## applyAllModifiers

Bake every modifier of a mesh into its base mesh, leaving an empty stack over an editable source.

Arguments: meshes

## applyModifier

Bake a modifier (and everything below it in the stack) into the base mesh.

Arguments: modifier

## arclen

The length of a curve along its arc, world space.

Arguments: a curve (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `constructionHistory` | `ch` | Accepted for Maya scripts. |

## arrayCenter

Set what an array modifier's copies turn and scale about. Give an object to centre on its transform (its position and axes, kept live as it moves; run again after moving the arrayed mesh), pivot=True to copy the mesh's pivot into centerPoint, or neither to go back to the object's origin.

Arguments: modifier, [object]

| Flag | Short | Description |
| --- | --- | --- |
| `pivot` |  | Centre on the arrayed mesh's current pivot. |

## artAttrSkinPaintCtx

The paint skin weights tool's settings. Edit and query: -val value, -op opacity, -r radius, -sao the operation (absolute, additive, scale, smooth), -stP the stamp (gaussian, poly, solid, square), -inf the influence. -clear floods the mesh with the value.

Arguments: the tool's name

| Flag | Short | Description |
| --- | --- | --- |
| `value` | `val` | The value the brush paints, 0 to 1. |
| `opacity` | `op` | How much of the value each dab applies. |
| `radius` | `r` | Brush radius in world units. |
| `selectedattroper` | `sao` | absolute (replace), additive, scale or smooth. |
| `stampProfile` | `stP` | gaussian, poly, solid or square. |
| `influence` | `inf` | The joint painted. |
| `colorfeedback` | `cl` | Show the weights in colour. |
| `clear` | `clr` | Flood: paint the value over the selection's points. |
| `edit` | `e` | Edit mode. |
| `query` | `q` | Query mode. |

## attachCurve

Join curves in order. With -rpo the first takes the result. Returns the transform.

Arguments: curves (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `replaceOriginal` | `rpo` | The first curve takes the result. |
| `name` | `n` | Name of the new curve. |
| `method` | `m` | Accepted for Maya scripts. |
| `blendBias` | `bb` | Accepted for Maya scripts. |
| `constructionHistory` | `ch` | Accepted for Maya scripts. |

## autoBevel

Add an autoBevel modifier to the top of each mesh's stack: bevels fold edges found live on its input mesh. Returns the modifier names.

Arguments: meshes

| Flag | Short | Description |
| --- | --- | --- |
| `minAngle` |  | Smallest fold, degrees between the face normals. |
| `maxAngle` |  | Largest fold, degrees. |
| `side` |  | both, outside (convex) or inside (concave). |
| `thin` |  | Skip thickness edges narrower than this, cm. |
| `minLength` |  | Skip edges shorter than this, cm. |
| `completeCorners` |  | Add the quad loops joining folds that meet at an angle. |
| `mode` |  | auto, autoOverrides, manual, angle or weight. |
| `weightThreshold` |  | weight mode threshold. |
| `offset` |  | Width, cm. |
| `segments` |  | Edges across each bevel, 1 to 32. |
| `profile` |  | 0.5 round, 0.25 straight. |
| `clamp` |  | Clamp the width. |
| `smoothingAngle` |  | Hard-edge angle for the new edges. |

## autoKeyframe

Auto key: changing an animated channel keys it at the current frame. Returns whether it is on.

| Flag | Short | Description |
| --- | --- | --- |
| `query` | `q` | Return the value of the flags named instead of setting them. |
| `edit` | `e` | Change the flags named. |
| `state` | `st` | On or off. |

## bakeModifierStack

Bake meshes to their current result, deleting the generator and modifiers feeding them. Curves likewise keep what their history makes and lose the history; other objects are skipped.

Arguments: objects

## bakePivot

Turn the objects onto their pivots' axes, counter-rotating their points, so the pivot orientation becomes the object's own and scale works along it. Nothing moves in the world. Instances are made unique.

Arguments: objects

## bakeResults

Key channels at every -sb frames over -t with the values they evaluate to, replacing what drove them. -hi below includes the hierarchy, -at the channels; -pok keeps keys outside the range. Returns how many channels.

Arguments: objects or plugs (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `attribute` | `at` | Channel name(s). |
| `time` | `t` | The range to bake: "1:100" (the playback range by default). |
| `sampleBy` | `sb` | Frames between keys (1). |
| `hierarchy` | `hi` | none or below. |
| `preserveOutsideKeys` | `pok` | Keep keys outside the baked range. |
| `simulation` | `sm` | Accepted for Maya scripts (always evaluated in time order). |
| `disableImplicitControl` | `dic` | Accepted for Maya scripts. |
| `sparseAnimCurveBake` | `sac` | Accepted for Maya scripts. |
| `minimizeRotation` | `mr` | Accepted for Maya scripts. |
| `controlPoints` | `cp` | Accepted for Maya scripts. |
| `shape` | `s` | Accepted for Maya scripts. |

## bevelApplyEdges

Store the selected edges as an autoBevel modifier's edge set (as overrides of its rule) and end Edit Edges. Returns {added, removed}.

Arguments: modifier or mesh

## bevelCancelEdges

End an autoBevel modifier's Edit Edges without changing its edge set.

Arguments: modifier or mesh

## bevelClearOverrides

Drop an autoBevel modifier's hand-picked edges, going back to its rule.

Arguments: modifier or mesh

## bevelEdgeInfo

The edges an autoBevel modifier picks on its input mesh: {edges, count, lost, editing}.

Arguments: modifier or mesh

## bevelEditEdges

Show an autoBevel modifier's input mesh (it and later modifiers pass through) with its edges selected, so they can be changed with the normal selection tools. Finish with bevelApplyEdges or bevelCancelEdges.

Arguments: modifier or mesh

## bevelMeasureThin

Set an autoBevel modifier's thin to 1.5 times the longest selected edge (pick a thickness edge). Returns it.

Arguments: modifier or mesh

## blendShape

Blend shapes: the targets named first, the base mesh last (or selected the same way). Each target gets a weight on the blendShape node, named after it. Returns [blendShape].

Arguments: targets, then the base

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the blendShape node. |

## camera

Create a camera, or query and edit one: -fl focal length, -hfa -vfa film, -ncp -fcp clips, -o orthographic, -ow width, -coi, -p position, -rot rotation, -wci world centre of interest. Returns [camera1, cameraShape1].

Arguments: a camera, with -q and -e

| Flag | Short | Description |
| --- | --- | --- |
| `focalLength` | `fl` | Focal length, mm. |
| `horizontalFilmAperture` | `hfa` | Film width, inches. |
| `verticalFilmAperture` | `vfa` | Film height, inches. |
| `nearClipPlane` | `ncp` | Near clip. |
| `farClipPlane` | `fcp` | Far clip. |
| `orthographic` | `o` | No perspective. |
| `orthographicWidth` | `ow` | View width when orthographic. |
| `centerOfInterest` | `coi` | Distance to what it tumbles about. |
| `lensSqueezeRatio` | `lsr` | Lens squeeze. |
| `position` | `p` | World position. |
| `rotation` | `rot` | Rotation, degrees. |
| `worldCenterOfInterest` | `wci` | The point it looks at. |
| `worldUp` | `wup` | Accepted for Maya scripts. |
| `name` | `n` | Name. |
| `query` | `q` | Query mode. |
| `edit` | `e` | Edit mode. |

## centerPivot

Move pivots to the centre of each object's bounding box (mesh points, curve points and handles, and everything parented under it, so a group centres on its children) without moving the object.

Arguments: objects

## circle

Create a NURBS circle, with a makeNurbCircle node (-nr normal, -c centre, -r radius, -d degree, -s sections, -sw sweep). Returns [nurbsCircle1, makeNurbCircle1].

| Flag | Short | Description |
| --- | --- | --- |
| `normal` | `nr` | The circle's normal (0 0 1). |
| `center` | `c` | The centre. |
| `firstPoint` | `fp` | Toward the first section's start. |
| `radius` | `r` | Radius (1). |
| `degree` | `d` | Degree (3). |
| `sections` | `s` | Sections (8). |
| `sweep` | `sw` | Degrees covered (360). |
| `useTolerance` | `ut` | Accepted for Maya scripts. |
| `tolerance` | `tol` | Accepted for Maya scripts. |
| `constructionHistory` | `ch` | Keep the node (on by default). |
| `object` | `o` | Accepted for Maya scripts. |
| `name` | `n` | Name of the transform. |

## closeCurve

Close a curve. Returns [curve, node].

Arguments: curves (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `preserveShape` | `ps` | Accepted for Maya scripts. |
| `blendBias` | `bb` | Accepted for Maya scripts. |
| `constructionHistory` | `ch` | Keep a history node (on by default). |
| `replaceOriginal` | `rpo` | Change the curve itself (on by default). |
| `name` | `n` | Name of a new curve. |
| `object` | `o` | Make an object (accepted for Maya scripts). |

## cluster

Make a cluster of the selected vertices (or whole meshes): a handle they follow. Returns [cluster, handle].

Arguments: meshes or vertices (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the cluster; its handle takes the name plus Handle. |

## componentInfo

Positions, normals and adjacency of a mesh's components in bulk. Faces: center, normal, area, points, edges, neighbors (faces across each edge). Edges: points, center, length, faces, border. Vertices: position, normal, edges, neighbors (points), faces. World space unless space='object'. ids= picks some (default: the selected ones of that kind, or all).

Arguments: [mesh]

| Flag | Short | Description |
| --- | --- | --- |
| `kind` | `k` | vertex, edge or face. |
| `ids` |  | Component ids; [first, last] pairs are ranges. |
| `space` |  | world or object. |
| `fields` |  | Keys to keep per component, e.g. ['id', 'center', 'normal'] (default all). |

## componentSelection

Selected components: {mesh: {"vertices": [...], "edges": [...], "faces": [...]}}. ordered=True gives a flat list [[mesh, kind, id], ...] in pick order instead (kind is vertex, edge or face).

| Flag | Short | Description |
| --- | --- | --- |
| `ordered` |  | True: a flat list [[mesh, kind, id], ...] in the order the components were picked. False: grouped per mesh and kind, ids sorted ascending, pick order lost. |

## connectAttr

connectAttr('src.attr', 'dst.attr'). A whole array input ('loft1.sections') gets a new last element. Returns the destination plug.

Arguments: src dst

| Flag | Short | Description |
| --- | --- | --- |
| `force` | `f` | Replace an existing input connection. |

## constructionHistory

Turn construction history on or off for later poly edits, or query it. Returns whether it is on.

| Flag | Short | Description |
| --- | --- | --- |
| `toggle` | `tgl` | Turn it on if off, off if on. |
| `state` | `st` | Turn it on (True) or off (False). |

## convertSelection

Convert the component selection to another kind and switch to that mode. In object mode the selected meshes convert as a whole: every component of that kind.

Arguments: to

## convertToInstance

Replace meshes identical to a source with instances of it, dropping their own copies of the data: cleans up imported kits. The source is the last mesh given (the lead); with only the source, every other mesh in the scene is checked. Returns the meshes converted.

Arguments: meshes... source

| Flag | Short | Description |
| --- | --- | --- |
| `tolerance` |  | Points may differ by this much, object space, cm. |
| `force` |  | Convert the given meshes even when their geometry differs from the source's. |

## copyKey

Copy keys of objects or channels to the key clipboard; -t limits them. Returns how many.

Arguments: objects, plugs or curves (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `attribute` | `at` | Channel name(s). |
| `time` | `t` | A frame or range "10:20". |
| `option` | `o` | keys (the default) or curve. |
| `hierarchy` | `hi` | Accepted for Maya scripts. |
| `clipboard` | `cb` | Accepted for Maya scripts. |

## copySkinWeights

Copy weights between skins, or mirror them on one skin: copySkinWeights -ss skinCluster1 -ds skinCluster2 -nm, or copySkinWeights -ss skinCluster1 -ds skinCluster1 -mm YZ -mi. Selected points limit what the destination takes.

Arguments: source then destination (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `sourceSkin` | `ss` | Source skinCluster. |
| `destinationSkin` | `ds` | Destination skinCluster. |
| `mirrorMode` | `mm` | Mirror plane: YZ (default), XZ or XY. |
| `mirrorInverse` | `mi` | Copy from the negative side to the positive. |
| `noMirror` | `nm` | A plain copy. |
| `surfaceAssociation` | `sa` | closestPoint (default), rayCast or closestComponent. |
| `influenceAssociation` | `ia` | closestJoint (default), closestBone, oneToOne, label or name, tried in turn. |
| `normalize` | `nr` | Normalize the copied weights (default on). |
| `sampleSpace` | `spa` | Accepted for Maya scripts. |

## createDisplayLayer

Create a display layer holding the objects (the selection by default), taking them out of any layer they were in. Returns its name.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name; layer1 when empty, made unique. |
| `empty` | `e` | Make it with no members, whatever is selected. |

## createMesh

Make a mesh from points and faces (lists of point indices, any size). Returns its name.

| Flag | Short | Description |
| --- | --- | --- |
| `points` |  | [[x, y, z], ...] in cm. |
| `faces` |  | [[i, j, k, ...], ...]. |
| `uvs` |  | [[u, v], ...]: one per face corner, face by face in faces' order. Fills uv0. |
| `name` | `n` | Name; made unique. |

## createNode

Create a node of the given type. Returns its path (the name, unless another node shares it).

Arguments: type

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name; made unique among its siblings. |
| `parent` | `p` | Parent transform. |
| `skipSelect` | `ss` | Don't select the new node. |

## createSet

Create an object set holding the members (the selected components in component mode, else the selected objects). Members are object names or components like pCube1.vtx[0:3], pCube1.e[5], pCube1.f[2]. Returns its name.

Arguments: members

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name; set1 when empty, made unique. |
| `quickSelect` | `qs` | Make it a quick select set (Edit > Quick Select Sets). |
| `empty` | `e` | Make it with no members, whatever is selected. |

## currentTime

Set the current frame, or query it. Animated channels move to their values there.

Arguments: frame

| Flag | Short | Description |
| --- | --- | --- |
| `query` | `q` | Return the value of the flags named instead of setting them. |
| `edit` | `e` | Change the flags named. |
| `time` | `t` | The frame to go to. |
| `update` | `u` | Accepted for Maya scripts; the scene always updates. |

## curve

Create a NURBS curve: curve -d 3 -p 0 0 0 -p 1 0 0 -p 2 1 0 -p 3 1 0. -k gives knots, -per a periodic curve, -ep edit points to pass through, -bezier takes Bezier points. Given a curve: -a appends the points, -r replaces it. Returns its transform.

Arguments: a curve, with -a or -r

| Flag | Short | Description |
| --- | --- | --- |
| `degree` | `d` | Degree (3). |
| `point` | `p` | A CV: x y z. Repeat it. |
| `editPoint` | `ep` | An edit point to pass through: x y z. Repeat it. |
| `knot` | `k` | A knot. Repeat it; CVs + degree - 1 of them. |
| `periodic` | `per` | A closed, periodic curve. |
| `bezier` | `bez` | The points are Bezier points. |
| `append` | `a` | Add the points to the curve named. |
| `replace` | `r` | Replace the curve named. |
| `worldSpace` | `ws` | Accepted for Maya scripts. |
| `objectSpace` | `os` | Accepted for Maya scripts. |
| `name` | `n` | Name of the transform. |

## cutKey

Delete keys in -t (or all of them) from objects or channels. Curves left with no keys are deleted, leaving the channel at its value.

Arguments: objects, plugs or curves (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `attribute` | `at` | Channel name(s). |
| `time` | `t` | A frame or range "10:20". |
| `clear` | `cl` | Delete without copying to the clipboard. |
| `option` | `o` | Accepted for Maya scripts. |
| `index` | `in` | Accepted for Maya scripts. |

## dagPose

Saved poses of skeletons. -restore -bp returns the joints named to their bind pose; -save stores a pose; -q -bp lists the bind poses of the joints.

Arguments: joints or a dagPose (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `restore` | `r` | Return to the pose. |
| `save` | `s` | Save a new pose. |
| `bindPose` | `bp` | The bind pose. |
| `global` | `g` | The whole pose, not only what's below the joints named. |
| `name` | `n` | Name of a saved pose. |
| `edit` | `e` | Edit mode. |
| `query` | `q` | Query mode. |

## deformer

Make a deformer: deformer -type ffd [-n name] objects, with ffd, cluster, softMod, deltaMush, tension, wrap, wire or a nonlinear type. Returns [node]. -q -g gives the deformed shapes, -q -gi their indices. -e -g obj adds geometry, -e -rm -g obj takes it off.

Arguments: objects to deform (the selection when none); the deformer with -e and -q

| Flag | Short | Description |
| --- | --- | --- |
| `edit` | `e` | Edit mode. |
| `query` | `q` | Query mode. |
| `type` | `typ` | The node type to make. |
| `name` | `n` | Name of the deformer. |
| `geometry` | `g` | Geometry to deform, add or remove. Query: the deformed shapes. |
| `geometryIndices` | `gi` | Query: the indices of the deformed shapes. |
| `remove` | `rm` | With -e -g: take the geometry off the deformer. |
| `before` | `bf` | Go just above the deformer nearest the mesh. |
| `after` | `af` | Go nearest the mesh (the default). |
| `frontOfChain` | `foc` | Go above every other deformer. |

## delete

Delete objects (and their children). Defaults to the selection.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `constructionHistory` | `ch` | Delete the objects' construction history instead: each mesh keeps what its history makes now. |

## deleteAttr

Delete attributes added with addAttr: deleteAttr('node.attr'), deleteAttr(objects, attribute=name), or a whole component with deleteAttr(objects, category=name), which also takes the name out of the object's usdApiSchemas.

Arguments: plugs or objects

| Flag | Short | Description |
| --- | --- | --- |
| `attribute` | `at` | The attribute to delete from each object. |
| `category` |  | Delete every added attribute under this Attribute Editor heading. |

## deleteComponents

Delete the selected vertices, edges or faces (with whatever depends on them).

## deleteDisplayLayer

Delete display layers; their objects stay, in no layer.

Arguments: layers

## deleteSet

Delete sets; their members stay.

Arguments: sets

## deltaMush

Add a delta mush after the mesh's other deformers: deltaMush -si 10 -ss 0.5. The mesh as it is now is the rest pose. Returns [deltaMush1].

Arguments: meshes or vertices (the selection when none); the deformer with -e and -q

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the deformer. |
| `before` | `bf` | Go just above the deformer nearest the mesh. |
| `after` | `af` | Go nearest the mesh, after the other deformers (the default). |
| `frontOfChain` | `foc` | Go above every other deformer. |
| `envelope` | `en` | How much of the deformation applies (1). |
| `edit` | `e` | Edit mode. |
| `query` | `q` | Query mode. |
| `smoothingIterations` | `si` | Smoothing passes (10). |
| `smoothingStep` | `ss` | How far each pass moves a point towards its neighbours (0.5). |
| `pinBorderVertices` | `pbv` | Border points stay out of the smoothing (on). |
| `pin` | `pin` | The same as -pinBorderVertices. |
| `inwardConstraint` | `ic` | Stops points moving in along the normal: 0 free, 1 not at all. |
| `outwardConstraint` | `oc` | Stops points moving out along the normal: 0 free, 1 not at all. |
| `displacement` | `dis` | How much of the rest pose's detail goes back on (1). |

## detachCurve

Cut a curve at -p (repeat it). With -rpo the first piece replaces it. Returns the pieces' transforms.

Arguments: a curve (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `parameter` | `p` | Where to cut. Repeat it. |
| `replaceOriginal` | `rpo` | The first piece replaces the curve. |
| `keep` | `k` | Accepted for Maya scripts. |
| `constructionHistory` | `ch` | Accepted for Maya scripts. |

## disconnectAttr

disconnectAttr('dst.attr'), or (src, dst). A whole array input loses every element, or with src only the ones src drives.

Arguments: [src] dst

## displayLayer

Edit a display layer's switches (V, P, display type, colour, name); with no flags, query it. Returns its info: name, visibility, hideOnPlayback, displayType, color (null for none), order and members.

Arguments: layer

| Flag | Short | Description |
| --- | --- | --- |
| `visibility` | `v` | Show or hide its objects. |
| `hideOnPlayback` | `hpb` | Hide its objects while the animation plays. |
| `displayType` | `dt` | normal, template or reference. |
| `color` | `c` | Wireframe tint, linear RGB; turns the tint on. |
| `useColor` | `uc` | Turn the wireframe tint on or off. |
| `name` | `n` | Rename the layer; made unique. |

## duplicate

Duplicate objects with their children, keeping each mesh's generator and modifier stack live. Returns the new top-level names. count makes a run of copies, copy i placed by the translate/rotate/scale step applied i times about the pivot: a row, a ring or a spiral in one call. mode instance shares the mesh (edits to one change all); reference shares the base mesh but gives each copy its own modifier stack.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name for a single duplicate. |
| `count` | `c` | Copies of each object, 1 to 10000. |
| `translate` | `t` | Move from one copy to the next, cm. |
| `rotate` | `r` | Rotation from one copy to the next about the pivot, degrees (XYZ). |
| `scale` | `s` | Scale from one copy to the next about the pivot. |
| `pivot` |  | What the step rotates and scales about. |
| `pivotPoint` |  | World point rotated and scaled about when pivot is point, cm. |
| `space` |  | Axes the step is measured along: the world's or each object's own. |
| `mode` | `m` | What a duplicated mesh shares with its source: copy shares nothing. instance shares the mesh and its whole modifier stack: every copy looks the same and exports as one mesh. reference shares only the base mesh and keeps a modifier stack of its own (same shape, a different bevel). Component edits on a shared mesh reach every copy. |
| `group` | `g` | Put the copies under a new group. |

## duplicateModifier

Copy a modifier with its settings and insert the copy right after it in the stack. Returns the copy's name.

Arguments: modifier

## duplicateWithTransform

Duplicate the last duplicated object again, placed by how its copy now sits relative to its source. Duplicate an object, move, rotate or scale the copy, then repeat this to continue the run. Returns the new names. The step is measured from the current world matrices of the last source and copy, so it follows any later edit to the copy and survives selection changes. It is forgotten when either object is deleted.

| Flag | Short | Description |
| --- | --- | --- |
| `count` | `c` | Copies to add, 1 to 10000. |

## editDisplayLayerMembers

Put objects (the selection by default) in a display layer, moving them out of their old one. Naming defaultLayer takes them out of any layer. Returns the layer's members.

Arguments: layer, objects

## editKeys

Edit keys picked by curve and index: -dt/-dv move them, -t/-v set them, -itt/-ott change their tangents, -delete removes them. Keys are [[curve, index], ...].

| Flag | Short | Description |
| --- | --- | --- |
| `keys` | `k` | The keys: [[curve, index], ...]. |
| `timeOffset` | `dt` | Frames to move them by. |
| `valueOffset` | `dv` | Value to move them by. |
| `time` | `t` | Frame to put them at. |
| `value` | `v` | Value to give them. |
| `inTangentType` | `itt` | Tangent into them. |
| `outTangentType` | `ott` | Tangent out of them. |
| `breakdown` | `bd` | Make them breakdowns, or normal keys. |
| `delete` | `del` | Remove them. |

## exportObj

Export meshes to an OBJ file in world space. Defaults to the selection.

Arguments: path objects...

## exportPly

Export meshes to PLY in world space, merged into one mesh, defaulting to the selection and the meshes under it: polygons as they are, normals, uv0 and the current color set per vertex (points split where those differ). Loose points and edges are written too. Returns the path.

Arguments: path objects...

| Flag | Short | Description |
| --- | --- | --- |
| `unit` |  | The file's length unit. |
| `upAxis` |  | The file's up axis; converted to the scene's. |
| `binary` |  | Binary little endian; off writes ASCII. |
| `normals` |  | Write normals. |
| `uvs` |  | Write uv0 as s, t. |
| `colors` |  | Write the current color set as 8-bit RGBA. |

## exportStl

Export meshes to STL in world space, triangulated, defaulting to the selection and the meshes under it. Binary merges them into one solid; ASCII writes a solid per mesh. Returns the path.

Arguments: path objects...

| Flag | Short | Description |
| --- | --- | --- |
| `unit` |  | The file's length unit. 3D printing assumes mm. |
| `upAxis` |  | The file's up axis; converted to the scene's. |
| `binary` |  | Binary STL; off writes ASCII. |
| `colors` |  | Binary only: the current color set as per-triangle VisCAM colors. |

## extendCurve

Lengthen a curve along its tangent: -d distance, -s 0 end, 1 start, 2 both. Returns [curve, node].

Arguments: curves (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `distance` | `d` | How far. |
| `start` | `s` | 0 the end, 1 the start, 2 both. |
| `extensionType` | `et` | Accepted for Maya scripts. |
| `constructionHistory` | `ch` | Keep a history node (on by default). |
| `replaceOriginal` | `rpo` | Change the curve itself (on by default). |
| `name` | `n` | Name of a new curve. |
| `object` | `o` | Make an object (accepted for Maya scripts). |

## extrude

Sweep a profile curve along a path curve (or -d -l a direction and length): extrude profile path -et 2 -sc 1 -ro 0. Returns [extrudedSurface1, extrude1].

Arguments: the profile, then the path (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `extrudeType` | `et` | 0 distance, 1 flat, 2 tube (2). |
| `scale` | `sc` | Scale at the path's end. |
| `rotation` | `ro` | Twist at the path's end, degrees. |
| `direction` | `d` | Direction, with no path. |
| `length` | `l` | Length, with no path. |
| `divisions` | `dv` | Mesh points per span of the profile. |
| `fixedPath` | `fpt` | Accepted for Maya scripts. |
| `useComponentPivot` | `ucp` | Accepted for Maya scripts. |
| `useProfileNormal` | `upn` | Accepted for Maya scripts. |
| `reverseSurfaceIfPathReversed` | `rsp` | Accepted for Maya scripts. |
| `polygon` | `po` | Accepted: the output is always polygons. |
| `constructionHistory` | `ch` | Keep the node (on by default). |
| `range` | `rn` | Accepted for Maya scripts. |
| `name` | `n` | Name of the new object. |

## file

Scene files: -new, -open path, -save, -rename name, -import path, -exportAll path, -exportSelected path; -type mayaAscii. In query mode -sceneName (with -shortName for the file alone), -exists, -type, -modified.

Arguments: the file

| Flag | Short | Description |
| --- | --- | --- |
| `new` | `new` | A new, empty scene. |
| `open` | `o` | Replace the scene with the file. |
| `save` | `s` | Write the scene under its name. |
| `rename` | `rn` | Set the scene's name, for the next save. |
| `import` | `i` | Add the file's contents to the scene. |
| `exportAll` | `ea` | Write the whole scene to the file. |
| `exportSelected` | `es` | Write the selection, what lies below it, its parents and the skeletons of its skins. |
| `type` | `typ` | mayaAscii, or aztek for Aztek's own format. |
| `options` | `op` | File translator options. Accepted for Maya scripts. |
| `force` | `f` | Don't ask about unsaved changes. Nothing asks here. |
| `query` | `q` | Query mode. |
| `sceneName` | `sn` | Query: the scene's path; empty when untitled. |
| `shortName` | `shn` | Query: with -sceneName, the file name alone. |
| `exists` | `ex` | Query: whether the file (or the scene's file) exists. |
| `modified` | `mf` | Query: whether the scene has edits. |
| `reference` | `r` | Not supported; refused. |
| `loadReference` | `lr` | Not supported; refused. |
| `prompt` | `pmt` | Accepted for Maya scripts. |
| `ignoreVersion` | `iv` | Accepted for Maya scripts. |
| `preserveReferences` | `pr` | Accepted for Maya scripts. |
| `constructionHistory` | `ch` | Accepted for Maya scripts; history is baked in .ma. |
| `channels` | `chn` | Accepted for Maya scripts. |
| `constraints` | `con` | Accepted for Maya scripts. |
| `expressions` | `exp` | Accepted for Maya scripts. |
| `shader` | `sh` | Accepted for Maya scripts. |
| `returnNewNodes` | `rnn` | Accepted for Maya scripts. |
| `namespace` | `ns` | Accepted for Maya scripts. |

## filterExpand

The selection (or arguments) flattened and filtered by selection mask: -sm 31 vertices, 32 edges, 34 faces, 12 meshes. Returns None when nothing matches, as Maya does.

Arguments: items (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `selectionMask` | `sm` | Mask number(s). |
| `expand` | `ex` | One item per component (default on). |

## findComponents

Ids of a mesh's vertices, edges or faces matching every predicate given (normal direction, side of the symmetry plane, box, distance, extreme along an axis, open border). Ids index the editable cage, like selectComponents. Changes nothing; see selectComponentsWhere to select them.

Arguments: [mesh]

| Flag | Short | Description |
| --- | --- | --- |
| `kind` | `k` | vertex, edge or face. |
| `normal` | `n` | Keep components whose normal is within angle of this direction (world space). |
| `angle` |  | Tolerance for normal=, degrees. |
| `side` | `s` | '+x', '-x', '+y', ... keeps components whose centre is on that side of the object-space origin plane; 'center' (or 'centerx', 'centery', 'centerz') keeps those on the plane. |
| `box` | `b` | [[minX, minY, minZ], [maxX, maxY, maxZ]], world space: keep components whose centre is inside. None leaves that side open, so [[None, 20, None], [None, None, None]] is everything above y=20. |
| `direction` | `dir` | Edges only: keep edges running along this world direction (either way) within angle; (0, 1, 0) finds vertical edges, whose ring polyInsertLoop cuts with a horizontal loop. |
| `near` |  | World point; with radius= keeps components within it, else keeps the count nearest. |
| `radius` | `r` | Distance for near=, cm. |
| `extreme` | `x` | '+y', '-z', ...: keep the count components furthest along that world direction (the top face is extreme='+y'). |
| `count` | `c` | How many to keep for extreme= or near= without radius. |
| `border` |  | True keeps only components on an open border, False only interior ones. |
| `shell` |  | Keep components of one connected shell: 0 is the largest, numbered as meshCheck lists them. |
| `fromSelection` | `fs` | Only consider the currently selected components of that kind. |
| `tolerance` | `t` | Distance counted as on a plane for side=, cm (default scales with the mesh). |

## findKeyframe

The frame of the next, previous, first or last key on the objects (the selection when none).

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `which` | `w` | next, previous, first or last. |
| `time` | `t` | Search from this frame instead of the current one. |

## findRelatedSkinCluster

The skinCluster on a mesh or its transform, or an empty string.

Arguments: a mesh or its transform

## fitBspline

A cubic through the curve's CVs. Returns [curve, node].

Arguments: curves (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `tolerance` | `tol` | Accepted for Maya scripts. |
| `constructionHistory` | `ch` | Keep a history node (on by default). |
| `replaceOriginal` | `rpo` | Change the curve itself (on by default). |
| `name` | `n` | Name of a new curve. |
| `object` | `o` | Make an object (accepted for Maya scripts). |

## flushUndo

Clear undo history. which='past' drops the steps undo would revert, so undo cannot step back past this point; 'future' drops the redo steps; 'all' drops both.

| Flag | Short | Description |
| --- | --- | --- |
| `which` | `w` | past, future or all. |

## freezeTransforms

Bake translate/rotate/scale into mesh or curve points, leaving identity values. World positions don't change. With hierarchy (the default, like Maya) everything parented under the objects is frozen too; without it the children's transforms absorb what their parent gives up. Joints keep their translate and take their rotation into jointOrient, or with jointOrient line up with the world.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `apply` |  | Maya's makeIdentity -a: bake the transform into the geometry (always on). |
| `normal` | `n` | Accepted for Maya scripts. |
| `preserveNormals` | `pn` | Accepted for Maya scripts. |
| `translate` | `t` | Freeze translation. |
| `rotate` | `r` | Freeze rotation. |
| `scale` | `s` | Freeze scale. |
| `hierarchy` |  | Freeze everything parented under the objects too. |
| `jointOrient` |  | Also line joints' axes up with the world, zeroing their orientation. |

## getAttr

Value of 'node.attr'; a whole array gives its elements' values in index order. lock=True returns whether it is locked instead; multiIndices=True an array's element indices.

Arguments: plug

| Flag | Short | Description |
| --- | --- | --- |
| `lock` | `l` | Return the lock state instead of the value. |
| `multiIndices` | `mi` | Return an array's element indices, ascending. |

## group

Group objects under a new transform, or create an empty group with empty=True.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Group name. |
| `empty` | `em` | Create an empty group. |
| `parent` | `p` | Parent for the new group. |

## growSelection

Grow the component selection by one ring of neighbours: points to the points of their faces, edges and faces to those sharing a point with the selection.

## hide

Hide objects (visibility off). Defaults to the selection.

Arguments: objects

## ikHandle

Create an IK handle from -sj to -ee (or two selected joints). The RP solver bends the middle toward the pole vector; SC aims the chain. Returns [handle, effector].

Arguments: start and end joints (the selection when -sj/-ee are not given)

| Flag | Short | Description |
| --- | --- | --- |
| `startJoint` | `sj` | First joint of the chain. |
| `endEffector` | `ee` | Last joint of the chain. |
| `solver` | `sol` | ikRPsolver (default) or ikSCsolver. |
| `name` | `n` | Name of the handle. |

## importObj

Import an OBJ file; one mesh per object. Returns the new mesh names.

Arguments: path

## importPly

Import a PLY file (ASCII or binary, either byte order) as one mesh: polygons with normals, UVs (per vertex or MeshLab's per-corner texcoord), vertex colors as colorSet and face colors as faceColor. Other numeric vertex and face properties (quality, intensity, confidence...) become float attributes of the same name. A file with only vertices comes in as a point cloud: a mesh of loose points. Returns the new mesh names, or with report=True a dict of nodes, pointCloud, textureFiles and skipped.

Arguments: path

| Flag | Short | Description |
| --- | --- | --- |
| `unit` |  | The file's length unit. Scanners and photogrammetry write metres. |
| `upAxis` |  | The file's up axis; converted to the scene's. |
| `normals` |  | Keep the file's normals. |
| `report` |  | Return the import report instead of just the names. |

## importStl

Import an STL file, binary or ASCII: one mesh per solid. Triangles sharing positions weld into a connected mesh, edges bent past hardAngle are hard, and binary per-triangle colors (VisCAM and Materialise) become a color set. Returns the new mesh names.

Arguments: path

| Flag | Short | Description |
| --- | --- | --- |
| `unit` |  | The file's length unit. STL has none; 3D printing assumes mm. |
| `upAxis` |  | The file's up axis; converted to the scene's. |
| `weld` |  | Merge identical positions into shared points. |
| `hardAngle` |  | Edges bent more than this many degrees are hard; -1 leaves all soft. |

## insertKnotCurve

Add knots at -p: curve keeps its shape. Returns [curve, node].

Arguments: curves (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `parameter` | `p` | Where. |
| `numberOfKnots` | `nk` | How many. |
| `addKnots` | `add` | Accepted for Maya scripts. |
| `insertBetween` | `ib` | Accepted for Maya scripts. |
| `constructionHistory` | `ch` | Keep a history node (on by default). |
| `replaceOriginal` | `rpo` | Change the curve itself (on by default). |
| `name` | `n` | Name of a new curve. |
| `object` | `o` | Make an object (accepted for Maya scripts). |

## instance

Make instances of objects: meshes that share their source's mesh, so an edit to any of them changes all. Returns the new names. reference=True makes references instead, which share the base mesh under the source's modifiers but keep a modifier stack of their own (same base, a different bevel).

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name for a single instance. |
| `reference` |  | Share only the base mesh and keep a modifier stack of its own (same shape, a different bevel). Off, instances share the modifier stack too, so every copy looks the same and exports as one mesh. |

## instanceInfo

How a mesh shares data: {kind: none | source | instance | reference, source, members}.

Arguments: mesh

## invertSelection

Invert the selection: the unselected components of the current kind on the hilited meshes, or in object mode every unselected object. An isolated view inverts only what it shows.

## joint

Create a joint at -p (world), under the selected joint, and select it. With -e: -oj orients joints (xyz...), -sao the secondary axis (yup...), -ch their children too, -zso zeroes scale orient. -q -p returns the position.

Arguments: joints (edit and query)

| Flag | Short | Description |
| --- | --- | --- |
| `position` | `p` | World position (or offset from the parent joint with -r). |
| `relative` | `r` | -p is relative to the parent joint. |
| `absolute` | `a` | -p is a world position (the default). |
| `name` | `n` | Name of the new joint. |
| `radius` | `rad` | Display size. |
| `orientation` | `o` | jointOrient of the new joint, degrees. |
| `orientJoint` | `oj` | Edit: aim axis order, xyz, yzx... or none. |
| `secondaryAxisOrient` | `sao` | Edit: where the second axis points, yup, zup, xdown... |
| `children` | `ch` | Edit: the joints below too. |
| `zeroScaleOrient` | `zso` | Accepted for Maya scripts. |
| `edit` | `e` | Change existing joints. |
| `query` | `q` | Query a joint (its world position). |

## keyTangent

Set the tangent types of keys (-itt, -ott; -t limits them), their angles (-ia, -oa, degrees), or with -g the defaults new keys get.

Arguments: objects, plugs or curves (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `query` | `q` | Return the value of the flags named instead of setting them. |
| `edit` | `e` | Change the flags named. |
| `attribute` | `at` | Channel name(s). |
| `time` | `t` | A frame or range "10:20". |
| `inTangentType` | `itt` | Tangent into the keys. |
| `outTangentType` | `ott` | Tangent out of the keys. |
| `inAngle` | `ia` | In tangent angle, degrees (makes it fixed). |
| `outAngle` | `oa` | Out tangent angle, degrees (makes it fixed). |
| `lock` | `l` | Keep in and out tangents together. |
| `global` | `g` | The default tangents for new keys. |

## keyTimes

Frames that have keys on the objects (the selection when none), for the time slider.

Arguments: objects

## keyframe

Query or edit keys. -q -kc counts them, -q -tc lists their frames, -q -vc their values, -q -n the curves. -tc/-vc (with -r for relative) move the keys in -t (or all).

Arguments: objects, plugs or curves (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `query` | `q` | Return the value of the flags named instead of setting them. |
| `edit` | `e` | Change the flags named. |
| `attribute` | `at` | Channel name(s). |
| `time` | `t` | A frame or range "10:20". |
| `floatChange` | `fc` | Driven keys: the driver value, as -tc for time keys. |
| `breakdown` | `bd` | Query: the breakdown keys' frames. Edit: make keys breakdowns, or not. |
| `index` | `in` | Key index(es) on each curve. |
| `keyframeCount` | `kc` | Query: how many keys. |
| `timeChange` | `tc` | Query: the keys' frames. Edit: their new frame (or offset with -r). |
| `valueChange` | `vc` | Query: the keys' values. Edit: their new value (or offset with -r). |
| `relative` | `r` | Edits are offsets. |
| `absolute` | `a` | Edits are new values (the default). |
| `option` | `o` | Accepted for Maya scripts. |
| `name` | `n` | Query: the curves' names. |
| `eval` | `ev` | Query: the curves' values at the current frame. |
| `selected` | `sl` | Accepted for Maya scripts. |

## lattice

Add a lattice to the selected meshes or vertices: lattice -dv 2 5 2 -oc true. Returns [ffd1, ffd1Lattice, ffd1Base]. Move ffd1Lattice.pt[s][t][u] to deform. With -e on the ffd or lattice: -dv, -ldv, -ol, -ofd, -fm, -remap, -resetLattice. With -q: -dv, -ldv, -ol, -ofd, -fm, -g.

Arguments: meshes or vertices (the selection when none); the ffd or lattice with -e and -q

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the deformer. |
| `before` | `bf` | Go just above the deformer nearest the mesh. |
| `after` | `af` | Go nearest the mesh, after the other deformers (the default). |
| `frontOfChain` | `foc` | Go above every other deformer. |
| `envelope` | `en` | How much of the deformation applies (1). |
| `edit` | `e` | Edit mode. |
| `query` | `q` | Query mode. |
| `divisions` | `dv` | Control points in S, T and U (2 2 2). |
| `ldivisions` | `ldv` | Local influence in S, T and U: how many control points move a point (2 2 2). |
| `objectCentered` | `oc` | Fit the lattice round the geometry (on). Off, a unit lattice at the origin. |
| `outsideLattice` | `ol` | 0 only points inside deform, 1 all of them, 2 falling off with distance. |
| `outsideFalloffDist` | `ofd` | With -ol 2: how far the effect reaches, in lattice widths. |
| `freezeMapping` | `fm` | Hold each point's place in the lattice from now on, wherever it goes. |
| `position` | `pos` | Where to put the lattice. |
| `rotation` | `ro` | Rotation of the lattice, degrees. |
| `scale` | `s` | Size of the lattice. |
| `remap` | `rmp` | With -e: take the frozen mapping again from where the points are now. |
| `resetLattice` | `rl` | With -e: put the control points back on the regular grid. |
| `latticeReset` | `ltr` | With -e: the same as -resetLattice. |
| `removeTweaks` | `rt` | With -e: the same as -resetLattice. |
| `geometry` | `g` | Query: the deformed shapes. |

## listAttr

Attribute names of a node.

Arguments: object

| Flag | Short | Description |
| --- | --- | --- |
| `keyable` | `k` | Only keyable attributes. |
| `locked` | `l` | Only locked plugs; a Vec3 lists each locked component (translateX). |
| `userDefined` | `ud` | Only attributes added with addAttr. |

## listConnections

Nodes (or plugs with -p) connected to the objects or plugs: -s sources, -d destinations (both by default), -type filters by node type, -c pairs each with the plug on this side.

Arguments: objects or plugs

| Flag | Short | Description |
| --- | --- | --- |
| `source` | `s` | Include upstream connections (default on). |
| `destination` | `d` | Include downstream connections (default on). |
| `plugs` | `p` | Return plugs rather than nodes. |
| `connections` | `c` | Pair each with the plug on this side. |
| `type` | `t` | Only nodes of this type. |
| `skipConversionNodes` | `scn` | Accepted for Maya scripts. |

## listDisplayLayers

Every display layer in panel order, each as displayLayer returns it.

## listHistory

The nodes feeding an object's mesh, nearest first: history nodes, then the generator or intermediate shape at the bottom.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `type` | `typ` | Only nodes of this type or a type derived from it (geometryFilter for every deformer). |

## listRelatives

Children (default), parent or all descendants.

Arguments: object

| Flag | Short | Description |
| --- | --- | --- |
| `parent` | `p` |  |
| `allDescendents` | `ad` |  |

## listSets

Every object set's name in creation order. quickSelect=True lists only quick select sets, False only plain ones; object= lists the sets that hold that object. info=True returns {name, quickSelect, members} each.

| Flag | Short | Description |
| --- | --- | --- |
| `quickSelect` | `qs` | Only quick select sets (True) or only plain sets (False). |
| `object` | `o` | Only sets holding this object, whole or by its components. |
| `info` | `i` | Return each set as a dict instead of its name. |

## loft

A surface through curves in order: loft c1 c2 c3 -c 0. Returns [loftedSurface1, loft1].

Arguments: curves in order (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `close` | `c` | Back to the first curve. |
| `divisions` | `dv` | Mesh points per span of the first curve. |
| `uniform` | `u` | Accepted for Maya scripts. |
| `autoReverse` | `ar` | Accepted for Maya scripts. |
| `degree` | `d` | Accepted for Maya scripts. |
| `sectionSpans` | `ss` | Accepted for Maya scripts. |
| `polygon` | `po` | Accepted: the output is always polygons. |
| `constructionHistory` | `ch` | Keep the node (on by default). |
| `range` | `rn` | Accepted for Maya scripts. |
| `name` | `n` | Name of the new object. |

## ls

List nodes and components: names, wildcards (pCube*), components (pCube1.vtx[*]), filtered by the flags.

Arguments: names or patterns

| Flag | Short | Description |
| --- | --- | --- |
| `selection` | `sl` | The selection. |
| `orderedSelection` | `os` | The selection, in order. |
| `flatten` | `fl` | Components one per item. |
| `long` | `l` | Full DAG paths. |
| `shortNames` | `sn` | Shortest unique names (the default). |
| `type` | `typ` | Only nodes of this type or derived; a list of types too. |
| `exactType` | `et` | Only nodes of exactly this type. |
| `dag` | `dag` | Only DAG nodes. |
| `dagObjects` | `do` | DAG nodes and everything below them. |
| `shapes` | `s` | Only shapes. |
| `transforms` | `tr` | Only transforms. |
| `geometry` | `g` | Only geometry shapes. |
| `cameras` | `ca` | Only cameras. |
| `lights` | `lt` | Only lights. |
| `objectsOnly` | `o` | Objects, not their components. |
| `visible` | `v` | Only visible objects. |
| `invisible` | `iv` | Only hidden objects. |
| `assemblies` | `assemblies` | Only top level DAG nodes. |
| `leaf` | `lf` | Only nodes with no children. |
| `noIntermediate` | `ni` | No intermediate objects. |
| `intermediateObjects` | `io` | Only intermediate objects. |
| `hilite` | `hl` | The hilited objects. |
| `sets` | `set` | Only sets. |
| `showType` | `st` | Each name followed by its type. |
| `head` | `hd` | Only the first n. |
| `tail` | `tl` | Only the last n. |

## makeLive

Make a mesh the live surface: Quad Draw and the snap to live draw on it. -none clears it. -q asks.

Arguments: meshes (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `none` | `n` | Clear the live surface. |
| `query` | `q` | Query mode. |

## makeUnique

Give instances and references data of their own, so edits to them stop reaching the other members. An instance gets a live copy of its source's stack, a reference its own base mesh; a source hands its data to its first instance and keeps a copy. Returns the meshes changed.

Arguments: meshes

## matchPivot

Copy the first object's pivot onto the others, keeping them where they are. position=False keeps their pivot positions and orient=False their pivot orientations.

Arguments: objects (the first is the source)

| Flag | Short | Description |
| --- | --- | --- |
| `position` | `p` | Copy the pivot's world position. |
| `orient` | `o` | Copy the pivot's world axes. |

## meshCheck

Validate a model in one compact report: ok (no issues), shells with their world boxes (largest first), open border edges, non-manifold edges and vertices, zero-area faces, face/triangle counts, symmetry drift. issues says what is wrong and how to fix it. Changes nothing.

Arguments: [mesh]

| Flag | Short | Description |
| --- | --- | --- |
| `expectShells` |  | Connected parts the model should have (0 skips the check). |
| `maxTriangles` |  | Triangle budget; more is an issue. |

## meshData

A mesh's points and faces as plain lists: {points: [[x,y,z]...], faces: [[i,j,k,...]...]}. base=True reads the editable base mesh (under modifiers) instead of the final result.

Arguments: mesh

| Flag | Short | Description |
| --- | --- | --- |
| `base` |  | The base mesh, not the modified result. |
| `worldSpace` |  | Points in world space. |

## meshInfo

Counts and health of a mesh: points, edges, faces, triangles, tri/quad/ngon faces, open border edges and loops, non-manifold edges, shells, closed, object and world bounds, symmetry, smooth preview, modifiers, material slots (and the cage's counts under modifiers).

Arguments: object

## modifierKinds

Every modifier kind addModifier accepts, including ones extensions registered. docs=True returns {kind: description} instead.

| Flag | Short | Description |
| --- | --- | --- |
| `docs` |  | Return each kind's description too. |

## modifierStack

A mesh's stack as data: its source (generator, meshSource or null) and each modifier, applied first first, with name, type, enabled, exportEnabled and its last evaluation error.

Arguments: mesh

## modifiers

A mesh's modifier stack, bottom (applied first) to top.

Arguments: mesh

## move

Move objects. move(by=(x, y, z)) moves relative in `space`; move(to=(x, y, z)) puts each pivot at a world position. A selected parent and child move once. move(x=0) sets one world axis and leaves the others: each selected object's pivot, or with components selected every selected point, so move(x=0) puts an edge loop on the centre line.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `by` |  | Relative offset, cm. |
| `to` |  | Absolute world position of the pivot, cm. |
| `x` |  | World X for the pivot, or for every selected point, cm. |
| `y` |  | World Y, cm. |
| `z` |  | World Z, cm. |
| `space` |  | 'world', 'object' or 'parent' for `by`. |

## moveComponents

Move the selected components by an offset in world space, or with normal= push each point along its own vertex normal (a face selection uses only the selected faces' normals, so the region inflates as a whole). uvn= also slides each point along its own surface tangents: U follows the uv0 u direction where the mesh has uvs (in edge mode, the selected edges), else the world axis least aligned with the selection's normal, and V completes the frame. Curve points use the curve's frame: U its tangent (a handle's own direction), N across it within its plane, V out of the plane.

| Flag | Short | Description |
| --- | --- | --- |
| `by` |  | Offset, cm. |
| `normal` |  | Distance along the vertex normals, cm. |
| `uvn` |  | Distances along each point's own U tangent, V tangent and normal, cm. |
| `onto` |  | Meshes whose surface each moved point then lands on, at its closest point. |

## moveModifier

Move a modifier up or down its stack by a number of steps (positive is up, applied later).

Arguments: modifier

| Flag | Short | Description |
| --- | --- | --- |
| `by` |  | Steps. |

## movePivot

Move objects' pivots without moving the objects: -by a world offset, or -to a world position. Maya spells it move -r x y z obj.scalePivot obj.rotatePivot.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `by` | `r` | World offset. |
| `to` |  | World position. |

## namingConvention

Naming rules for this workspace (.aztek/naming.json). Query with no flags; check lists violations; fix renames them, whether or not enforcement is on. check and fix cover the given objects and everything under them, or the whole scene; referenced nodes keep their names.

Arguments: [objects]

| Flag | Short | Description |
| --- | --- | --- |
| `enable` | `e` | Turn enforcement on or off for the workspace (user prefs if none open). |
| `mode` | `m` | fix \| warn \| block |
| `uniqueNames` | `un` | Make names unique across the whole scene, not just among siblings (workspace, else user prefs). |
| `check` | `c` | Return [[node, expectedName], ...] for names that break the rules. |
| `fix` | `f` | Rename every node that breaks the rules. |

## newScene

Clear the scene. Not undoable.

## nodeType

Type name of a node.

Arguments: object

## nonLinear

Add a nonlinear deformer (-type bend, twist, squash, flare, sine or wave) to the selected meshes, with a handle fitted to them. Returns [deformer, handle].

Arguments: meshes (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the deformer; its handle takes the name plus Handle. |
| `type` | `typ` | bend, twist, squash, flare, sine or wave. |
| `curvature` | `cur` | bend: how far it bends. |
| `startAngle` | `sa` | twist: degrees at the low bound. |
| `endAngle` | `ea` | twist: degrees at the high bound. |
| `factor` | `f` | squash: stretch (positive) or squash (negative). |
| `amplitude` | `amp` | sine, wave: height of the waves. |
| `wavelength` | `wav` | sine, wave: length of the waves. |

## nurbsSquare

Create a square curve (-nr normal, -c centre, -sl1 side). Returns [nurbsSquare1, makeNurbsSquare1].

| Flag | Short | Description |
| --- | --- | --- |
| `normal` | `nr` | The normal. |
| `center` | `c` | The centre. |
| `sideLength1` | `sl1` | The side (1). |
| `name` | `n` | Name of the transform. |

## objExists

True if a node with this name or path exists.

Arguments: name

## objectType

The node type of an object (objectType -isType checks one).

Arguments: object

| Flag | Short | Description |
| --- | --- | --- |
| `isType` | `i` | Whether it is of this type. |

## offsetCurve

A copy offset in the curve's plane: -d distance, -nr normal. Returns [curve, node].

Arguments: curves (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `distance` | `d` | How far. |
| `normal` | `nr` | The plane's normal. |
| `useGivenNormal` | `ugn` | Accepted for Maya scripts. |
| `tolerance` | `tol` | Accepted for Maya scripts. |
| `constructionHistory` | `ch` | Keep a history node (on by default). |
| `replaceOriginal` | `rpo` | Change the curve itself (on by default). |
| `name` | `n` | Name of a new curve. |
| `object` | `o` | Make an object (accepted for Maya scripts). |

## orientConstraint

Constrain the object named (or selected) last to the targets before it. -mo keeps the current offset; -w sets the targets' weight. Returns [constraint].

Arguments: targets, then the object to drive

| Flag | Short | Description |
| --- | --- | --- |
| `maintainOffset` | `mo` | Keep the object where it is now relative to the targets. |
| `weight` | `w` | Weight of each target (1). |
| `name` | `n` | Name of the constraint. |
| `aimVector` | `aim` | Accepted for Maya scripts (local X aims). |
| `upVector` | `u` | Accepted for Maya scripts (local Y up). |
| `worldUpType` | `wut` | Accepted for Maya scripts. |
| `skipTranslate` | `st` | Accepted for Maya scripts. |
| `skipRotate` | `sr` | Accepted for Maya scripts. |

## orientJoint

Orient joints: the first axis of the order aims at the child joint, the second toward -sao. Joints below follow unless -ch 0.

Arguments: order (xyz), then joints

| Flag | Short | Description |
| --- | --- | --- |
| `secondaryAxisOrient` | `sao` | yup, zup, xdown... |
| `children` | `ch` | The joints below too (default on). |

## parent

parent(child..., parent), or parent(child..., world=True). Keeps world transforms unless relative=True. chain=True parents each object under the next one, in order: (a, b, c) puts a under b and b under c. index=N places the objects, in order, at position N among the parent's children (0 is first), which also reorders objects already under that parent. Leaves the moved objects selected.

Arguments: objects... parent

| Flag | Short | Description |
| --- | --- | --- |
| `world` | `w` | Unparent to the world. |
| `relative` | `r` | Keep local transforms instead of world. |
| `chain` | `ch` | Parent in selection order, each under the next. |
| `index` | `i` | Position among the parent's children, counted without the moved objects. |

## parentConstraint

Constrain the object named (or selected) last to the targets before it. -mo keeps the current offset; -w sets the targets' weight. Returns [constraint].

Arguments: targets, then the object to drive

| Flag | Short | Description |
| --- | --- | --- |
| `maintainOffset` | `mo` | Keep the object where it is now relative to the targets. |
| `weight` | `w` | Weight of each target (1). |
| `name` | `n` | Name of the constraint. |
| `aimVector` | `aim` | Accepted for Maya scripts (local X aims). |
| `upVector` | `u` | Accepted for Maya scripts (local Y up). |
| `worldUpType` | `wut` | Accepted for Maya scripts. |
| `skipTranslate` | `st` | Accepted for Maya scripts. |
| `skipRotate` | `sr` | Accepted for Maya scripts. |

## pasteKey

Paste the key clipboard onto channels at -t (the current time by default). -o insert (the default) moves later keys along, replace clears the pasted range, replaceCompletely clears the curve, merge overlays. -to and -vo offset, -cp repeats.

Arguments: objects or plugs (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `attribute` | `at` | Channel name(s). |
| `time` | `t` | Where to paste. |
| `option` | `o` | insert, replace, replaceCompletely or merge. |
| `timeOffset` | `to` | Added to the paste time. |
| `valueOffset` | `vo` | Added to the values. |
| `copies` | `cp` | How many times to paste. |
| `connect` | `c` | Accepted for Maya scripts. |
| `clipboard` | `cb` | Accepted for Maya scripts. |

## percent

Set or query deformer weights of points: percent -v 0.5 cluster1 pCube1.vtx[0:3]. -m multiplies what is there. -q -v returns one weight per point. With no points named: the selected components, else all the deformer's points.

Arguments: the deformer, then points or meshes

| Flag | Short | Description |
| --- | --- | --- |
| `edit` | `e` | Edit mode. |
| `query` | `q` | Query mode. |
| `value` | `v` | The weight. |
| `multiply` | `m` | Multiply the weights by the value instead of setting them. |

## pickWalk

Walk the selection through the hierarchy like the arrow keys: up to the parent, down to the first child, left/right to the previous/next sibling (wrapping). Right after a delete, with nothing selected since, it walks from where the deleted objects were.

| Flag | Short | Description |
| --- | --- | --- |
| `direction` |  | up, down, left or right. |

## pivot

World position of an object's rotate pivot.

Arguments: object

## planar

A face filling a closed curve. Returns [planarTrimmedSurface1, planarTrim1].

Arguments: a closed curve (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `degree` | `d` | Accepted for Maya scripts. |
| `tolerance` | `tol` | Accepted for Maya scripts. |
| `polygon` | `po` | Accepted: the output is always polygons. |
| `constructionHistory` | `ch` | Keep the node (on by default). |
| `range` | `rn` | Accepted for Maya scripts. |
| `name` | `n` | Name of the new object. |

## playbackOptions

The playback range (min/max), the whole animation range (ast/aet), looping and speed. In query mode returns the flag asked about.

| Flag | Short | Description |
| --- | --- | --- |
| `query` | `q` | Return the value of the flags named instead of setting them. |
| `edit` | `e` | Change the flags named. |
| `minTime` | `min` | First frame of the playback range. |
| `maxTime` | `max` | Last frame of the playback range. |
| `animationStartTime` | `ast` | First frame of the whole animation. |
| `animationEndTime` | `aet` | Last frame of the whole animation. |
| `loop` | `l` | continuous, once or oscillate. |
| `playbackSpeed` | `ps` | 1 for real time, 0 for every frame. |
| `framesPerSecond` | `fps` | Frames per second (24 by default, film). |

## pointConstraint

Constrain the object named (or selected) last to the targets before it. -mo keeps the current offset; -w sets the targets' weight. Returns [constraint].

Arguments: targets, then the object to drive

| Flag | Short | Description |
| --- | --- | --- |
| `maintainOffset` | `mo` | Keep the object where it is now relative to the targets. |
| `weight` | `w` | Weight of each target (1). |
| `name` | `n` | Name of the constraint. |
| `aimVector` | `aim` | Accepted for Maya scripts (local X aims). |
| `upVector` | `u` | Accepted for Maya scripts (local Y up). |
| `worldUpType` | `wut` | Accepted for Maya scripts. |
| `skipTranslate` | `st` | Accepted for Maya scripts. |
| `skipRotate` | `sr` | Accepted for Maya scripts. |

## pointOnCurve

Facts at a parameter of a curve: -pr parameter (-top a fraction of its length), asking -p position, -t tangent, -nt unit tangent (position when none).

Arguments: a curve (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `parameter` | `pr` | The parameter. |
| `turnOnPercentage` | `top` | -pr is a fraction 0 to 1 of the length. |
| `position` | `p` | Ask for the point. |
| `tangent` | `t` | Ask for the tangent. |
| `normalizedTangent` | `nt` | Ask for the unit tangent. |
| `constructionHistory` | `ch` | Accepted for Maya scripts. |

## pointPosition

The world position of a point: a vertex, a joint or transform (its pivot), or a locator. -l for local space.

Arguments: a point

| Flag | Short | Description |
| --- | --- | --- |
| `world` | `w` | World space (the default). |
| `local` | `l` | Object space. |

## polyAppendVertex

Add a face to a mesh from existing vertices and new points: -a vertex (repeat), -p x y z (repeat), or -items [{vertex:}, {position:}] in order. With no mesh named or selected and only points, makes a new mesh. Returns the new face and the mesh.

| Flag | Short | Description |
| --- | --- | --- |
| `mesh` |  | The mesh; the selected one by default. |
| `append` | `a` | An existing vertex. Repeat it. |
| `point` | `p` | A new point: x y z. Repeat it. |
| `items` |  | In order: {vertex: id} or {position: [x, y, z]}. |
| `constructionHistory` | `ch` | Accepted for Maya scripts. |

## polyAutoProjection

Project UVs from the six box planes, each face from the one it faces most, then lay the shells out in 0 to 1.

Arguments: faces or meshes

| Flag | Short | Description |
| --- | --- | --- |
| `percentageSpace` | `ps` | Space between shells, percent of the square. |
| `layout` | `lm` | Accepted for Maya scripts. |
| `planes` | `p` | Accepted for Maya scripts (six planes). |

## polyAverageVertex

Average Vertices: each selected vertex (or every vertex) moves toward the middle of its neighbours, -i times.

Arguments: vertices or meshes (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `iterations` | `i` | Passes (1). |

## polyBevel

Bevel the selected edges (vertex or face selections convert to their edges).

| Flag | Short | Description |
| --- | --- | --- |
| `offset` |  | Width, cm. |
| `segments` |  | Segments across the bevel. |
| `profile` |  | 0.5 round, 0.25 straight, toward 1 square. |
| `clamp` |  | Keep widths from overlapping. |
| `smoothingAngle` |  | New edges sharper than this are hard. |

## polyBoolean

Union, difference or intersection of meshes (Manifold: robust, watertight output), with the seam cleaned up: slivers welded, the n-gons along the cut split into quads that carry on the edge loops, an optional rounded fillet, and weighted normals. The first object keeps the result; the others are deleted unless keepCutters=True.

Arguments: base cutters...

| Flag | Short | Description |
| --- | --- | --- |
| `operation` |  | union, difference or intersection. |
| `keepCutters` |  | Keep the cutter objects (hidden). |
| `live` |  | Keep it live instead of baking: a boolean modifier per cutter, so moving or editing a cutter updates the cut. The cutters go in a template display layer (BooleanCutters): grey wire you select from the outliner. |
| `cleanup` |  | Weld slivers and stray points along the seam. |
| `snapDistance` |  | Weld distance for cleanup, cm; negative is a tenth of the median seam edge. |
| `splitNgons` |  | Cut the n-gons along the seam into quads (and triangles) that carry on the edge loops. |
| `quadBand` |  | Remesh a band of faces around the seam into quads that follow it, welded back into the faces around it. Falls back to splitting n-gons where the band's border can't be kept. |
| `bandRings` |  | Rings of faces past the seam the quad band takes in. |
| `timeout` |  | Seconds the quad band remesh may take before the n-gon split runs instead; 0 waits forever. |
| `filletWidth` |  | Width of a rounded strip along the seam, cm; 0 keeps it sharp. |
| `filletSegments` |  | Rows of faces across the fillet. |
| `hardAngle` |  | New edges sharper than this many degrees are marked hard. |
| `weightedNormals` |  | Add a weighted normals modifier, if the mesh has none, so the n-gons shade flat. |

## polyBridge

Join selected border loops or chains with faces. Several pairs bridge at once; a single border round a slot or sliver bridges its two long sides; borders of different lengths pair by arc length. Selected faces are deleted and the holes bridged.

| Flag | Short | Description |
| --- | --- | --- |
| `divisions` |  | Rows between the borders; -1 picks them from the gap, the faces around it and the open ends. |
| `curveType` |  | How the rows run between the borders. |
| `smoothness` |  | 0 straightens the curve, 1 is the full curve. |
| `tensionSource` |  | Handle length leaving the source border (Blend and Auto). |
| `tensionTarget` |  | Handle length entering the target border (Blend and Auto). |
| `bridgeOffset` |  | Steps the pairing round closed loops. |
| `directionSource` |  | Read the source border the other way round. |
| `directionTarget` |  | Read the target border the other way round. |
| `twist` |  | Turns the middle rows about the bridge's axis, degrees at the target. |
| `taper` |  | Scale of the middle rows about their centre. |
| `bias` |  | Where the taper peaks, 0 at the source, 1 at the target. |
| `taperCurve` |  | Taper strength along the bridge, values evenly spaced from source to target. |
| `profileFactor` |  | Bulge (positive) or pinch (negative) of the middle rows, as a fraction of the gap. |
| `profileShape` |  | Falloff of the profile across the bridge. |
| `curve` |  | The curve Path follows. |
| `transition` |  | How borders of different lengths make up the difference. |
| `weldEnds` |  | Open bridges land their end rows on the border between the two sides' ends. |
| `weldTolerance` |  | How far a row may be from an end border point and still weld, as a fraction of a row. |
| `relax` |  | Smoothing passes over the new points. |
| `smoothingAngle` |  | New edges sharper than this are hard. |
| `merge` |  | Merge the borders into one instead of bridging. |
| `mergeFactor` |  | Where merged points go: 0 the source, 1 the target. |
| `connect` |  | Pair loops up, or run one bridge through them all in order. |
| `startSource` |  | Source point that pairs with startTarget. |
| `startTarget` |  | Target point that pairs with startSource. |
| `pairs` |  | More point pairs to hold: [[source, target], ...]. |

## polyCapsule

Create a Capsule mesh. Returns [mesh, generator] (generator is None with generator=False).

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the mesh; made unique. |
| `position` |  | World position. |
| `rotation` |  | Rotation in degrees. |
| `normal` |  | Turn the up axis to this direction, as for placing on a surface; a dome cut by fraction then sits on position by its open base. |
| `generator` |  | Keep the generator node so its parameters stay editable. |
| `constructionHistory` | `ch` | False: no generator node, as generator=False. |
| `radius` | `r` | cm. |
| `height` | `h` | End to end, caps included, cm. |
| `subdivisionsAxis` | `sa` | Segments around. |
| `subdivisionsHeight` | `sh` | Segments along the straight part. |
| `subdivisionsCaps` | `sc` | Rings on each rounded end. |
| `baseAtOrigin` |  | Sit on the ground. |
| `upAxis` | `ax` | Direction the primitive's up points: x, y, z, -x, -y or -z (or a vector, as Maya's -ax 1 0 0); the scene up axis when left out. |

## polyCircularize

Fit the selected edge loops, the border of each selected face region, or the selected vertices to a circle in their best-fit plane, turned to move the points least. Points inside a face region follow the border smoothly, so a rectangular face comes out round, ready to extrude into a pipe. Open chains become an arc between their ends.

| Flag | Short | Description |
| --- | --- | --- |
| `spacing` |  | even (equal angles), keep (relative distances along the loop) or project (straight out to the circle). |
| `radius` |  | Radius, cm; 0 keeps the loop's average radius. |
| `rotation` |  | Extra turn about the circle's axis, degrees. |
| `sides` |  | 3 or more: conform to a regular polygon with this many sides, its corners on the circle. |
| `flatten` |  | Put the points in the circle's plane; off keeps each one's height off it. |
| `strength` |  | How far toward the shape, 0 to 1; soft selection fades it further. |

## polyCollapse

Collapse each connected group of selected edges to its centre.

## polyCombine

Merge meshes into one object (world space kept). Returns its name.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `name` |  | Name of the combined mesh. |

## polyCone

Create a Cone mesh. Returns [mesh, generator] (generator is None with generator=False).

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the mesh; made unique. |
| `position` |  | World position. |
| `rotation` |  | Rotation in degrees. |
| `normal` |  | Turn the up axis to this direction, as for placing on a surface; a dome cut by fraction then sits on position by its open base. |
| `generator` |  | Keep the generator node so its parameters stay editable. |
| `constructionHistory` | `ch` | False: no generator node, as generator=False. |
| `radius` | `r` | cm. |
| `height` | `h` | cm. |
| `subdivisionsAxis` | `sa` | Segments around. |
| `subdivisionsHeight` | `sh` | Segments up. |
| `subdivisionsCap` |  | Rings of quads around the base's fill; 0 and 1 both leave just the fill. |
| `capType` |  | How each flat end is filled inside its rings: quads (a grid joined to the rim by a ring, one triangle when subdivisionsAxis is odd), fan (triangles to the centre), ngon (one polygon) or none (left open). |
| `baseAtOrigin` |  | Sit on the ground. |
| `fraction` |  | Portion of a full turn swept around the up axis: 1 is whole, 0.5 half. |
| `cutCaps` |  | Close the flat ends a partial sweep leaves. |
| `capTip` |  | Cut the point off at tipRadius and fill it as capType, so quads leave no triangles at the tip. |
| `tipRadius` |  | Radius of the capped tip, cm; small keeps it looking pointed. |
| `upAxis` | `ax` | Direction the primitive's up points: x, y, z, -x, -y or -z (or a vector, as Maya's -ax 1 0 0); the scene up axis when left out. |

## polyConform

Make face winding consistent; closed parts face outward.

## polyConformShape

Conform the selected components to a fitted shape: line, circle, square or rectangle (per chain or loop, corners on the points nearest the fitted rectangle's), sphere, cylinder (fitted axis) or plane (flatten). Face interiors follow their borders for the loop shapes.

| Flag | Short | Description |
| --- | --- | --- |
| `shape` |  | line, circle, square, rectangle, sphere, cylinder or plane. |
| `spacing` |  | Line, circle, square, rectangle: even, keep or project. |
| `radius` |  | Circle, sphere, cylinder: radius, cm; 0 fits it. |
| `rotation` |  | Circle, square, rectangle: extra turn in the plane, degrees. |
| `sides` |  | Circle: 3 or more makes it a regular polygon. |
| `axis` |  | Cylinder: auto (fitted), x, y or z (object axes). |
| `strength` |  | How far toward the shape, 0 to 1; soft selection fades it further. |

## polyConformToObject

Conform the selected components onto another mesh or a curve. loop: each selected edge loop (or face region border) lands one to one on the target's nearest closed loop (a cylinder's rim, a ring), at the turn that moves it least, or spread along it when the counts differ; face interiors follow. surface: every point to the nearest surface point. vertex: every point to the nearest target vertex. A curve target works through its line: loop spreads a loop along a closed curve, surface puts each point on the nearest point of the curve.

| Flag | Short | Description |
| --- | --- | --- |
| `target` |  | Mesh or curve to conform to; default: the selected mesh or curve that has no components selected. |
| `mode` |  | loop, surface or vertex. |
| `spacing` |  | Loop mode with unequal counts: even, keep or project. |
| `strength` |  | How far toward the shape, 0 to 1; soft selection fades it further. |

## polyConnect

Connect selected vertices that share a face with new edges, or the midpoints of selected edges across their faces.

## polyCrease

Set the crease of the selected edges (-v 0 to 10): it holds them sharper under smoothing. -q -v asks.

Arguments: edges (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `value` | `v` | Crease value: 0 to 10. |
| `relativeValue` | `rv` | Accepted for Maya scripts. |
| `query` | `q` | Query mode. |

## polyCreateFacet

Create a mesh of one face through the points given with -p (repeat it, or a list). Returns [transform].

| Flag | Short | Description |
| --- | --- | --- |
| `point` | `p` | The face's points, in order. |
| `constructionHistory` | `ch` | Accepted for Maya scripts. |

## polyCube

Create a Cube mesh. Returns [mesh, generator] (generator is None with generator=False).

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the mesh; made unique. |
| `position` |  | World position. |
| `rotation` |  | Rotation in degrees. |
| `normal` |  | Turn the up axis to this direction, as for placing on a surface; a dome cut by fraction then sits on position by its open base. |
| `generator` |  | Keep the generator node so its parameters stay editable. |
| `constructionHistory` | `ch` | False: no generator node, as generator=False. |
| `width` | `w` | Size along X, cm. |
| `height` | `h` | Size along the up axis, cm. |
| `depth` | `d` | Size along Z, cm. |
| `subdivisionsX` | `sx` | Segments across. |
| `subdivisionsY` | `sy` | Segments up. |
| `subdivisionsZ` | `sz` | Segments deep. |
| `baseAtOrigin` |  | Sit on the ground instead of centring. |
| `upAxis` | `ax` | Direction the primitive's up points: x, y, z, -x, -y or -z (or a vector, as Maya's -ax 1 0 0); the scene up axis when left out. |

## polyCylinder

Create a Cylinder mesh. Returns [mesh, generator] (generator is None with generator=False).

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the mesh; made unique. |
| `position` |  | World position. |
| `rotation` |  | Rotation in degrees. |
| `normal` |  | Turn the up axis to this direction, as for placing on a surface; a dome cut by fraction then sits on position by its open base. |
| `generator` |  | Keep the generator node so its parameters stay editable. |
| `constructionHistory` | `ch` | False: no generator node, as generator=False. |
| `radius` | `r` | cm. |
| `height` | `h` | cm. |
| `subdivisionsAxis` | `sa` | Segments around. |
| `subdivisionsHeight` | `sh` | Segments up. |
| `subdivisionsCaps` | `sc` | Rings of quads around each cap's fill; 0 and 1 both leave just the fill. |
| `capType` |  | How each flat end is filled inside its rings: quads (a grid joined to the rim by a ring, one triangle when subdivisionsAxis is odd), fan (triangles to the centre), ngon (one polygon) or none (left open). |
| `caps` |  | Close the ends; off leaves them open whatever capType says. |
| `baseAtOrigin` |  | Sit on the ground. |
| `fraction` |  | Portion of a full turn swept around the up axis: 1 is whole, 0.5 half. |
| `cutCaps` |  | Close the flat ends a partial sweep leaves. |
| `upAxis` | `ax` | Direction the primitive's up points: x, y, z, -x, -y or -z (or a vector, as Maya's -ax 1 0 0); the scene up axis when left out. |

## polyDisc

Create a Disc mesh. Returns [mesh, generator] (generator is None with generator=False).

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the mesh; made unique. |
| `position` |  | World position. |
| `rotation` |  | Rotation in degrees. |
| `normal` |  | Turn the up axis to this direction, as for placing on a surface; a dome cut by fraction then sits on position by its open base. |
| `generator` |  | Keep the generator node so its parameters stay editable. |
| `constructionHistory` | `ch` | False: no generator node, as generator=False. |
| `radius` | `r` | cm. |
| `subdivisionsAxis` | `sa` | Segments around. |
| `subdivisionsRings` |  | Rings; 0 is one n-gon. |
| `fraction` |  | Portion of a full turn swept around the up axis: 1 is whole, 0.5 half. |
| `capType` |  | How the innermost ring is filled: quads (a grid joined to it by a ring, one triangle when subdivisionsAxis is odd), fan (triangles to the centre) or ngon (one polygon). subdivisionsRings 0 is always one n-gon. |
| `upAxis` | `ax` | Direction the primitive's up points: x, y, z, -x, -y or -z (or a vector, as Maya's -ax 1 0 0); the scene up axis when left out. |

## polyDissolve

Remove selected edges or vertices by merging the faces around them, cleanly.

## polyDuplicateFaces

Copy the selected faces into a separate shell of the same mesh and select the copies.

## polyEditEdgeFlow

Move the selected edge loops onto the curvature of the surface around them (Maya's Edit Edge Flow). Each vertex lands on a curve through the nearest unselected vertices across the loop, shaped by the vertices beyond them, and neighbouring selected loops spread evenly between those, as subdividing would place them. Selected vertices off any chain, and face selections, fit both directions across each vertex.

| Flag | Short | Description |
| --- | --- | --- |
| `adjustEdgeFlow` |  | 0 flat between the neighbours, 1 on the curve, above 1 exaggerated, negative inverted. |
| `edgeFlow` |  | Shape the curve from the vertices beyond the neighbours; off, from the surface normals at the neighbours. |
| `tension` |  | Toward 1 flattens the curve, negative makes it fuller. |
| `loopSmooth` |  | Also smooth each loop along its length, 0 to 1, keeping its spacing. |
| `iterations` |  | Passes of Smooth Along Loop. |

## polyEditUV

Move (-u -v), scale (-su -sv) or turn (-a degrees) UVs about the pivot (-pu -pv). Relative by default; -r 0 sets -u/-v. -q returns u, v of each UV.

Arguments: UVs (pCube1.map[0:3]), faces or meshes

| Flag | Short | Description |
| --- | --- | --- |
| `u` | `u` | U offset (or value with -r 0). |
| `v` | `v` | V offset (or value with -r 0). |
| `scaleU` | `su` | Scale in U about the pivot. |
| `scaleV` | `sv` | Scale in V about the pivot. |
| `angle` | `a` | Turn, degrees, about the pivot. |
| `pivotU` | `pu` | Pivot U. |
| `pivotV` | `pv` | Pivot V. |
| `relative` | `r` | Offsets (default) or values. |
| `query` | `q` | Return u, v of each UV. |

## polyEvaluate

Counts on meshes or components: -v vertices, -e edges, -f faces, -t triangles, -uv UVs, -s shells, -b bounding box. With no flag, a dictionary of the counts.

Arguments: meshes or components (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `vertex` | `v` | Vertex count. |
| `edge` | `e` | Edge count. |
| `face` | `f` | Face count. |
| `triangle` | `t` | Triangle count. |
| `uvcoord` | `uv` | UV count. |
| `shell` | `s` | Shell count (UV shells). |
| `boundingBox` | `b` | World bounding box ((xmin, xmax), (ymin, ymax), (zmin, zmax)). |

## polyExtract

Move the selected faces out into new mesh objects (one per source mesh). Returns their names.

## polyExtrude

Extrude the selected faces along their normals, or in edge mode grow a strip of faces off the selected edges. Connected faces extrude as regions unless individual=True.

| Flag | Short | Description |
| --- | --- | --- |
| `thickness` |  | Distance along the normals, cm. |
| `offset` |  | Faces: inward distance of the extruded border, cm; negative grows them. Edges: outward distance from their faces, cm. |
| `individual` |  | Each face separately. |
| `keepBack` |  | Faces: where the extruded faces only border open edges, as a whole open shell, keep a copy facing back at the base. |
| `duplicate` |  | Faces: leave the mesh as it is and put the extruded faces in a new mesh. |
| `keepSides` |  | With duplicate, the new mesh also keeps the side faces. |

## polyFillHole

Cap the holes bordered by the selected edges (every hole when nothing is selected).

## polyFlip

Reverse the winding of the selected faces (the whole mesh in object mode).

## polyFlipUV

Mirror UVs across the middle of their bounds: -ft 0 horizontally (in U), 1 vertically.

Arguments: UVs, faces or meshes

| Flag | Short | Description |
| --- | --- | --- |
| `flipType` | `ft` | 0 horizontal, 1 vertical. |
| `local` | `l` | Each shell about its own middle. |

## polyGridFill

Rebuild triangle fans and other patches as a quad grid inside a ring of quads: select the fans' centre vertices or the faces. With edges selected, fills the holes they border the same way.

| Flag | Short | Description |
| --- | --- | --- |
| `gridSize` |  | The grid's size against the hole; 1 grids right to the border with no ring. |
| `rotate` |  | Steps the grid's corners round the border. |

## polyIcoSphere

Create a IcoSphere mesh. Returns [mesh, generator] (generator is None with generator=False).

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the mesh; made unique. |
| `position` |  | World position. |
| `rotation` |  | Rotation in degrees. |
| `normal` |  | Turn the up axis to this direction, as for placing on a surface; a dome cut by fraction then sits on position by its open base. |
| `generator` |  | Keep the generator node so its parameters stay editable. |
| `constructionHistory` | `ch` | False: no generator node, as generator=False. |
| `radius` | `r` | cm. |
| `subdivisions` |  | 1 is an icosahedron; each level splits every triangle in four. |
| `baseAtOrigin` |  | Sit on the ground. |
| `fraction` |  | Portion of the height kept, from the top: 1 is whole, 0.5 a dome open at the bottom. |
| `upAxis` | `ax` | Direction the primitive's up points: x, y, z, -x, -y or -z (or a vector, as Maya's -ax 1 0 0); the scene up axis when left out. |

## polyInsertLoop

Insert edge loops across the ring of the first selected edge. Several loops are centred on fraction, spacing apart.

| Flag | Short | Description |
| --- | --- | --- |
| `fraction` |  | Where along the ring, 0 to 1; with several loops, where their middle goes. |
| `count` |  | Number of loops. |
| `spacing` |  | Several loops: gap between neighbours, 1 even spacing across the edge, smaller packs them together. |

## polyInset

Inset the selected faces.

| Flag | Short | Description |
| --- | --- | --- |
| `thickness` |  | Inward distance, cm. |
| `depth` |  | Push along the normals afterwards, cm. |
| `individual` |  | Each face separately. |
| `clamp` |  | Shrink the thickness where the inset would cross itself, as on narrow faces. |

## polyKnife

Knife / multi-cut: cut a mesh along a path of points, splitting the faces between them and the edges the path crosses. Between two points the cut follows the surface under the straight screen line joining them (seen from eye= or along direction=; without either, along the points' average normal). With loopEdge= it inserts an edge loop across that edge's quad ring instead, and with deleteEdge= it dissolves that edge. Follows the mesh's symmetry. A single point on an edge inserts a vertex there. Returns the new edges.

| Flag | Short | Description |
| --- | --- | --- |
| `mesh` |  | Mesh to cut; default: the selected or hilited mesh. |
| `points` |  | Path points: {"vertex": id}, {"edge": id, "t": 0..1 from its first point}, or {"face": id, "position": [x, y, z] world}. |
| `closed` |  | Also cut from the last point back to the first. |
| `through` |  | Cut every face under the line, hidden faces too. |
| `eye` |  | World eye of a perspective view. |
| `direction` |  | World view direction of an orthographic view. |
| `loopEdge` |  | Insert an edge loop across this edge's ring instead of cutting a path. |
| `fraction` |  | Where along loopEdge the loop crosses it, from its first point. |
| `deleteEdge` |  | Dissolve this edge, joining its two faces, instead of cutting. |
| `select` |  | Select the new edges. |

## polyListComponentConversion

Convert components without selecting them: -fv/-fe/-ff from, -tv/-te/-tf to (-in for only those fully inside). Returns component strings.

Arguments: components or meshes (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `fromVertex` | `fv` | From vertices (read from the arguments here). |
| `fromEdge` | `fe` | From edges. |
| `fromFace` | `ff` | From faces. |
| `toVertex` | `tv` | To vertices. |
| `toEdge` | `te` | To edges. |
| `toFace` | `tf` | To faces. |
| `internal` | `in` | Only components wholly inside. |

## polyLockNormals

Store the current shading normals of the selected meshes as split normals, so later edits keep them.

## polyMapCut

Cut UVs along the selected edges: each side gets its own UVs, splitting shells.

Arguments: edges

## polyMapSew

Sew UVs along the selected edges: the two sides meet halfway.

Arguments: edges

## polyMapSewMove

Move and Sew: the smaller shell moves onto the other along the selected edges, then they're sewn.

Arguments: edges

| Flag | Short | Description |
| --- | --- | --- |
| `nf` | `nf` | Accepted for Maya scripts. |
| `lps` | `lps` | Accepted for Maya scripts. |

## polyMerge

Weld selected vertices closer than a distance (all of a mesh's points if it has no component selection). Exactly two selected vertices weld whatever the distance.

| Flag | Short | Description |
| --- | --- | --- |
| `distance` |  | Weld distance, cm. |
| `average` |  | Move merged points to their average. |

## polyMergeCenter

Weld the selected vertices into one at their centre.

## polyMirror

Mirror whole meshes across the plane through the object's or the world's origin facing an axis, welding along the seam. direction positive keeps the + side and rebuilds the - side from it, cutting faces that cross the plane; negative does the reverse; both adds a mirrored copy of the whole mesh. flip=True reflects the mesh across the plane instead, keeping nothing of the original.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `axis` |  | x, y or z. |
| `direction` |  | positive (+ to -), negative (- to +) or both. |
| `space` |  | object or world: whose origin and axes the plane uses. |
| `weld` |  | Weld points on the plane. |
| `tolerance` |  | Points this close to the plane are cut onto it and welded, cm. |
| `flip` |  | Reflect the mesh across the plane rather than adding a mirrored copy: the original is not kept. |

## polyMoveVertex

Move vertices by -t (translate) in world space, as Maya's polyMoveVertex.

Arguments: vertices

| Flag | Short | Description |
| --- | --- | --- |
| `translate` | `t` | World offset. |

## polyNormalizeUV

Fit UVs into 0 to 1: all together (-nt 1) or each shell on its own (-nt 0), keeping proportions unless -pa 0.

Arguments: faces or meshes

| Flag | Short | Description |
| --- | --- | --- |
| `normalizeType` | `nt` | 1 all together, 0 each shell. |
| `preserveAspectRatio` | `pa` | Keep proportions (default on). |

## polyOffsetLoop

Insert an edge loop either side of the selected edges, parallel to them (Maya's Offset Edge Loop). Partial loops end on a border, or inside the mesh by meeting at the end vertex or wrapping round it. Returns the new edges.

| Flag | Short | Description |
| --- | --- | --- |
| `distance` |  | How far out: a fraction of each crossing edge's length, or cm with absolute. |
| `absolute` |  | distance is in cm, square to the selected edges. |
| `side` |  | both, left or right of the edges' direction. |
| `ends` |  | Open ends inside the mesh: converge (meet at the end vertex) or wrap (round it). |
| `deleteEdge` |  | Dissolve the selected edges afterward, leaving the two new loops. |

## polyPipe

Create a Pipe mesh. Returns [mesh, generator] (generator is None with generator=False).

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the mesh; made unique. |
| `position` |  | World position. |
| `rotation` |  | Rotation in degrees. |
| `normal` |  | Turn the up axis to this direction, as for placing on a surface; a dome cut by fraction then sits on position by its open base. |
| `generator` |  | Keep the generator node so its parameters stay editable. |
| `constructionHistory` | `ch` | False: no generator node, as generator=False. |
| `radius` | `r` | Outer radius, cm. |
| `thickness` | `t` | Wall thickness, cm. |
| `height` | `h` | cm. |
| `subdivisionsAxis` | `sa` | Segments around. |
| `subdivisionsHeight` | `sh` | Segments up. |
| `baseAtOrigin` |  | Sit on the ground. |
| `fraction` |  | Portion of a full turn swept around the up axis: 1 is whole, 0.5 half. |
| `cutCaps` |  | Close the flat ends a partial sweep leaves. |
| `upAxis` | `ax` | Direction the primitive's up points: x, y, z, -x, -y or -z (or a vector, as Maya's -ax 1 0 0); the scene up axis when left out. |

## polyPlane

Create a Plane mesh. Returns [mesh, generator] (generator is None with generator=False).

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the mesh; made unique. |
| `position` |  | World position. |
| `rotation` |  | Rotation in degrees. |
| `normal` |  | Turn the up axis to this direction, as for placing on a surface; a dome cut by fraction then sits on position by its open base. |
| `generator` |  | Keep the generator node so its parameters stay editable. |
| `constructionHistory` | `ch` | False: no generator node, as generator=False. |
| `width` | `w` | Size along X, cm. |
| `depth` | `d` | Size along Z, cm. |
| `subdivisionsX` | `sx` | Segments across. |
| `subdivisionsZ` | `sz` | Segments deep. |
| `upAxis` | `ax` | Direction the primitive's up points: x, y, z, -x, -y or -z (or a vector, as Maya's -ax 1 0 0); the scene up axis when left out. |

## polyPoke

Fan each selected face into triangles around its centre.

## polyProjection

Project UVs of the selected faces (or meshes) through a plane, cylinder or sphere. -md picks the plane's axis (x, y, z) or b for best fit.

Arguments: faces or meshes

| Flag | Short | Description |
| --- | --- | --- |
| `type` | `t` | planar, cylindrical or spherical. |
| `mapDirection` | `md` | x, y, z, or b (best plane). |
| `constructionHistory` | `ch` | Accepted for Maya scripts. |

## polyQuadSphere

Create a QuadSphere mesh. Returns [mesh, generator] (generator is None with generator=False).

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the mesh; made unique. |
| `position` |  | World position. |
| `rotation` |  | Rotation in degrees. |
| `normal` |  | Turn the up axis to this direction, as for placing on a surface; a dome cut by fraction then sits on position by its open base. |
| `generator` |  | Keep the generator node so its parameters stay editable. |
| `constructionHistory` | `ch` | False: no generator node, as generator=False. |
| `radius` | `r` | cm. |
| `subdivisions` |  | Segments per cube face edge. |
| `fraction` |  | Portion of the height kept, from the top: 1 is whole, 0.5 a dome open at the bottom. |
| `upAxis` | `ax` | Direction the primitive's up points: x, y, z, -x, -y or -z (or a vector, as Maya's -ax 1 0 0); the scene up axis when left out. |

## polyQuadrangulate

Join neighbouring triangles into quads.

| Flag | Short | Description |
| --- | --- | --- |
| `maxAngle` |  | Largest angle between the two triangles, degrees. |

## polyReduce

Reduce the polygon count of whole meshes while keeping their shape, silhouette, UVs and normals. The result is triangles (or quads with quads=True). A mesh with modifiers gets a decimate modifier on top instead. Honours the mesh's symmetry: one half (or one radial wedge) is reduced and mirrored (or repeated).

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `ratio` |  | Fraction of triangles to keep, 0 to 1. |
| `targetTriangles` |  | Triangles to keep; above 0 it overrides ratio. |
| `keepBorders` |  | Open borders stay exactly where they are. |
| `keepHardEdges` |  | Hard edges stay intact, as UV seams and material borders always do; off lets them collapse for a lower count. |
| `symmetry` |  | auto (the mesh's symmetry setting), off, x, y or z. |
| `quads` |  | Join triangle pairs back into quads where they make good ones. |

## polyRotateUVs

Turn UVs about the middle of their bounds, degrees counterclockwise.

Arguments: angle, then UVs, faces or meshes

## polySeparate

Split meshes into one object per connected part. Returns the new names.

Arguments: objects

## polySlideEdge

Slide the selected edges within the faces beside them: -v -1 to 1, a fraction of the way along the neighbouring edges to one side or the other.

| Flag | Short | Description |
| --- | --- | --- |
| `value` | `v` | How far: -1 to 1. |
| `absolute` | `a` | Accepted for Maya scripts. |
| `direction` | `d` | Accepted for Maya scripts. |
| `edgeFlow` | `ef` | Accepted for Maya scripts. |
| `symmetry` | `sym` | Accepted for Maya scripts. |

## polySoftEdge

Make the selected edges soft or hard by the angle their faces meet at: sharper than angle turns hard, the rest soft. 180 softens every edge, 0 hardens every edge. In object mode, whole selected meshes.

| Flag | Short | Description |
| --- | --- | --- |
| `angle` |  | Edges whose faces meet sharper than this turn hard, degrees. |

## polySphere

Create a Sphere mesh. Returns [mesh, generator] (generator is None with generator=False).

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the mesh; made unique. |
| `position` |  | World position. |
| `rotation` |  | Rotation in degrees. |
| `normal` |  | Turn the up axis to this direction, as for placing on a surface; a dome cut by fraction then sits on position by its open base. |
| `generator` |  | Keep the generator node so its parameters stay editable. |
| `constructionHistory` | `ch` | False: no generator node, as generator=False. |
| `radius` | `r` | cm. |
| `subdivisionsAxis` | `sa` | Segments around. |
| `subdivisionsHeight` | `sh` | Segments pole to pole. |
| `baseAtOrigin` |  | Sit on the ground. |
| `fraction` |  | Portion of the height kept, from the top: 1 is whole, 0.5 a dome open at the bottom. |
| `upAxis` | `ax` | Direction the primitive's up points: x, y, z, -x, -y or -z (or a vector, as Maya's -ax 1 0 0); the scene up axis when left out. |

## polySpherize

Push the selected components (whole meshes in object mode) onto a sphere fitted to them, or onto an ellipsoid with scale. center and radius override the fit (world space, cm); radiusMode picks the radius from the points otherwise. With alongNormals each point slides along its own normal to the sphere. Returns the sphere used: {center, radius, scale, rotation}, world space.

| Flag | Short | Description |
| --- | --- | --- |
| `amount` |  | How far toward the sphere, 0 to 1; soft selection fades it further. |
| `fit` |  | best (least-squares sphere) or center (around the points' centroid). |
| `radiusMode` |  | Radius when radius is 0: average distance, fit (the fitted radius), min, max or explicit. |
| `radius` |  | Radius, cm; above 0 it replaces radiusMode. |
| `center` |  | Sphere centre, world space cm; default: the fitted centre. |
| `scale` |  | Ellipsoid semi-axis factors along its X, Y and Z. |
| `rotation` |  | Turn of the ellipsoid's axes in world space, degrees (XYZ). |
| `alongNormals` |  | Move each point along its own normal onto the sphere. |

## polySpinEdge

Turn each selected edge one step within its two faces.

## polySplitRings

Subdivide one way: insert loops across every ring through the selected edges, or across the selected faces in one direction, without the perpendicular cuts a full subdivide adds. Returns the new edges.

| Flag | Short | Description |
| --- | --- | --- |
| `count` |  | Loops per ring, evenly spaced. |
| `direction` |  | Faces: across adds lines across the selection's short way, along the long way. |
| `edgeFlow` |  | Curve the new loops to follow the surrounding surface rather than lie flat. |

## polyStraighten

Straighten the selected edge chains (or chains of selected vertices) into lines between their ends. Several separate chains straighten separately; closed loops are left alone. Vertices with no edges between them line up on their best-fit line.

| Flag | Short | Description |
| --- | --- | --- |
| `spacing` |  | keep (relative distances along the chain), even or project (straight across). |
| `fitLine` |  | Use the line through all of a chain's points instead of its ends. |
| `strength` |  | How far toward the shape, 0 to 1; soft selection fades it further. |

## polySubdivide

Catmull-Clark subdivide whole meshes (crease-aware).

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `levels` |  | Subdivision levels. |
| `keepHardEdges` |  | Keep normals split along hard edges; off smooths the shading across them. |

## polyTargetWeld

Target weld: merge one vertex into another, which keeps its place (or both meet halfway with center=), or an edge into another, end to nearest end. Follows the mesh's symmetry; a weld across the symmetry plane meets on it. Returns the welded vertices.

| Flag | Short | Description |
| --- | --- | --- |
| `mesh` |  | Mesh to weld; default: the selected or hilited mesh. |
| `source` |  | Vertex (or edge) that moves. |
| `target` |  | Vertex (or edge) it merges into. |
| `edge` |  | source and target are edges. |
| `center` |  | Meet halfway instead of at the target. |

## polyToCurve

A curve through a loop of selected edges (Maya's Mesh > Create Curve From Edges): -d the degree (3), -f 0 open or 2 best. Returns [curve].

| Flag | Short | Description |
| --- | --- | --- |
| `degree` | `d` | Degree. |
| `form` | `f` | Accepted for Maya scripts. |
| `constructionHistory` | `ch` | Accepted for Maya scripts. |
| `name` | `n` | Name of the curve. |

## polyTorus

Create a Torus mesh. Returns [mesh, generator] (generator is None with generator=False).

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the mesh; made unique. |
| `position` |  | World position. |
| `rotation` |  | Rotation in degrees. |
| `normal` |  | Turn the up axis to this direction, as for placing on a surface; a dome cut by fraction then sits on position by its open base. |
| `generator` |  | Keep the generator node so its parameters stay editable. |
| `constructionHistory` | `ch` | False: no generator node, as generator=False. |
| `radius` | `r` | Ring radius, cm. |
| `sectionRadius` | `sr` | Tube radius, cm. |
| `subdivisionsAxis` | `sa` | Segments around the ring. |
| `subdivisionsHeight` | `sh` | Segments around the tube. |
| `fraction` |  | Portion of a full turn swept around the up axis: 1 is whole, 0.5 half. |
| `cutCaps` |  | Close the flat ends a partial sweep leaves. |
| `upAxis` | `ax` | Direction the primitive's up points: x, y, z, -x, -y or -z (or a vector, as Maya's -ax 1 0 0); the scene up axis when left out. |

## polyTriangulate

Triangulate the selected faces (the whole mesh in object mode), deterministically, as Unreal will. Concave and non-planar faces, collinear points and faces touching themselves split without overlaps or slivers, and no diagonal doubles an edge another face already has. With symmetry on, the mirrored faces split the mirrored way. A whole mesh with modifiers gets a triangulate modifier on top instead, so the result is triangulated rather than the cage.

| Flag | Short | Description |
| --- | --- | --- |
| `beauty` |  | Flip n-gon diagonals toward the largest smallest angle (what display and export use). |
| `quadSplit` |  | Quads split along the shorter diagonal (what display, export and MikkTSpace use) or always from the first corner. |

## polyUnlockNormals

Drop the stored split normals of the selected meshes, so shading comes from the hard and soft edges again.

## polyWeightedNormals

Face-weighted normals on the selected faces (whole meshes in object mode): each smooth fan takes the normal of its biggest faces, so chamfered or bevelled low poly shades its large faces flat and only the chamfers bend. Written as locked split normals, which export to FBX. polyUnlockNormals goes back to computed ones.

| Flag | Short | Description |
| --- | --- | --- |
| `mode` | `m` | What weighs a face: area, angle (its corner angle) or areaAngle. |
| `dominance` | `d` | Faces under this fraction of the heaviest face in a fan are left out; 0 keeps all. |
| `keepSharp` | `ks` | Keep hard edges split. |
| `faceStrength` | `fs` | Only the faces with the highest face int faceStrength count, when the mesh has it. |

## rebuildCurve

Rebuild with even spans: -s spans (0 keeps the count), -d degree, -kcp keeps the CVs. Returns [curve, node].

Arguments: curves (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `spans` | `s` | Spans. |
| `degree` | `d` | Degree. |
| `keepControlPoints` | `kcp` | Keep the CVs. |
| `rebuildType` | `rt` | Accepted for Maya scripts (uniform). |
| `endKnots` | `end` | Accepted for Maya scripts. |
| `keepRange` | `kr` | Accepted for Maya scripts. |
| `keepEndPoints` | `kep` | Accepted for Maya scripts. |
| `keepTangents` | `kt` | Accepted for Maya scripts. |
| `tolerance` | `tol` | Accepted for Maya scripts. |
| `constructionHistory` | `ch` | Keep a history node (on by default). |
| `replaceOriginal` | `rpo` | Change the curve itself (on by default). |
| `name` | `n` | Name of a new curve. |
| `object` | `o` | Make an object (accepted for Maya scripts). |

## redo

Redo the last undone command.

## removeFromDisplayLayer

Take objects (the selection by default) out of their display layers.

Arguments: objects

## removeFromSet

Take members (the selection by default) out of a set. An object takes its components with it. Returns the set's members.

Arguments: set, members

## removeModifier

Remove modifiers, reconnecting the stack around them.

Arguments: modifiers

## removeMultiInstance

Remove array elements with their connections and values: 'node.attr[3]', or 'node.group[3]' for every child of a grouped array.

Arguments: plugs

## rename

rename(old, new) or rename(new) for the single selected object. Returns the renamed node's path.

Arguments: [object] newName

## reorderDeformers

Swap two deformers' places in a mesh's history: reorderDeformers cluster1 ffd1 pCube1. Without geometry, on every mesh both are on.

Arguments: two deformers, then optionally the geometry

## reorderDisplayLayer

Move a display layer to a row of the Display Layers panel (0 is the top).

Arguments: layer

| Flag | Short | Description |
| --- | --- | --- |
| `index` | `i` | Row it moves to. |

## resetPivot

Put the pivots back at the object's origin, keeping the object where it is.

Arguments: objects

## resetTransforms

Put translate, rotate and scale back to their defaults (the object moves).

Arguments: objects

## reverseCurve

Reverse the direction. Returns [curve, node].

Arguments: curves (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `constructionHistory` | `ch` | Keep a history node (on by default). |
| `replaceOriginal` | `rpo` | Change the curve itself (on by default). |
| `name` | `n` | Name of a new curve. |
| `object` | `o` | Make an object (accepted for Maya scripts). |

## revolve

Turn a curve about an axis: revolve curve1 -ax 0 1 0 -p 0 0 0 -s 8 -ssw 0 -esw 360. Returns [revolvedSurface1, revolve1].

Arguments: a curve (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `axis` | `ax` | The axis. |
| `pivot` | `p` | A point on the axis. |
| `sections` | `s` | Sections (8). |
| `divisions` | `dv` | Mesh points per span of the curve. |
| `startSweep` | `ssw` | Start, degrees. |
| `endSweep` | `esw` | End, degrees. |
| `degree` | `d` | Accepted for Maya scripts. |
| `useTolerance` | `ut` | Accepted for Maya scripts. |
| `tolerance` | `tol` | Accepted for Maya scripts. |
| `polygon` | `po` | Accepted: the output is always polygons. |
| `constructionHistory` | `ch` | Keep the node (on by default). |
| `range` | `rn` | Accepted for Maya scripts. |
| `name` | `n` | Name of the new object. |

## rotate

Rotate objects about their pivots. rotate(by=(x, y, z)) adds a rotation (degrees, XYZ order) in `space`; rotate(to=(x, y, z)) sets the local Euler angles. rotate(x=90) sets one local angle and leaves the others. With components selected it turns them that far about the world axis through their centre instead.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `by` |  | Relative rotation, degrees. |
| `to` |  | Absolute local rotation, degrees. |
| `x` |  | Local X rotation, degrees; on components, a turn about world X. |
| `y` |  | Local Y rotation, degrees. |
| `z` |  | Local Z rotation, degrees. |
| `space` |  | 'world', 'object' or 'parent' for `by`. |

## rotateComponents

Rotate the selected components about a world pivot (default: their centre; with symmetry, a pair clear of the mirror plane turns about each side's own centre). Degrees, XYZ order.

| Flag | Short | Description |
| --- | --- | --- |
| `by` |  | Rotation, degrees. |
| `pivot` |  | World pivot. |

## scale

Scale objects along their own axes. scale(by=(x, y, z)) multiplies; scale(to=(x, y, z)) sets. scale(x=2) sets one local scale and leaves the others. With components selected it scales them by that factor along the world axis about their centre instead, so scale(y=0) flattens them.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `by` |  | Factor per axis. |
| `to` |  | Absolute scale. |
| `x` |  | Local X scale; on components, a factor along world X. |
| `y` |  | Local Y scale. |
| `z` |  | Local Z scale. |

## scaleComponents

Scale the selected components about a world pivot (default: their centre; with symmetry, a pair clear of the mirror plane scales about each side's own centre).

| Flag | Short | Description |
| --- | --- | --- |
| `by` |  | Factor per axis: the world axes, or the axes orient= turns them to. |
| `pivot` |  | World pivot. |
| `orient` |  | Rotation of the scale axes from the world axes, degrees, XYZ order. |

## scaleConstraint

Constrain the object named (or selected) last to the targets before it. -mo keeps the current offset; -w sets the targets' weight. Returns [constraint].

Arguments: targets, then the object to drive

| Flag | Short | Description |
| --- | --- | --- |
| `maintainOffset` | `mo` | Keep the object where it is now relative to the targets. |
| `weight` | `w` | Weight of each target (1). |
| `name` | `n` | Name of the constraint. |
| `aimVector` | `aim` | Accepted for Maya scripts (local X aims). |
| `upVector` | `u` | Accepted for Maya scripts (local Y up). |
| `worldUpType` | `wut` | Accepted for Maya scripts. |
| `skipTranslate` | `st` | Accepted for Maya scripts. |
| `skipRotate` | `sr` | Accepted for Maya scripts. |

## scaleKey

Scale keys in time about -tp (-ts) and in value about -vp (-vs). -t limits them. Returns how many.

Arguments: objects, plugs or curves (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `attribute` | `at` | Channel name(s). |
| `time` | `t` | A frame or range. |
| `timeScale` | `ts` | Time scale. |
| `timePivot` | `tp` | Time pivot. |
| `valueScale` | `vs` | Value scale. |
| `valuePivot` | `vp` | Value pivot. |

## sculptMeshCacheCtx

The sculpting tools' settings. -st the brush (sculpt, smooth, relax, grab, pinch, flatten, foamy, spray, repeat, imprint, wax, scrape, fill, knife, smear, bulge, amplify, freeze), -sz size, -str strength, -inv invert, -sym symmetry, -q to ask. -cf clears the freeze.

Arguments: the tool's name

| Flag | Short | Description |
| --- | --- | --- |
| `sculptTool` | `st` | The brush. |
| `size` | `sz` | Brush radius, world units. |
| `strength` | `str` | Strength, 0 to 1. |
| `invert` | `inv` | Push the other way. |
| `symmetry` | `sym` | Mirror across the object's YZ plane. |
| `clearFreeze` | `cf` | Unfreeze everything. |
| `query` | `q` | Query mode. |
| `edit` | `e` | Edit mode. |

## sculptStamp

One dab of the sculpting brush at a world point on a mesh: for scripts and tests. -p the point, -n the surface normal, -d the drag for Grab and Smear. Returns how many points it moved.

Arguments: a mesh

| Flag | Short | Description |
| --- | --- | --- |
| `position` | `p` | World centre. |
| `normal` | `n` | World normal. |
| `drag` | `d` | World drag since the last dab. |
| `invert` | `inv` | As Ctrl. |
| `smooth` | `sm` | As Shift. |

## select

Select objects. Replaces the selection unless add/deselect/toggle.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `add` | `add` |  |
| `deselect` | `d` |  |
| `toggle` | `tgl` |  |
| `clear` | `cl` | Select nothing. |

## selectAllComponents

Select every component of the current kind on the hilited meshes.

## selectBorder

Select border edges: the perimeter of selected faces, or the whole open boundary through selected edges or vertices.

## selectByAngle

Grow the face selection across edges where neighbouring faces bend less than an angle.

| Flag | Short | Description |
| --- | --- | --- |
| `angle` |  | Largest bend between neighbouring faces, degrees. |

## selectComponents

Select vertices, edges or faces of a mesh by id. Ids may be [first, last] ranges. Without add/deselect/toggle it replaces the selection of that kind.

Arguments: mesh

| Flag | Short | Description |
| --- | --- | --- |
| `vertices` |  | Point ids. |
| `edges` |  | Edge ids. |
| `faces` |  | Face ids. |
| `add` |  | Add to the selection. |
| `deselect` |  | Remove from the selection. |
| `toggle` |  | Toggle. |
| `symmetric` |  | Also the mirror images, when the mesh's symmetry is on. |

## selectComponentsWhere

Select a mesh's components by predicates (the flags of findComponents) and switch to that component mode. With symmetry on, edits still mirror; select one side. Returns the selected ids.

Arguments: [mesh]

| Flag | Short | Description |
| --- | --- | --- |
| `kind` | `k` | vertex, edge or face. |
| `normal` | `n` | Keep components whose normal is within angle of this direction (world space). |
| `angle` |  | Tolerance for normal=, degrees. |
| `side` | `s` | '+x', '-x', '+y', ... keeps components whose centre is on that side of the object-space origin plane; 'center' (or 'centerx', 'centery', 'centerz') keeps those on the plane. |
| `box` | `b` | [[minX, minY, minZ], [maxX, maxY, maxZ]], world space: keep components whose centre is inside. None leaves that side open, so [[None, 20, None], [None, None, None]] is everything above y=20. |
| `direction` | `dir` | Edges only: keep edges running along this world direction (either way) within angle; (0, 1, 0) finds vertical edges, whose ring polyInsertLoop cuts with a horizontal loop. |
| `near` |  | World point; with radius= keeps components within it, else keeps the count nearest. |
| `radius` | `r` | Distance for near=, cm. |
| `extreme` | `x` | '+y', '-z', ...: keep the count components furthest along that world direction (the top face is extreme='+y'). |
| `count` | `c` | How many to keep for extreme= or near= without radius. |
| `border` |  | True keeps only components on an open border, False only interior ones. |
| `shell` |  | Keep components of one connected shell: 0 is the largest, numbered as meshCheck lists them. |
| `fromSelection` | `fs` | Only consider the currently selected components of that kind. |
| `tolerance` | `t` | Distance counted as on a plane for side=, cm (default scales with the mesh). |
| `add` |  | Add to the selection of that kind instead of replacing it. |
| `deselect` |  | Remove the matches from the selection. |

## selectConnected

Select every component of a kind on the connected piece (faces joined through shared edges) that one component belongs to. Without add/deselect/toggle it replaces the selection of that kind. Returns the ids.

Arguments: mesh

| Flag | Short | Description |
| --- | --- | --- |
| `kind` |  | vertex \| edge \| face |
| `id` |  | A component of the piece to select. |
| `add` |  | Add to the selection. |
| `deselect` |  | Remove from the selection. |
| `toggle` |  | Toggle. |
| `symmetric` |  | Also the mirror images, when the mesh's symmetry is on. |

## selectContainedFaces

Select the faces enclosed by the selected edges: a flood fill that stops at selected edges and mesh borders. On each piece of a mesh the smaller regions are kept (every one but the largest by area), or with larger=True only the largest. Switches to face mode.

| Flag | Short | Description |
| --- | --- | --- |
| `larger` |  | Keep the largest region instead of the smaller ones. |

## selectDisplayLayerMembers

Select the objects in display layers.

Arguments: layers

| Flag | Short | Description |
| --- | --- | --- |
| `add` |  | Add them to the selection instead of replacing it. |

## selectFoldEdges

Select fold edges of meshes by the autoBevel rule (on the editable cage). Returns the number selected.

Arguments: meshes

| Flag | Short | Description |
| --- | --- | --- |
| `add` |  | Add to the edge selection instead of replacing it. |
| `minAngle` |  | Smallest fold, degrees between the face normals. |
| `maxAngle` |  | Largest fold, degrees. |
| `side` |  | both, outside (convex) or inside (concave). |
| `thin` |  | Skip thickness edges narrower than this, cm. |
| `minLength` |  | Skip edges shorter than this, cm. |
| `completeCorners` |  | Add the quad loops joining folds that meet at an angle. |

## selectHierarchy

Add every transform below each selected object to the selection, parents before children.

## selectInstanceSource

Select the source of each given instance or reference (a source selects itself). Returns the selection.

Arguments: meshes

## selectInstances

Select every mesh sharing data with the given ones: their source, instances and references. Returns the selection.

Arguments: meshes

| Flag | Short | Description |
| --- | --- | --- |
| `add` |  | Add to the selection instead of replacing it. |

## selectKey

Select keys, for the commands that take -sl. -t limits them; -add, -rm and -tgl change the selection; -cl clears it. -q -kc counts them.

Arguments: objects, plugs or curves (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `attribute` | `at` | Channel name(s). |
| `time` | `t` | A frame or range. |
| `add` | `add` | Add to the selection. |
| `remove` | `rm` | Remove from the selection. |
| `toggle` | `tgl` | Toggle in the selection. |
| `clear` | `cl` | Clear the selection. |
| `keyframeCount` | `kc` | Query: how many selected. |
| `query` | `q` | Query mode. |
| `edit` | `e` | Edit mode. |

## selectLoop

Extend the selection to full loops (or rings with ring=True): edges along their own loops, vertices along the loops of the edges joining them, faces along the face loops crossing the edges they share.

| Flag | Short | Description |
| --- | --- | --- |
| `ring` |  | Select rings instead of loops. Faces have no ring and take their loops. |

## selectLoopThrough

Select the edge loop (or ring) through one edge of a mesh, or with faces=True the face loop crossing it. Without add/deselect/toggle it replaces the selection of that kind. Returns the ids.

Arguments: mesh

| Flag | Short | Description |
| --- | --- | --- |
| `edge` |  | The edge the loop runs through. |
| `ring` |  | The edge ring instead of the loop. |
| `faces` |  | The faces of the edge's ring: the face loop crossing the edge. |
| `add` |  | Add to the selection. |
| `deselect` |  | Remove from the selection. |
| `toggle` |  | Toggle. |
| `symmetric` |  | Also the mirror images, when the mesh's symmetry is on. |

## selectMode

Switch between object, vertex, edge and face selection. With no argument, returns the current mode.

Arguments: [mode]

## selectPathBetween

Select the components between two of a mesh's vertices, edges or faces: the shorter way along a loop or ring they share, else the shortest path across the surface. Neighbours (vertices on one edge, edges along a loop or across a quad, faces sharing an edge) select the whole loop or ring through both instead. Adds to the selection unless replace or deselect. Returns the ids in order.

Arguments: mesh

| Flag | Short | Description |
| --- | --- | --- |
| `kind` |  | vertex \| edge \| face |
| `from` |  | Id the path starts at. |
| `to` |  | Id the path ends at. |
| `replace` |  | Replace the selection of that kind. |
| `deselect` |  | Remove the path from the selection. |
| `symmetric` |  | Also the mirror images, when the mesh's symmetry is on. |

## selectSetMembers

Select what sets hold: their objects, and their components in component mode. Returns the members.

Arguments: sets

| Flag | Short | Description |
| --- | --- | --- |
| `add` |  | Add them to the selection instead of replacing it. |

## setAttr

setAttr('node.attr', value). Vec3 takes a list or three numbers. lock=True/False locks or unlocks the plug, with or without a value: a locked plug can't be changed until it is unlocked.

Arguments: plug [value]

| Flag | Short | Description |
| --- | --- | --- |
| `lock` | `l` | Lock (True) or unlock (False) the plug. |

## setDrivenKeyframe

Set a driven key: at the driver's value (-cd plug, -dv value, or its current one) the channel takes its value (-v, or its current one). -q -dr lists a channel's drivers, -q -dn a driver's driven channels.

Arguments: the driven object or channels (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `currentDriver` | `cd` | The driving attribute. |
| `driverValue` | `dv` | The driver's value at this key (its current one by default). |
| `value` | `v` | The channel's value at this key (its current one by default). |
| `attribute` | `at` | Channel name(s). |
| `inTangentType` | `itt` | Tangent into the key. |
| `outTangentType` | `ott` | Tangent out of the key. |
| `driver` | `dr` | Query: the channel's drivers. |
| `driven` | `dn` | Query: the driver's driven channels. |
| `query` | `q` | Query mode. |
| `edit` | `e` | Edit mode. |

## setInfinity

What curves do before and after their keys: -pri and -poi constant, linear, cycle, cycleRelative or oscillate.

Arguments: objects, plugs or curves (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `attribute` | `at` | Channel name(s). |
| `preInfinite` | `pri` | Before the first key. |
| `postInfinite` | `poi` | After the last key. |
| `query` | `q` | Query mode. |
| `edit` | `e` | Edit mode. |

## setKeyframe

Key channels at the current frame (or -t). Objects key all their keyable channels unless -at names some. Returns the number of keys set.

Arguments: objects or object.attribute plugs (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `attribute` | `at` | Channel name(s) to key: tx, rotateY... |
| `time` | `t` | Frame(s) to key at. |
| `value` | `v` | Value to key instead of the current one. |
| `inTangentType` | `itt` | Tangent into the key. |
| `outTangentType` | `ott` | Tangent out of the key. |
| `breakdown` | `bd` | The key is a breakdown: it keeps its place between the keys around it. |
| `hierarchy` | `hi` | Accepted for Maya scripts. |
| `controlPoints` | `cp` | Accepted for Maya scripts. |
| `shape` | `s` | Accepted for Maya scripts. |

## setMembers

A set's members as names and components (pCube1, pCube1.vtx[0:3]). nodes=True gives the distinct objects instead, component owners included.

Arguments: set

| Flag | Short | Description |
| --- | --- | --- |
| `nodes` |  | Return the objects the set touches instead of its members. |

## setMeshPoints

Move every point of a mesh's base (under its modifiers) at once, in meshData's point order, in object space. Topology stays. One undo step. Python's Node.set_points takes a numpy array, faster.

Arguments: mesh

| Flag | Short | Description |
| --- | --- | --- |
| `points` |  | [[x, y, z], ...], one per point, in cm. |
| `buffer` |  | Internal: points handed over from Python (Node.set_points). |

## setPivot

Move the rotate and scale pivots to a world position, or orient the pivot, without moving the objects. A joint's pivot is the joint itself: position= moves the joint and orient= sets its jointOrient, leaving its children where they are.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `position` |  | World position, cm. |
| `orient` |  | Pivot orientation relative to the object's axes, Euler XYZ degrees (pivotOrient); on a joint, its jointOrient. |

## setSymmetry

Object-space symmetry for component edits on meshes: 'off', a mirror plane 'x', 'y' or 'z', or 'radial': count wedges repeated round radialAxis (with mirror=True also mirrored within each wedge), every edit applied to all of them.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `axis` |  | off, x, y, z or radial. |
| `count` |  | Radial: how many wedges repeat round the axis, 2 to 64; default: the mesh's own. |
| `radialAxis` |  | Radial: the axis, x, y, z or custom (direction=); default: the mesh's own. |
| `direction` |  | Radial: a custom axis direction, object space. |
| `mirror` |  | Radial: also mirror each wedge across a plane through the axis (dihedral symmetry). |
| `phase` |  | Radial: degrees the wedge boundaries and mirror planes turn about the axis. |

## setSymmetryPlane

Set where meshes' symmetry plane lies: through the selected edges or vertices (which must lie flat across an axis; that axis becomes the symmetry axis), or back through the pivot with pivot=True. With radial symmetry the radial axis passes through the selection's centre instead. Returns the plane per mesh as [axis, offset], or ['radial', centre].

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `pivot` |  | Put the plane (or radial axis) back through the pivot. |

## show

Show objects. all=True shows every hidden object.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `all` |  | Show everything. |

## showLastHidden

Show the objects the last hide (or hiding toggle) hid. Returns them.

## shrinkSelection

Shrink the component selection by dropping what lies on its border.

## skinCluster

Bind skin: the mesh named last (or selected last) follows the joints, weighted by closest distance. -mi caps the influences per point, -dr sets the falloff, -bm the bind method, -sm the skinning method, -tsb binds to only the joints given. Edit: -ub unbinds, -ubk unbinds keeping history, -ai and -ri add and remove influences, -lw with -inf locks weights, -fnw normalizes, -sw smooths the selected points. Query: -inf, -wi, -g, -mi, -dr, -sm, -lw.

Arguments: joints, then the mesh (the selection when none); a skinCluster or skinned mesh with -e and -q

| Flag | Short | Description |
| --- | --- | --- |
| `toSelectedBones` | `tsb` | Only the joints given, not their whole skeleton. |
| `maximumInfluences` | `mi` | Most joints per point (5). |
| `obeyMaxInfluences` | `omi` | Hold points to maximumInfluences while weights are edited. |
| `dropoffRate` | `dr` | Falloff with distance (4). |
| `bindMethod` | `bm` | 0 closest distance, 1 closest in hierarchy, 2 heat map, 3 geodesic voxel. |
| `skinMethod` | `sm` | 0 classic linear, 1 dual quaternion. |
| `normalizeWeights` | `nw` | 0 none, 1 interactive, 2 post. |
| `weightDistribution` | `wd` | Accepted for Maya scripts. |
| `includeHiddenSelections` | `ihs` | Accepted for Maya scripts. |
| `name` | `n` | Name of the skinCluster. |
| `unbind` | `ub` | With -e: remove the skin, back to the bind pose. |
| `unbindKeepHistory` | `ubk` | With -e: stop the skinning, keeping the node and its weights. |
| `addInfluence` | `ai` | With -e: joints to add. |
| `removeInfluence` | `ri` | With -e: joints to remove; their weight goes to the others. |
| `removeUnusedInfluence` | `rui` | With -e: remove influences that hold no weight. |
| `weight` | `wt` | With -ai: starting weight of the added influence. |
| `influence` | `inf` | With -e -lw: the joint. Query: the influences. |
| `weightedInfluence` | `wi` | Query: influences holding weight. |
| `geometry` | `g` | Query: the deformed shape. |
| `lockWeights` | `lw` | With -inf: lock or unlock an influence's weights; alone on -e, every influence. |
| `forceNormalizeWeights` | `fnw` | With -e: normalize the selected points now. |
| `smoothWeights` | `sw` | With -e: smooth the selected points' weights by this much (0 to 1). |
| `smoothWeightsMaxIterations` | `swi` | With -sw: passes (2). |
| `recacheBindMatrices` | `rbm` | With -e: the joints' current pose becomes the bind pose. |
| `edit` | `e` | Edit mode. |
| `query` | `q` | Query mode. |

## skinPaintStamp

One dab of the paint skin weights brush at a world point, from the tool's settings: for scripts and tests. -inv paints the inverse, as Ctrl does in the tool; -sm smooths, as Shift does.

Arguments: a skinned mesh

| Flag | Short | Description |
| --- | --- | --- |
| `position` | `p` | World centre of the dab. |
| `invert` | `inv` | Paint the inverse. |
| `smooth` | `sm` | Smooth instead. |

## skinPercent

Set or query skin weights of points: skinPercent -tv joint1 1 skinCluster1 pCube1.vtx[0]. -tv joint value (multi), -r relative, -zri zeroes the rest, -tmw from to moves weight, -prw prunes weights below a value, -nrm normalizes, -rtd resets to the default bind. Query: -v the weights, -t the influences or one influence's weight, -ib ignores small ones.

Arguments: skinCluster, then points or components (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `transformValue` | `tv` | joint weight pairs. |
| `transformMoveWeights` | `tmw` | from to pairs: moves the weight of one influence to another. |
| `relative` | `r` | -tv adds to the current weight. |
| `zeroRemainingInfluences` | `zri` | -tv zeroes the influences it doesn't name. |
| `pruneWeights` | `prw` | Removes weights below this value, normalizing what's left. |
| `normalize` | `nrm` | Normalizes the points. |
| `resetToDefault` | `rtd` | Back to the weights of a default bind. |
| `ignoreBelow` | `ib` | Query: skip influences below this value. |
| `transform` | `t` | Query: one influence's weight, or the list of influences. |
| `value` | `v` | Query: weights per influence. |
| `edit` | `e` | Edit mode. |
| `query` | `q` | Query mode. |

## smoothCurve

Smooth the CVs: -s how far, -i passes. Returns [curve, node].

Arguments: curves (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `smoothness` | `s` | How far each pass moves. |
| `iterations` | `i` | Passes. |
| `constructionHistory` | `ch` | Keep a history node (on by default). |
| `replaceOriginal` | `rpo` | Change the curve itself (on by default). |
| `name` | `n` | Name of a new curve. |
| `object` | `o` | Make an object (accepted for Maya scripts). |

## smoothPreview

Display meshes subdivided without changing them (the 1/2/3 keys), including every mesh under a given group. level 0 is off.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `level` |  | Subdivision levels to display (0 to 3). |

## snapKey

Move keys to whole multiples of -tm (1 by default), and to multiples of -vm if given.

Arguments: objects, plugs or curves (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `attribute` | `at` | Channel name(s). |
| `time` | `t` | A frame or range. |
| `timeMultiple` | `tm` | Snap time to multiples of this. |
| `valueMultiple` | `vm` | Snap value to multiples of this. |

## softMod

Add a soft modification to the selected vertices or meshes: softMod -fr 2 -fas true. Returns [softMod1, softMod1Handle]. Move the handle and points follow by their falloff. With vertices it falls off around them; with a mesh, around its centre or -fc. -e and -q on the softMod or handle: -fr, -fom, -fc, -fas, -rel, -en.

Arguments: vertices or meshes (the selection when none); the softMod with -e and -q

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the deformer. |
| `before` | `bf` | Go just above the deformer nearest the mesh. |
| `after` | `af` | Go nearest the mesh, after the other deformers (the default). |
| `frontOfChain` | `foc` | Go above every other deformer. |
| `envelope` | `en` | How much of the deformation applies (1). |
| `edit` | `e` | Edit mode. |
| `query` | `q` | Query mode. |
| `falloffRadius` | `fr` | How far the effect reaches, cm (5). |
| `falloffMode` | `fom` | 0 straight through space, 1 along the surface. |
| `falloffCenter` | `fc` | Centre of the falloff, world space. |
| `falloffAroundSelection` | `fas` | Fall off around the selected points rather than the centre (on). |
| `relative` | `rel` | Only the handle's own transform counts, not its parents'. |

## softSelect

Soft selection (B): -sse 1 turns it on, -ssd sets the falloff radius. -q -sse / -q -ssd query them.

| Flag | Short | Description |
| --- | --- | --- |
| `softSelectEnabled` | `sse` | 1 on, 0 off. |
| `softSelectDistance` | `ssd` | Falloff radius, world units. |
| `softSelectFalloff` | `ssf` | Accepted for Maya scripts (0 volume, 1 surface). |
| `query` | `q` | Query mode. |

## softSelectCurve

Set or query the soft select falloff curve (weight against distance from the selection, 0 at the selection, 1 at the radius). With no flags it returns the current curve.

| Flag | Short | Description |
| --- | --- | --- |
| `keys` | `k` | Keys as [[pos, value, interp], ...]; interp is linear, smooth, spline or flat. |
| `preset` | `p` | Apply a built-in or saved preset by name. |
| `falloffMode` | `fm` | Distance measured through space or along the surface. |
| `save` |  | Save the current curve (after keys/preset) as a user preset with this name. |
| `remove` |  | Delete the named user preset. |
| `rename` |  | User preset to rename; the new name goes in to. |
| `to` |  | New name for rename. |

## spaceLocator

Create a locator: a transform with a locator shape under it. Returns [transform].

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name; made unique among its siblings. |
| `position` | `p` | Where the locator shape sits in its transform's space (localPosition). |

## splitRingsApplySelection

Store the selected edges or faces (with their mirror images under symmetry) as what a splitRings modifier splits, and end Edit Selection. Returns how many it stored.

Arguments: modifier or mesh

## splitRingsCancelSelection

End a splitRings modifier's Edit Selection without changing what it splits.

Arguments: modifier or mesh

## splitRingsEditSelection

Show a splitRings modifier's input mesh (it and later modifiers pass through) with the edges or faces it splits selected, so they can be changed with the normal selection tools. Finish with splitRingsApplySelection or splitRingsCancelSelection.

Arguments: modifier or mesh

## sweepMeshFromCurve

A tube along a curve with a sweepMeshCreator node: -scaleProfileX radius, -profilePolySides, -interpolationSteps, -taper, -twist. Returns [sweep1, sweepMeshCreator1].

Arguments: a curve (the selection when none)

| Flag | Short | Description |
| --- | --- | --- |
| `scaleProfileX` | `spx` | Radius of the profile. |
| `profilePolySides` | `pps` | Sides of the profile. |
| `interpolationSteps` | `is` | Rings per span. |
| `taper` | `tp` | Radius scale at the end. |
| `twist` | `tw` | Twist at the end, degrees. |
| `polygon` | `po` | Accepted: the output is always polygons. |
| `constructionHistory` | `ch` | Keep the node (on by default). |
| `range` | `rn` | Accepted for Maya scripts. |
| `name` | `n` | Name of the new object. |

## symmetrize

Make meshes exactly symmetric: every point on one side of the symmetry plane is copied, reflected, onto its partner, and on-plane points are put on the plane. With radial symmetry, one wedge's points are copied round the axis onto their orbits.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `side` |  | The side that is kept. |
| `wedge` |  | Radial: the wedge that is kept; -1: the one most of the selected components are in, else 0. |

## symmetrizeRadial

Rebuild meshes exactly radially symmetric: the kept wedge is repeated round the axis (and mirrored within each wedge with mirror=True), replacing the rest, and welded along the seams and the axis; then the mesh's symmetry is set to radial with these settings, so every edit after keeps it. Settings default to the mesh's own radial symmetry. Model one wedge and run this to get the whole. Returns the faces per mesh.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `count` |  | Radial: how many wedges repeat round the axis, 2 to 64; default: the mesh's own. |
| `radialAxis` |  | Radial: the axis, x, y, z or custom (direction=); default: the mesh's own. |
| `direction` |  | Radial: a custom axis direction, object space. |
| `mirror` |  | Radial: also mirror each wedge across a plane through the axis (dihedral symmetry). |
| `phase` |  | Radial: degrees the wedge boundaries and mirror planes turn about the axis. |
| `wedge` |  | The wedge that is kept; -1: the one most of the selected components are in, else the one holding most of the mesh. |
| `tolerance` |  | Weld distance along the seams and the axis, cm (0: scaled to the mesh). |

## symmetrizeSelection

Add the mirror images of the component selection, on each mesh whose symmetry is on.

## symmetryCheck

Report how symmetric meshes are across their symmetry plane (or round their radial axis): the largest distance between a point and its partner's reflection (or its orbit's image), and the points without a partner. For radial symmetry also the unmatched point ids and the count the mesh fits best. Changes nothing.

Arguments: objects

## symmetryRebuild

Rebuild meshes symmetric: the kept side is mirrored across the plane through the object's pivot, replacing the other side, welded on the plane, and the symmetry pairs are stored again. A mesh with radial symmetry is rebuilt from its kept wedge instead (see symmetrizeRadial). With pairsOnly, the geometry is left alone and only the stored pairs are matched again, by position or, for meshes that are no longer symmetric, by topology outward from the selected edge (which must lie on the plane; any edge for radial symmetry). Returns the points left unmatched per mesh.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `axis` |  | x, y or z; default: the mesh's symmetry axis, else x. |
| `side` |  | The side that is kept; auto: the side of the selected components, else the side holding nearly all of the mesh, else positive. |
| `wedge` |  | Radial: the wedge that is kept; -1: the one most of the selected components are in, else the one holding most of the mesh. |
| `pairsOnly` |  | Only match the stored pairs again, leaving the geometry as it is. |
| `topological` |  | With pairsOnly: match by topology from the selected edge. |
| `tolerance` |  | Weld or match distance, cm (0: scaled to the mesh). |

## tension

Add a tension deformer: edges pull back towards their lengths in the mesh as it is now. Returns [tension1].

Arguments: meshes or vertices (the selection when none); the deformer with -e and -q

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the deformer. |
| `before` | `bf` | Go just above the deformer nearest the mesh. |
| `after` | `af` | Go nearest the mesh, after the other deformers (the default). |
| `frontOfChain` | `foc` | Go above every other deformer. |
| `envelope` | `en` | How much of the deformation applies (1). |
| `edit` | `e` | Edit mode. |
| `query` | `q` | Query mode. |
| `smoothingIterations` | `si` | Smoothing passes (10). |
| `smoothingStep` | `ss` | How far each pass moves a point towards its neighbours (0.5). |
| `pinBorderVertices` | `pbv` | Border points stay out of the smoothing (on). |
| `pin` | `pin` | The same as -pinBorderVertices. |
| `inwardConstraint` | `ic` | Stops points moving in along the normal: 0 free, 1 not at all. |
| `outwardConstraint` | `oc` | Stops points moving out along the normal: 0 free, 1 not at all. |
| `squashConstraint` | `sqc` | Stops squashed edges relaxing: 0 free, 1 not at all. |
| `stretchConstraint` | `stc` | Stops stretched edges relaxing: 0 free, 1 not at all. |

## toggleSymmetry

Symmetry off on the meshes if every one already has it on (on axis=, when given), else on for all of them. Non-meshes are skipped. Returns the axis set, or 'off'.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `axis` |  | x, y, z or radial to turn on with; default: the axis already in use, else the last one picked. |

## toggleVisibility

Toggle visibility (the H key). Defaults to the selection. Every object ends up the same: hidden if most were visible (a tie hides), shown otherwise. Returns the new visibility.

Arguments: objects

## u3dLayout

Layout: pack the UV shells of the selection into 0 to 1, each scaled to its size in 3D and turned to fit.

Arguments: faces, UVs or meshes

| Flag | Short | Description |
| --- | --- | --- |
| `shellSpacing` | `spc` | Space between shells, in UV units. |
| `rotateStep` | `rst` | 0 keeps shells unturned. |
| `resolution` | `res` | Accepted for Maya scripts. |

## u3dUnfold

Unfold: flatten the UV shells of the selection with the least stretch, keeping each shell's place and size.

Arguments: faces, UVs or meshes

| Flag | Short | Description |
| --- | --- | --- |
| `iterations` | `ite` | Accepted for Maya scripts. |
| `pack` | `p` | Accepted for Maya scripts. |
| `borderintersection` | `bi` | Accepted for Maya scripts. |
| `triangleflip` | `tf` | Accepted for Maya scripts. |

## undo

Undo the last command.

## undoInfo

Query or control the undo queue. openChunk/closeChunk group everything in between into one undo step.

| Flag | Short | Description |
| --- | --- | --- |
| `openChunk` | `ock` | Start a chunk. |
| `closeChunk` | `cck` | End a chunk. |
| `chunkName` | `cn` | Label for openChunk. |
| `state` | `st` | Turn undo recording on or off. |
| `limit` | `l` | Max undo steps, 0 for infinite. |
| `memoryBudget` | `mb` | Drop the oldest steps past this many MB, 0 for unlimited. |
| `history` | `h` | Also return every step's label, oldest first, and the current position. |

## undoJump

Undo or redo in one go until the given number of undo steps remain, as listed by undoInfo(history=True).

| Flag | Short | Description |
| --- | --- | --- |
| `position` | `p` | Undo steps left afterwards: 0 undoes everything. |

## unfrozenTransforms

List the objects whose translate, rotate or scale is not identity (0, 0, 0 and 1, 1, 1), with their values. Checks the objects given or selected, or with scene every mesh, curve and group (joints, cameras, lights and locators are placed by their transforms and are left out). With rotateStep, rotations that are whole multiples of the step count as frozen; invert lists only rotations on the step instead. Returns one {name, translate, rotate, scale, unfrozen} per object listed, unfrozen naming the channels that put it there.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `scene` |  | Check the whole scene instead of the objects. |
| `hierarchy` |  | Also check everything parented under the objects. |
| `translate` |  | List objects with translation. |
| `rotate` |  | List objects with rotation. |
| `scale` |  | List objects with scale. |
| `rotateStep` |  | Degrees: rotations on whole multiples of it count as frozen. 0 is off. |
| `invert` |  | With rotateStep, list rotations on the step rather than off it. |
| `tolerance` |  | How far from identity, or from the step, still counts as on it. |

## ungroup

Move each group's children up to the group's parent, keeping where they are, then delete the group. Defaults to the selection; returns the children.

Arguments: groups

## wire

Make points follow a curve: wire -w curve1 -dds 0 2.0 -en 1 -ce 0 -li 0 -n name mesh. Returns [wire1]. With -e on a wire: -w adds a wire, -dds and the settings change. With -q: -w and the settings.

Arguments: meshes to deform, then the wire unless -w names it (the selection when none); the wire with -e and -q

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the deformer. |
| `before` | `bf` | Go just above the deformer nearest the mesh. |
| `after` | `af` | Go nearest the mesh, after the other deformers (the default). |
| `frontOfChain` | `foc` | Go above every other deformer. |
| `envelope` | `en` | How much of the deformation applies (1). |
| `edit` | `e` | Edit mode. |
| `query` | `q` | Query mode. |
| `wire` | `w` | The curve to follow, or several. Any shape with points will do. |
| `dropoffDistance` | `dds` | Wire index, then how far it reaches, cm (1). |
| `groupWithBase` | `gw` | Accepted and ignored: the base wire is held on the node, so there is nothing to group. |
| `rotation` | `rt` | How much points turn with the wire: 0 not at all, 1 fully (1). |
| `crossingEffect` | `ce` | Where wires cross: 0 they blend, 1 they add up (0). |
| `tension` | `tn` | How tightly the effect gathers to the wire (1). |
| `localInfluence` | `li` | Where wires cross: how much a still wire holds its points, 0 to 1 (0). |

## wrap

Make meshes follow a wrapper: wrap -inf wrapper mesh, or select the meshes and then the wrapper. Returns [wrap1]. With -e on a wrap (say one from deformer -type wrap): -inf adds an influence and binds again, -ri removes one, -rb binds again as things are now. With -q: -inf and the settings.

Arguments: meshes to deform, then the wrapper unless -inf names it (the selection when none); the wrap with -e and -q

| Flag | Short | Description |
| --- | --- | --- |
| `name` | `n` | Name of the deformer. |
| `before` | `bf` | Go just above the deformer nearest the mesh. |
| `after` | `af` | Go nearest the mesh, after the other deformers (the default). |
| `frontOfChain` | `foc` | Go above every other deformer. |
| `envelope` | `en` | How much of the deformation applies (1). |
| `edit` | `e` | Edit mode. |
| `query` | `q` | Query mode. |
| `influence` | `inf` | The wrapper mesh, or several. With -e: add. Query: the influences. |
| `removeInfluence` | `ri` | With -e: the influence to remove. |
| `rebind` | `rb` | With -e: bind again as things are now. |
| `maxDistance` | `md` | How far a point looks for triangles to bind to, cm (1); 0 has no limit. |
| `exclusiveBind` | `ebn` | Each point binds to its nearest triangle only. |
| `autoWeightThreshold` | `awt` | Every point binds to its nearest triangle, however far (on). |
| `weightThreshold` | `wth` | With -awt off: drop bindings of less weight than this. |
| `falloffMode` | `fom` | 0 any triangle in reach, 1 only ones touching the nearest. |

## xform

Query or set the transform of objects, in local or world space. Sets every flag given on each object, or with query=True returns one value of the first object: pass the flag to read as True. Values are local to the parent unless worldSpace is on.

Arguments: Transforms to edit or query; the selection when none are given.

| Flag | Short | Description |
| --- | --- | --- |
| `query` | `q` | Return a value of the first object instead of setting. Pick it by passing one of translation, rotation, scale or matrix as True. |
| `worldSpace` | `ws` | Read and write in world space instead of relative to the parent. Rotation and scale queries stay local. |
| `relative` | `r` | On components: -t moves by the offset. |
| `absolute` | `a` | The values are absolute (the default). |
| `objectSpace` | `os` | Object space (the default). |
| `translation` | `t` | Position as (x, y, z) to set, or True to read it with query. |
| `rotation` | `ro` | Euler rotation in degrees as (x, y, z) to set, or True to read it with query. |
| `scale` | `s` | Scale as (x, y, z) to set, or True to read it with query. |
| `matrix` | `m` | The whole transform as 16 floats, column-major, to set; or True to read it with query. |

## zeroPivot

Move the pivots to the world origin, keeping the object where it is.

Arguments: objects

## zeroPivotRotation

Line the pivots' axes up with the world (or with the object's own axes), leaving their positions and the objects alone.

Arguments: objects

| Flag | Short | Description |
| --- | --- | --- |
| `space` |  | Axes to line up with. |

