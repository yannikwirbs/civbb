# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`.

## Now

- Phase 0 (prove the avatar compiler). Current milestone: **M0.10 Unity exporter** (M0.9 closed).
- Next: #52 (body pose: model in `civbb_anim::body`; next the exporter's `humanScale`, RootT/RootQ binding, hips placement
  and the default pose, plan on the issue), then #53, #54, #83, #84. Blackbird's worst states (0.35) show the right wing
  set in a different arm pose (not yet investigated).
- Current area: the Unity exporter and the VRChat SDK runtime as ground truth. Read `STATE.md` (Unity exporter
  and ground truth) and `LESSONS-unity.md` first.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` for this machine and CI (ADR 0045); bump it on purpose.
  `.githooks/pre-push` runs `cargo xtask ci` (enable per clone: `git config core.hooksPath .githooks`).
- Progress for the human: https://github.com/yannikwirbs/civbb/milestones

## Blockers and needs-user

- #38 baselines later.

## Decisions in flight

None. ADRs 0017 to 0062 are accepted (0035 supersedes the codec, mip and gutter parts of 0029; 0036 the rig
bounds of 0033; 0046 the capture frame and rebaking of 0040; 0047 refines 0017's shapes and proximity; 0049
supersedes 0015 and the solver points of 0016; 0055 the frame count of 0033; 0062 the HDR conversion and reference time of 0052). LOD follow-up: #63 runtime level switching in renderer and governor.
Renderer follow-ups: #59 GPU culling, #60 MSAA and multiview, #61 pipeline cache and freeing scene memory, #62
clustered lights and shadows (Phase 1). In M0.10: #52 (body pose), #53 (muscle frames for glTF/VRM), #54
(Unity-recorded animator goldens, also material curve defaults, sRGB and `_ST` conventions per ADR 0025, and a masked
playable layer), #71 (`X_*` extensions in the importer), #83 (PhysBone 1.0
transients), #84 (PhysBone residuals: non-60 Hz output, collider velocity, polar corners). #77 (contact advanced parameters) and #92 (hand and finger colliders as global PhysBone colliders)
in Phase 1, with #93 (animated `[HDR]` colours) and #94 (material time).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-20 (8): #87 closed (ADRs 0057, 0058, format v18, v19): lilToon's shadow ramp mapped exactly (it had lit most
  shadowed surfaces), shadow bands (strength, second shadow, main strength, border colour), rim ramp and masks,
  emission main strength times the full albedo, tone correction. Rest FLIP (front): Yumeka 0.53 to 0.14, Tessakuchan
  0.21 to 0.10, HAOLAN 0.17 to 0.14 (sides 0.22 to 0.11), Cokoro 0.20 to 0.17, Ostgaroa 0.24 to 0.22. #88 closed (ADR
  0059, v20): material-slot curves; worst states Blackbird 0.80 to 0.35, Blackbird Quest 0.83 to 0.16, inuinu 0.82 to
  0.12. #89 closed (ADR 0060, v21): mirroring recorded in Unity (eyes keep their side; state and child mirrors XOR).
  Opened #90 (inside capsule colliders), #91 (material time; pinning Unity's `_Time` made Ostgaroa worse, cause open).
  Next: #78.
- 2026-11-20 (9): #78 closed (ADR 0061): the exporter runs the SDK's `UpdateAutoColliders` and writes the 14
  standard colliders as `X_contacts` senders with VRChat's body-part tags (155 across the 13 re-exported GLBs; feet
  on toes, fingers on their last bone, as the SDK picks); global PhysBone colliders to #92. #90: four inside-capsule
  cases recorded; a bone fatter than the capsule gets negative room (tip on the far side), fixed; HAOLAN's capsule is
  0.3 m against 0.0126 m bones, so its flare has another cause (next step on the issue). Next: #90.
- 2026-11-21 (1): #90 closed (PhysBone bone radius scales with its node's world scale, recorded; HAOLAN 0.144 to 0.054
  front). #91 closed (ADR 0062: `[HDR]` colours as saved, references at shader time 0; Cokoro 0.169 to 0.109, Ostgaroa
  0.219 to 0.134); #93, #94 opened. #52 started: body pose recorded and modelled (`civbb_anim::body`). Next: #52.
