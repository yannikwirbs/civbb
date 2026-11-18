# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`.

## Now

- Phase 0 (prove the avatar compiler). Current milestone: **M0.10 Unity exporter** (M0.9 closed).
- Next: #75 with #73 and #55 (record VRChat's runtime in the SDK project), #74 (LOD border edge on Cokoro), Unity
  reference renders for parity (design doc §10), #52, #53, #54.
- Current area: the Unity exporter and the VRChat SDK runtime as ground truth. Read `STATE.md` (Unity exporter
  and ground truth) and `LESSONS-unity.md` first.
- Progress for the human: https://github.com/yannikwirbs/civbb/milestones

## Blockers and needs-user

- #76: CI is red since Rust 1.100 (new clippy lint in `atlas.rs`); fix first.
- #38 baselines later.
- Local data: the three Cokoro Blender GLBs sit in `corpus/local/hold/` until #74 is fixed (the local LOD test fails
  on them).

## Decisions in flight

None. ADRs 0017 to 0043 are accepted (0035 supersedes the codec, mip and gutter parts of 0029; 0036 the rig
bounds of 0033). LOD follow-up: #63 runtime level switching in renderer and governor. Renderer follow-ups: #59 GPU
culling, #60 MSAA and multiview, #61 pipeline cache and freeing scene memory, #62 clustered lights and shadows
(Phase 1). Dynamics constants and contact choices marked unverified wait
for Unity references (M0.10). In M0.10: #52 (body pose), #53 (muscle frames for glTF/VRM), #54 (Unity-recorded animator
goldens, now also material curve defaults, sRGB and `_ST` conventions per ADR 0025), #55 (box contacts, face proximity,
component node, body-part senders; SDK 3.10.6 has no receiver value field), #71 (`X_*` extensions in the importer), #73
(rebake offsets), #74 (LOD border edge), #75 (VRChat runtime ground truth from the SDK project). ADR 0040's capture frame
and per-axis freeze are unverified.

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-06 (3): #36 exporter run on the five Booth packages: `ImportPackage` is async (fixed), 13 prefabs export and match the Blender GLBs (table on #36). ADR 0041 `X_*` = format JSON; #71 importer part 1 (avatar, animator, constraints, dynamics, contacts, expressions, active). Exporter writes animators, clips, Unity constraints; all five build. Next: #37 (owner: VRChat SDK host project), meanwhile `X_toon_material`, #55, #73.
- 2026-11-06 (4): #37 closed. New avatars extracted (zips verified, deleted) and converted. Exporter on the SDK project: VRChat components, muscle frames, face (ADR 0042), `X_toon_material` (ADR 0043); exported clips and constraints match Unity exactly (`unity_export` test); constraint at-rest from the transform; hips left to the body pose. Opened #74, #75. Next: #75 (#73, #55).
- 2026-11-18: iteration cost tuning (`docs/iteration-baseline.md`): dev workspace crates at opt-level 1 (tests 82 to 14 s, gate 88 to 18 s), session state split by read frequency (ADR 0044; session start 70 to 17 KB), quiet `cargo xtask ci`, `cargo xtask test` (changed crates only), `tools/unity-batch.sh`. Next: #75 (#73, #55) as before.
