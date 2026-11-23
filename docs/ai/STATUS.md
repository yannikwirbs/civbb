# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`.

## Now

- Phase 0 (prove the avatar compiler). Current milestone: **M0.10 Unity exporter** (M0.9 closed).
- Next: #84's last item (PhysBone output between fixed steps at 90 Hz, 1 to 6 mm; findings on the issue), then #71
  (`X_*` extensions in the importer). Blackbird's worst states (0.35) show the right wing set in a different arm
  pose (not yet investigated). The local branch `wip/52-body-pose` is merged into main (542bea6) and can be deleted.
- Current area: the Unity exporter and the VRChat SDK runtime as ground truth. Read `STATE.md` (Unity exporter
  and ground truth) and `LESSONS-unity.md` first.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` for this machine and CI (ADR 0045); bump it on purpose.
  `.githooks/pre-push` runs `cargo xtask ci` (enable per clone: `git config core.hooksPath .githooks`).
- Progress for the human: https://github.com/yannikwirbs/civbb/milestones

## Blockers and needs-user

- #38 baselines later.

## Decisions in flight

None. ADRs 0017 to 0073 are accepted (0035 supersedes the codec, mip and gutter parts of 0029; 0036 the rig
bounds of 0033; 0046 the capture frame and rebaking of 0040; 0047 refines 0017's shapes and proximity; 0049
supersedes 0015 and the solver points of 0016; 0055 the frame count of 0033; 0062 the HDR conversion and reference time of 0052; 0065 ADR 0010's WD-on empty states and 0028's `Start` state; 0066 the interruption points of 0008; 0067 the
material default of 0025; 0068 the version 1.0 gravity of 0049; 0070 the collider velocity of 0049; 0072 corrects
0064's thumb account). LOD follow-up: #63 runtime level switching in renderer and governor.
Renderer follow-ups: #59 GPU culling, #60 MSAA and multiview, #61 pipeline cache and freeing scene memory, #62
clustered lights and shadows (Phase 1). In M0.10: #71 (`X_*` extensions in the importer), #84 (PhysBone output between
fixed steps at 90 Hz; the other residuals are fixed or documented). #77 (contact advanced parameters) and #92 (hand and finger colliders as global PhysBone colliders)
in Phase 1, with #93 (an animated `[HDR]` colour's default) and #94 (material time).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-21 (3): #52 closed (ADR 0063: clips' root-motion settings exported and applied as recorded, hips placed
  from the body pose and anchored to Unity's rest centre of mass, 2 to 4 mm off; #96 for exact). #53 closed (ADR
  0064: muscle frames derived for glTF/VRM; thumbs #95). #54 started (ADR 0065): goldens recorded in Unity; a
  motionless WD-on state writes nothing, the first update advances, exit-time transitions start at the exit point.
  Next: #54.
- 2026-11-23: #54 continued: masked layer and interruption goldens recorded; unordered transitions restart
  themselves and interruptions snapshot the last output pose (ADR 0066). Next: #54 material property curves.
- 2026-11-23 (2): #54 closed (ADR 0067: material curves write the renderer's property block whole, defaults from
  material 0). #83 closed (ADR 0068: PhysBone 1.0 from the SDK's per-bone state read by reflection). #84 mostly (ADRs
  0069 to 0071: rest snap, collision friction, composed turns; 90 Hz output left). #95 closed (ADR 0072: Unity builds
  Avatars from `humanDescription.skeleton`; no thumb rule needed). #96 closed (ADR 0073: per-bone mass positions,
  hips within 1 mm). Next: #84's 90 Hz output, then #71.
