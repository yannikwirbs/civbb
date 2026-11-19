# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`.

## Now

- Phase 0 (prove the avatar compiler). Current milestone: **M0.10 Unity exporter** (M0.9 closed).
- Next: #79 (Unity reference renders for parity; install the avatars' shader packages in the host project first),
  then #75's last part (PhysBones under scripted motion), #77, #78, #52, #53, #54.
- Current area: the Unity exporter and the VRChat SDK runtime as ground truth. Read `STATE.md` (Unity exporter
  and ground truth) and `LESSONS-unity.md` first.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` for this machine and CI (ADR 0045); bump it on purpose.
  `.githooks/pre-push` runs `cargo xtask ci` (enable per clone: `git config core.hooksPath .githooks`).
- Progress for the human: https://github.com/yannikwirbs/civbb/milestones

## Blockers and needs-user

- #38 baselines later.

## Decisions in flight

None. ADRs 0017 to 0047 are accepted (0035 supersedes the codec, mip and gutter parts of 0029; 0036 the rig
bounds of 0033; 0046 the capture frame and rebaking of 0040; 0047 refines 0017's shapes and proximity). LOD
follow-up: #63 runtime level switching in renderer and governor. Renderer follow-ups: #59 GPU culling, #60 MSAA and
multiview, #61 pipeline cache and freeing scene memory, #62 clustered lights and shadows (Phase 1). PhysBone
constants marked unverified wait for #75's PhysBone recordings. In M0.10: #52 (body pose), #53 (muscle frames for
glTF/VRM), #54 (Unity-recorded animator goldens, also material curve defaults, sRGB and `_ST` conventions per ADR
0025), #71 (`X_*` extensions in the importer), #75 (PhysBones left), #77 (contact advanced parameters, material
writes), #78 (body-part senders), #79 (reference renders).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-18: iteration cost tuning (`docs/iteration-baseline.md`): dev workspace crates at opt-level 1 (tests 82 to 14 s, gate 88 to 18 s), session state split by read frequency (ADR 0044; session start 70 to 17 KB), quiet `cargo xtask ci`, `cargo xtask test` (changed crates only), `tools/unity-batch.sh`. Next: #75 (#73, #55) as before.
- 2026-11-19: #76 closed: Rust pinned to 1.100.0 by `rust-toolchain.toml` for this machine and CI (ADR 0045; fuzz keeps nightly via `RUSTUP_TOOLCHAIN`), the one new clippy finding (`atlas.rs`) allowed with a reason, unused `workspace.package.repository` dropped, `.githooks/pre-push` runs `cargo xtask ci`. Next: #75 (#73, #55).
- 2026-11-19 (2): the VRChat SDK's runtime as ground truth (#75). `VrcConstraints.cs` (73 cases) and `VrcContacts.cs` (39 cases) record it in the host project by reflection; goldens committed and replayed exactly. #73 closed (ADR 0046: freeze captures the animated placement and survives deactivation, rebake per island, the SDK's scale-offset power). #55 closed (ADR 0047, format v11: box contacts, face proximity, component node, Proximity from a sender's second frame). #74 closed (zero-area triangles, test fix; Cokoro back in `glb/`). Opened #77, #78, #79. Next: #79.
