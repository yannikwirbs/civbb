# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`.

## Now

- Phase 0 (prove the avatar compiler). Current milestone: **M0.10 Unity exporter** (M0.9 closed).
- Next: #82 (material mappings ranked by Unity parity; tables on #79, #81), then #77, #78, #52, #53, #54, #83, #84.
- Current area: the Unity exporter and the VRChat SDK runtime as ground truth. Read `STATE.md` (Unity exporter
  and ground truth) and `LESSONS-unity.md` first.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` for this machine and CI (ADR 0045); bump it on purpose.
  `.githooks/pre-push` runs `cargo xtask ci` (enable per clone: `git config core.hooksPath .githooks`).
- Progress for the human: https://github.com/yannikwirbs/civbb/milestones

## Blockers and needs-user

- #38 baselines later.

## Decisions in flight

None. ADRs 0017 to 0050 are accepted (0035 supersedes the codec, mip and gutter parts of 0029; 0036 the rig
bounds of 0033; 0046 the capture frame and rebaking of 0040; 0047 refines 0017's shapes and proximity; 0049
supersedes 0015 and the solver points of 0016). LOD follow-up: #63 runtime level switching in renderer and governor.
Renderer follow-ups: #59 GPU culling, #60 MSAA and multiview, #61 pipeline cache and freeing scene memory, #62
clustered lights and shadows (Phase 1). In M0.10: #52 (body pose), #53 (muscle frames for glTF/VRM), #54
(Unity-recorded animator goldens, also material curve defaults, sRGB and `_ST` conventions per ADR 0025, and a masked
playable layer), #71 (`X_*` extensions in the importer), #77 (contact advanced parameters, material writes), #78
(body-part senders), #82 (material mappings ranked by Unity parity), #83 (PhysBone 1.0 transients), #84 (PhysBone
residuals: non-60 Hz output, collider velocity, polar corners).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-19 (2): the VRChat SDK's runtime as ground truth (#75). `VrcConstraints.cs` (73 cases) and `VrcContacts.cs` (39 cases) record it in the host project by reflection; goldens committed and replayed exactly. #73 closed (ADR 0046: freeze captures the animated placement and survives deactivation, rebake per island, the SDK's scale-offset power). #55 closed (ADR 0047, format v11: box contacts, face proximity, component node, Proximity from a sender's second frame). #74 closed (zero-area triangles, test fix; Cokoro back in `glb/`). Opened #77, #78, #79. Next: #79.
- 2026-11-19 (3): #79 closed (ADR 0048): lilToon 2.4.1, Poiyomi 7.3.050 and UTS2 2.0.9 in the host project; `Civbb.Exporter.Reference.Render` renders a `civbb-client rig` hand-off in play mode, `civbb-client parity <dir> <pkg>` scores, `civbb build --unstaged`. All 13 exports red (Taro best 0.12 to 0.23); opened #80, #81, #82. #75: PhysBones recorded (`VrcPhysBones.cs`, 30 cases); our solver 0.04 to 0.57 m off, first model findings on #75. Next: #75 refit.
- 2026-11-19 (4): #75 closed (ADR 0049, format v12): PhysBones refit to the SDK runtime. Recorder builds cases in play mode, 81 cases at 60 Hz and 18 at 90/30 Hz; fixed 1/60 s steps plus a partial output step, rest offsets turned with the parent bone, v1.1 gravity bends the rest direction, simplified `0.99 spring` on the unprojected displacement, advanced stiffness as a lerp to the last offset, immobile carry, polar as a box, segment colliders; v1.1 within 0.5 mm in 55 of 62 cases, residuals #83 (v1.0) and #84. `Chain::version` exported; 13 exports re-exported and Unity references re-rendered (framing moved). #81 closed (ADR 0050, format v13): `Toon::light` Additive (MToon, VRM, VRChat Mobile, measured) or Clamped (lilToon, Poiyomi, UTS2): Inuinu 0.38 to 0.09, Yumeka 0.56 to 0.42. #80 closed: descriptor masks intersected into layer masks in the exporter (no parity change on the 13). Next: #82.
