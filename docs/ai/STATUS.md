# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`.

## Now

- Phase 0 (prove the avatar compiler). Current milestone: **M0.10 Unity exporter** (M0.9 closed).
- Next: #78 (standard colliders and automatic contact senders: plan on the issue), #90 (inside capsule colliders:
  HAOLAN's tunic flaps flare), #91 (material time and Ostgaroa's emissive panel), then #52, #53, #54, #83, #84.
  Blackbird's worst states (0.35) show the right wing set in a different arm pose (not yet investigated).
- Current area: the Unity exporter and the VRChat SDK runtime as ground truth. Read `STATE.md` (Unity exporter
  and ground truth) and `LESSONS-unity.md` first.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` for this machine and CI (ADR 0045); bump it on purpose.
  `.githooks/pre-push` runs `cargo xtask ci` (enable per clone: `git config core.hooksPath .githooks`).
- Progress for the human: https://github.com/yannikwirbs/civbb/milestones

## Blockers and needs-user

- #38 baselines later.

## Decisions in flight

None. ADRs 0017 to 0060 are accepted (0035 supersedes the codec, mip and gutter parts of 0029; 0036 the rig
bounds of 0033; 0046 the capture frame and rebaking of 0040; 0047 refines 0017's shapes and proximity; 0049
supersedes 0015 and the solver points of 0016; 0055 the frame count of 0033). LOD follow-up: #63 runtime level switching in renderer and governor.
Renderer follow-ups: #59 GPU culling, #60 MSAA and multiview, #61 pipeline cache and freeing scene memory, #62
clustered lights and shadows (Phase 1). In M0.10: #52 (body pose), #53 (muscle frames for glTF/VRM), #54
(Unity-recorded animator goldens, also material curve defaults, sRGB and `_ST` conventions per ADR 0025, and a masked
playable layer), #71 (`X_*` extensions in the importer), #78 (standard colliders and body-part senders), #83 (PhysBone 1.0
transients), #84 (PhysBone residuals: non-60 Hz output, collider velocity, polar corners), #90 (inside capsule colliders), #91
(material time). #77 (contact advanced parameters) moved to Phase 1: no export uses contacts.

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-19 (6): #86 closed (ADR 0055): muscles default to 0 (Unity's default human pose, not the bind pose),
  instances start built-ins standing (`Builtin::at_rest`: `Grounded` 1 ...), parity states settle 60 frames
  (Tessakuchan's v1.0 PhysBones lagged 100 degrees after 3); `Reference.Render` logs rest layers and muscles. #85
  closed: parameters start at the expression defaults (`Controller::start`), the hand-off follows. Rest FLIP (front):
  HAOLAN 0.47 to 0.17, HAOLAN Quest 0.46 to 0.09, KohzieBoi 0.28 to 0.14, Cokoro Quest 0.45 to 0.23, Cokoro 0.37 to
  0.20, Blackbird 0.33 to 0.11, Blackbird Quest 0.29 to 0.07, Tessakuchan 0.31 to 0.21; Yumeka 0.42 to 0.53 (now
  dressed as VRChat loads it; shading #87). #77 to Phase 1 (no contacts in the corpus); #78 probed
  (`UpdateAutoColliders` by reflection); opened #88. Next: #87.
- 2026-11-20 (7): toggled parity states against Unity (ADR 0056): Write Defaults off never holds humanoid muscles,
  exit times below 1 recur whatever the motion (both probed), and the reference runs parameter drivers and layer
  controls through the SDK's client hooks. Worst states: Cokoro 0.71 to 0.37, Cokoro Quest 0.74 to 0.35, HAOLAN 0.54
  to 0.23, HAOLAN Quest 0.56 to 0.18. Opened #89 (mirroring). Next: #87.
- 2026-11-20 (8): #87 closed (ADRs 0057, 0058, format v18, v19): lilToon's shadow ramp mapped exactly (it had lit most
  shadowed surfaces), shadow bands (strength, second shadow, main strength, border colour), rim ramp and masks,
  emission main strength times the full albedo, tone correction. Rest FLIP (front): Yumeka 0.53 to 0.14, Tessakuchan
  0.21 to 0.10, HAOLAN 0.17 to 0.14 (sides 0.22 to 0.11), Cokoro 0.20 to 0.17, Ostgaroa 0.24 to 0.22. #88 closed (ADR
  0059, v20): material-slot curves; worst states Blackbird 0.80 to 0.35, Blackbird Quest 0.83 to 0.16, inuinu 0.82 to
  0.12. #89 closed (ADR 0060, v21): mirroring recorded in Unity (eyes keep their side; state and child mirrors XOR).
  Opened #90 (inside capsule colliders), #91 (material time; pinning Unity's `_Time` made Ostgaroa worse, cause open).
  Next: #78.
