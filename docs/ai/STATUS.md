# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`.

## Now

- Phase 0 (prove the avatar compiler). Current milestone: **M0.10 Unity exporter** (M0.9 closed).
- Next: decide #36 (the exporter umbrella: its body still lists an NDMF/Modular Avatar/VRCFury pre-build and
  reference renders as part of an export; survey which local packages use those tools, then close #36 or split what
  remains into issues), then Phase 1 (#59 onwards). Blackbird's worst states (0.35) show the right wing set in a different arm
  pose (not yet investigated). The local branch `wip/52-body-pose` is merged into main (542bea6) and can be deleted.
- Current area: the Unity exporter and the VRChat SDK runtime as ground truth. Read `STATE.md` (Unity exporter
  and ground truth) and `LESSONS-unity.md` first.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` for this machine and CI (ADR 0045); bump it on purpose.
  `.githooks/pre-push` runs `cargo xtask ci` (enable per clone: `git config core.hooksPath .githooks`).
- Progress for the human: https://github.com/yannikwirbs/civbb/milestones

## Blockers and needs-user

- #38 baselines later.

## Decisions in flight

None. ADRs 0017 to 0074 are accepted (0035 supersedes the codec, mip and gutter parts of 0029; 0036 the rig
bounds of 0033; 0046 the capture frame and rebaking of 0040; 0047 refines 0017's shapes and proximity; 0049
supersedes 0015 and the solver points of 0016; 0055 the frame count of 0033; 0062 the HDR conversion and reference time of 0052; 0065 ADR 0010's WD-on empty states and 0028's `Start` state; 0066 the interruption points of 0008; 0067 the
material default of 0025; 0068 the version 1.0 gravity of 0049; 0070 the collider velocity of 0049; 0074 the partial output step and in-frame
interpolation of 0049; 0072 corrects
0064's thumb account). LOD follow-up: #63 runtime level switching in renderer and governor.
Renderer follow-ups: #59 GPU culling, #60 MSAA and multiview, #61 pipeline cache and freeing scene memory, #62
clustered lights and shadows (Phase 1). In M0.10 only #36 (umbrella) and #38 (needs-user) are open. #77 (contact advanced parameters) and #92 (hand and finger colliders as global PhysBone colliders)
in Phase 1, with #93 (an animated `[HDR]` colour's default) and #94 (material time).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-23: #54 continued: masked layer and interruption goldens recorded; unordered transitions restart
  themselves and interruptions snapshot the last output pose (ADR 0066). Next: #54 material property curves.
- 2026-11-23 (2): #54 closed (ADR 0067: material curves write the renderer's property block whole, defaults from
  material 0). #83 closed (ADR 0068: PhysBone 1.0 from the SDK's per-bone state read by reflection). #84 mostly (ADRs
  0069 to 0071: rest snap, collision friction, composed turns; 90 Hz output left). #95 closed (ADR 0072: Unity builds
  Avatars from `humanDescription.skeleton`; no thumb rule needed). #96 closed (ADR 0073: per-bone mass positions,
  hips within 1 mm). Next: #84's 90 Hz output, then #71.
- 2026-11-23 (3): #84 closed (ADR 0074): from the SDK's per-root timing and per-bone state at 90 and 144 Hz, the
  output between steps blends the last full step's end (raw, from the shown parent) towards the frame's collided step
  by `r / (1 - pull (1 - r))`, limits after; steps counted in f32 as the SDK accumulates; step `j` against the pose
  interpolated from the last step's by `j / executions`, colliders from the last frame's; falloff reads the offset as
  shown. Every 90 Hz case within 0.2 mm (was 1 to 46 mm). #71 closed (fuzz seed for `X_toon_material`, clean run).
  Next: decide #36, then Phase 1.
