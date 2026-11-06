# Aztek: working notes

## What it is

Aztek is a modelling, UV, rigging and animation application that behaves like Maya, released by Simon (srlegrand) at
https://github.com/srlegrand/AKA-Crest-Aztek. It started as a Rust port of Simon's C++ app Lila; Lila is now a parts
bin, and the target is Maya's behaviour. PARITY.md holds the target and status; PORTING.md the history of the port.

## Rules

- Maya's behaviour is the spec: command names, flags and short names, query/edit modes, return values, node and
  attribute names, defaults, hotkeys, menus, marking menus, channel box colours. When in doubt, do what Maya does.
- No Maya name, logo, icons or branding anywhere in the product, and no copying of Maya's artwork or logo lettering.
  Maya is the behavioural reference only. The word "lila" must not appear in the Rust code.
- The Python API is `aztek.cmds`; `maya.cmds` / `maya.mel` are thin shims onto it so simple Maya scripts run.
- Scope: modelling, UVs, rigging, animation, viewport navigation, manipulators, script editor, preferences. Not
  rendering, dynamics or Bifrost.
- Never add AI or Claude attribution anywhere: no Co-Authored-By, session links or "Generated with" lines in commits,
  no credits in files, READMEs or comments.
- Never `git push` or `git pull`; Simon does those himself.
- Writing style in docs and comments: dry and plain, no em dashes, dates as DD/MM/YYYY.
- Licence intent: Apache 2.0 (not yet added; don't add a LICENSE file unless asked).

## Layout

- `crates/aztek_core` prefs, paths. `aztek_undo` undo stack. `aztek_ops` command layer (CommandDef, flags, execute).
- `crates/aztek_geo` mesh (half-edge style corners, attributes; UVs are per-corner `uv0`, seams edge `seam`), ops.
- `crates/aztek_uv` charts, unwrap (LSCM, SLIM, BFF), layout helpers, projections.
- `crates/aztek_scene` the scene: graph, nodes, plugs, DAG (transform over shape: pCube1 / pCubeShape1),
  construction history (`history.rs`: one node per edit, polyTweak for point moves), components, mesh edits,
  `anim.rs` (animCurveTL/TA/TU, setKeyframe, keyframe, cutKey, keyTangent, playbackOptions, currentTime in frames,
  autoKeyframe, held values), `uv_ops.rs` (maps and shells from corner UVs, projections, unfold, layout, cut, sew,
  polyEditUV; each a history node holding a UvEdit), `rig.rs` (joint, orientJoint, skinCluster, skinPercent,
  constraints, ikHandle RP/SC), `deform.rs` (nonLinear, cluster, blendShape), `maya_cmds.rs` (polyEvaluate,
  listConnections, filterExpand, ...). New commands register in `session.rs`.
- `crates/aztek_host` static library the UI links: C ABI (`lib.rs`, header `app/src/aztek.h`), `core.rs` (Core:
  exec with MEL echo, exec_silent for previews, retract_preview, query/edit routing, UI runtime commands like
  GraphEditor, MoveTool), `mel.rs` (MEL parsing, Maya command and flag aliases), `query.rs` (JSON snapshots for the
  panels), `view.rs` / `viewport.rs` / `render.rs` / `gpu.rs` (ash Vulkan viewport, picking, manipulators, joints and
  handles drawn as lines, snapping X/V, pivot mode D), `python.rs` + `python/bootstrap.py` (pyo3).
- `app/src` Qt 6 Widgets shell in C++17: `mainwindow.*` (menus by menu set, shelves, hotkeys, time slider, editors),
  `panels.*` (outliner, channel box with picked channels and virtual slider, layer and attribute editors, script
  editor), `viewport.*`, `widgets.*` (option boxes, time and range sliders, splash, About), `grapheditor.*`,
  `uveditor.*`, `markingmenu.*`, `icons.cpp` (icons drawn with QPainter), `style.qss`.
- `app/icons` logo and app icon (SVG sources, PNG, ICO). `docs/` README screenshots.

The UI talks to the core only through commands (exec, exec_quiet, ask) and JSON queries; panels refresh when the
core's version counter moves (Host poller, 50 ms, or poke()). Drags preview with silent commands and run the final
command once on release so the history shows one Maya-style line.

## Build and test

    ./build.sh                 # Linux; --deps installs packages, --run starts it, --install puts it in ~/.local
    build.bat                  # Windows (PowerShell script build.ps1)
    cargo test --workspace     # all Rust tests; keep them green

The GitHub Action `.github/workflows/build.yml` builds Linux and Windows on every push to main.

Checking the UI without a display: run under Xvfb and look at the pictures.

    xvfb-run -a -s "-screen 0 1700x1050x24" ./build/aztek --script scene.mel --screenshot out.png

`--script` runs MEL or Python at start; `--screenshot` saves the main window (viewports included) and quits. For
interaction (marking menus, drags, separate editor windows) drive a real Xvfb with xdotool and grab the screen with
ImageMagick `import -window root`. `AZTEK_ECHO=1` prints the script history to stderr. Runtime commands such as
`GraphEditor;`, `UVEditor;`, `FourViewLayout;`, `JointTool;`, `MoveTool;` let a script open editors and set tools.

## Working loop

Pick the next gap from PARITY.md ("Not yet") or below, implement it to Maya's behaviour, add tests in the crate's
`tests/`, build, check it in the UI with screenshots, update PARITY.md's status with the date. Commit locally in small
steps with plain messages (no attribution); Simon pushes.

## Next, roughly in order

- Paint skin weights (artisan-style brush in the viewport), mirror and copy skin weights.
- Dope sheet; set driven keys; breakdowns; copy and paste keys.
- NURBS curves (CV curve tool, `curve`, `circle` controls for rigs), extrude and sweep along curves.
- Lattice, wrap, wire, delta mush, softMod.
- Spline IK, pole vector constraint, mirror joints, insert joint tool.
- Quad Draw, Create Polygon, Append to Polygon, Slide Edge, Multi-Cut parity.
- Tool Settings window (double-click a tool): soft select falloff, move axis orientation (world/object/component).
- MEL: variables, arrays, for/foreach/if/while, procedures, backticks, `eval`.
- OpenMaya API 2.0 subset in Python (MSelectionList, MDagPath, MFnMesh, MPoint/MVector/MMatrix, MPxCommand plugins).
- `.ma` read and write for exchange.
- Outliner: cameras and default sets as in Maya; Node Editor.
