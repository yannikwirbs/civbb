# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`.

## Now

- Phase 0 (prove the avatar compiler). Current milestone: **M0.10 Unity exporter** (M0.9 closed).
- Next: #54 (rest: material property curves, a masked playable layer, interruption sources, recorded with
  `Goldens.cs`), then #83, #84, then #95, #96. Blackbird's worst states (0.35) show the right wing set in a
  different arm pose (not yet investigated). The local branch `wip/52-body-pose` is merged into main (542bea6) and
  can be deleted.
- Current area: the Unity exporter and the VRChat SDK runtime as ground truth. Read `STATE.md` (Unity exporter
  and ground truth) and `LESSONS-unity.md` first.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` for this machine and CI (ADR 0045); bump it on purpose.
  `.githooks/pre-push` runs `cargo xtask ci` (enable per clone: `git config core.hooksPath .githooks`).
- Progress for the human: https://github.com/yannikwirbs/civbb/milestones

## Blockers and needs-user

- #38 baselines later.

## Decisions in flight

None. ADRs 0017 to 0065 are accepted (0035 supersedes the codec, mip and gutter parts of 0029; 0036 the rig
bounds of 0033; 0046 the capture frame and rebaking of 0040; 0047 refines 0017's shapes and proximity; 0049
supersedes 0015 and the solver points of 0016; 0055 the frame count of 0033; 0062 the HDR conversion and reference time of 0052; 0065 ADR 0010's WD-on empty states and 0028's `Start` state). LOD follow-up: #63 runtime level switching in renderer and governor.
Renderer follow-ups: #59 GPU culling, #60 MSAA and multiview, #61 pipeline cache and freeing scene memory, #62
clustered lights and shadows (Phase 1). In M0.10: #54 (animator goldens recorded; material curve defaults, sRGB
and `_ST` conventions per ADR 0025, a masked playable layer still to record), #95 (thumb and finger-spread muscle
frames), #96 (Unity's exact centre of mass), #71 (`X_*` extensions in the importer), #83 (PhysBone 1.0
transients), #84 (PhysBone residuals: non-60 Hz output, collider velocity, polar corners). #77 (contact advanced parameters) and #92 (hand and finger colliders as global PhysBone colliders)
in Phase 1, with #93 (animated `[HDR]` colours) and #94 (material time).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-21 (1): #90 closed (PhysBone bone radius scales with its node's world scale, recorded; HAOLAN 0.144 to 0.054
  front). #91 closed (ADR 0062: `[HDR]` colours as saved, references at shader time 0; Cokoro 0.169 to 0.109, Ostgaroa
  0.219 to 0.134); #93, #94 opened. #52 started: body pose recorded and modelled (`civbb_anim::body`). Next: #52.
- 2026-11-21 (2): #52 continued: the body pose follows the mask's Root part; baked clips place the hips as `BodyRig`
  does, unbaked ones drop their yaw; half the corpus clips are unbaked, so the runtime work stays on the local branch
  `wip/52-body-pose` until the exporter carries clip root-motion settings. Next: #52.
- 2026-11-21 (3): #52 closed (ADR 0063: clips' root-motion settings exported and applied as recorded, hips placed
  from the body pose and anchored to Unity's rest centre of mass, 2 to 4 mm off; #96 for exact). #53 closed (ADR
  0064: muscle frames derived for glTF/VRM; thumbs #95). #54 started (ADR 0065): goldens recorded in Unity; a
  motionless WD-on state writes nothing, the first update advances, exit-time transitions start at the exit point.
  Next: #54.
