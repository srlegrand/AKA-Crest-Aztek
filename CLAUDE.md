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
  constraints, ikHandle), `ik.rs` (RP, SC, spline), `constraint.rs`, `skin.rs` (weights, painting, mirror, copy),
  `deform*.rs` (nonLinear, cluster, blendShape, lattice, wrap, wire, delta mush, softMod and the rest), `curves.rs` and
  `surfaces.rs` (NURBS), `sculpt.rs`, `hik.rs` (HumanIK subset), `ma_file.rs` and `ma_file/deformers.rs` (.ma read and
  write), `references.rs`, `maya_cmds.rs` (polyEvaluate, listConnections, filterExpand, ...). New commands register in
  `session.rs`.
- `crates/aztek_host` static library the UI links: C ABI (`lib.rs`, header `app/src/aztek.h`), `core.rs` (Core:
  exec with MEL echo, exec_silent for previews, retract_preview, query/edit routing, UI runtime commands like
  GraphEditor, MoveTool), `mel.rs` (MEL parsing, Maya command and flag aliases), `query.rs` (JSON snapshots for the
  panels), `view.rs` / `viewport.rs` / `render.rs` / `gpu.rs` (ash Vulkan viewport, picking, manipulators, joints and
  handles drawn as lines, snapping X/V, pivot mode D), `tools.rs` (Multi-Cut, Quad Draw and other view tools),
  `python.rs` + `python/bootstrap.py` (pyo3), `scriptui.rs`, `panel_cmds.rs`, `ui_cmds.rs`, `ctx_cmds.rs` and
  `script_jobs.rs` (script-built UI, panels, tool contexts, scriptJob), `mel/` (the MEL interpreter).
- `app/src` Qt 6 Widgets shell in C++17: `mainwindow.*` (menus by menu set, shelves, hotkeys, time slider, editors),
  `panels.*` (outliner, channel box with picked channels and virtual slider, layer and attribute editors, script
  editor), `viewport.*`, `widgets.*` (option boxes, time and range sliders, splash, About), `grapheditor.*`,
  `uveditor.*`, `dopesheet.*`, `nodeeditor.*`, `toolsettings.*`, `shapeeditor.cpp`, `runtime.cpp` (script-built windows),
  `markingmenu.*`, `icons.cpp` (icons drawn with QPainter), `style.qss`.
- `app/icons` logo and app icon (SVG sources, PNG, ICO). `docs/` README screenshots.

The UI talks to the core only through commands (exec, exec_quiet, ask) and JSON queries; panels refresh when the
core's version counter moves (Host poller, 50 ms, or poke()). Drags preview with silent commands and run the final
command once on release so the history shows one Maya-style line.

## Build and test

    ./build.sh                 # Linux; --deps installs packages, --run starts it, --install puts it in ~/.local
    build.bat                  # Windows (PowerShell script build.ps1)
    cargo test --workspace     # all Rust tests; keep them green

The GitHub Action `.github/workflows/build.yml` builds Linux, Rocky Linux 8 and 9 and Windows on every push to main and attaches
the packages to the release. `packaging/` holds the RPM spec, the deb and Windows installer scripts and the install checks; see
`docs/PACKAGING.md`.

Checking a real `.ma` with no UI: `cargo run --release -p aztek_scene --example load_ma -- file.ma` lists warnings and how far the
rig is from what the file stored; `--example rig_diff -- a.ma b.ma node.attr=value` compares two files under a pose. Rig files and their
textures from elsewhere stay out of the repo: tests use small `.ma` text written by hand (`tests/rig_files.rs`).

Checking the UI without a display: run under Xvfb and look at the pictures.

    xvfb-run -a -s "-screen 0 1700x1050x24" ./build/aztek --script scene.mel --screenshot out.png

`--script` runs MEL or Python at start and leaves the camera where the script or file put it (`viewFit -all` frames); `--screenshot` saves the main window (viewports included) and quits. For
interaction (marking menus, drags, separate editor windows) drive a real Xvfb with xdotool and grab the screen with
ImageMagick `import -window root`. `AZTEK_ECHO=1` prints the script history to stderr. Runtime commands such as
`GraphEditor;`, `UVEditor;`, `FourViewLayout;`, `JointTool;`, `MoveTool;` let a script open editors and set tools.

## Working loop

Pick the next gap from PARITY.md ("Not yet") or below, implement it to Maya's behaviour, add tests in the crate's
`tests/`, build, check it in the UI with screenshots, update PARITY.md's status with the date. Commit locally in small
steps with plain messages (no attribution); Simon pushes.

## Next

Everything on the original list is in (paint weights, dope sheet, set driven keys, NURBS, the deformers, spline IK, Quad
Draw and friends, Tool Settings, MEL, OpenMaya, `.ma`, Node Editor). What is left is in PARITY.md under "Not yet" and in
each dated section's list of what is shallower than Maya. None of it has been checked against a real copy of Maya, so
the most useful next step is opening real files from Maya and running real Maya scripts.
