# Tickets

Requests waiting or in progress, newest detail kept. Move to Done with the date when reported finished.

## In progress
- History: every modelling action leaves history and shows in the Node Editor and Hypergraph; `G` after each. Marking menus call non-Maya commands such as `polySubdivide` and `polyDissolve`.
- Smooth Preview (1, 2, 3) wire as Maya draws it: main lines follow the smoothed surface, subdivision lines lighter.
- Skin core and deformation speed: viewport staleness fixed; root control on Lou still about 115 ms a change.
- First class USD (agent).
- Viewport lights, all default types (agent).
- Anim layers and HumanIK retargeting workflow (agent).
- Symmetry on all polygon actions; soft select stuck, B not toggling, hold B radius circle; remaining menu items and option boxes (agent).

## Waiting
- File > Examples: demo `.ma` scenes in the repo, shipped with the packages. After the above merge.
- README screenshots refreshed as features land. Decision needed on showing third party rigs.
- `.mb` files read (low priority).
- macOS build, if it's little trouble.
- Lou: `poseInterpolator`, blendShape on a lattice.
- Reference gaps: `exportEdits`, `-shd`, painted weights and tweaks on referenced meshes as edits; second reference of a large rig loads slower than the first.
- `conversionFactor` written normalised (0.0436 saves as 2.5), which Maya would read differently.

## Decisions for Simon
- Inno Setup says non-commercial use only: licence it or move to NSIS if Aztek is commercial.
