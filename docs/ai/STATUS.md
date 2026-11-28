# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`, superseded ADRs in `docs/adr/README.md`.

**Milestone:** M1.4 OpenXR (https://github.com/yannikwirbs/civbb/milestones).

## Next

#160 (native locomotion page in the radial menu), then #162 (the `civbb-xr` crate). M1.4's queue is #160, #162 to
#174 (`tools/issue.sh queue`). #160: `civbb_ui::radial` works on a format `Menu` whose controls write avatar
entries; floor poses and flight live on `Player`, not in the island's parameters, so the client-owned page needs a
small design (an ADR): e.g. a synthetic page appended to the root with entries the menu system intercepts. Its VR
half waits for #170. #162: `Context::new` creates instance and device itself; XR_KHR_vulkan_enable2 has the runtime
create both, so the device create info (features 1.0 to 1.3 incl. `multiview`, which `mesh.spv` now needs) must be
built in one place both paths share. SteamVR is the active runtime here; whether a headset is attached is unknown
(ask in #174 before relying on it). The floor poses (ADR 0119) were checked by bone positions only, never looked at
in a window: worth a capture (`view --world`, X) when the viewer is next run.

## Now

- **M1.4 started** (2026-11-28): issues #162 to #174 filed; #60 MSAA and multiview (ADR 0120), #158 stick
  navigation of the radial menu.
- **M1.3 code done** (open for #157 only): #141 desktop player (ADR 0106), #142 its platform values (ADR 0107), #143 built-in locomotion
  (ADR 0108: generated muscle-space clips grounded per avatar, proxy substitutes, default Base and Action layers
  merged at load by the viewer only), #152 default Gesture layer (ADR 0109). Emotes are #151 (M1.8).
  #144 test world and character controller (ADR 0110: own capsule controller, Jolt deferred to Phase 2;
  `civbb-client view --world`).
  #145 full-body IK, 3-point (ADR 0111: `civbb_anim::ik`, goals in avatar space on `Island::targets`, desktop
  view pitch shared over chest, neck and head); #146 (ADR 0112: 6- and 10-point goals, FABRIK spine; ADR 0113:
  tracker devices by role, calibration, `Trackers` resource, `view --trackers 3|6|10`). #147 (ADR 0114: eye height
  scales the avatar, speeds and capsule; `Station` component, the test world's bench; world stations are #156).
  #148 (ADR 0115: gilrs pads merged into `Input`, snap or smooth turning, `view --turn`; owner check #157).
  #149 (ADRs 0116, 0117: `civbb-ui` crate, cosmic-text labels from system fonts, radial expression menu on Tab,
  overlay pass in `civbb-render`). #153 (no dev overflow checks for `basisu`), #154 (ADR 0118: capsule height by
  posture), #70 (ADR 0119: floor poses on `civbb/pose`, flight by `WorldRules`; follow-ups #159, #160, #161).
- **Phase 0's gate is passed** (ADR 0084; table of all 13 Unity exports there, every red state after rest
  itemised with a Phase 1 issue). Phase 1 is split into milestones M1.1 to M1.9: M1.1 Native content model (#100)
  first, then ECS client runtime, desktop mode/locomotion/IK, OpenXR, protocol/instance server/nodes, voice,
  minimal hub and join flow, avatar fidelity (#109, #107, #110, #92, #93, #94, #77), Phase 1 gate (#61, #62, #63,
  #104). Milestones without issues get them filed from their design doc sections by the session that starts them.
- **M1.2 started** (ADR 0103): #130 ADR, #131 bevy_ecs world + `Frame` schedule (`civbb_core::schedule`,
  `civbb_client::frame::Runtime`, islands as entities), #132 bevy_tasks pool (rayon gone; bench within 10 %, see
  STATE.md), #133 extract snapshot + render thread (viewer pipelined), #134 package handles, runtime spawn and
  despawn, GPU instances by island (ADR 0104), #135 allocation-free island steps, #136 Tracy zones behind `profile` (code done; the owner checks it with a
  Tracy 0.14.1 server), #137 extract per island on the pool, #139 allocation-free frames on one worker, #140 own fork-join pool (ADR 0105:
  simulation 1.52 to 1.00 ms/frame in the viewer). #138 (freeing GPU ranges, VRAM budget) is in M1.7. M1.1 closed 2026-11-27 (#100; #126 moved to Phase 4).
- **M1.1 (#100) is done**: ADRs 0085 to 0090, design doc §9. Done: #113 to #121 (state table ADR 0095, chain
  outputs, platform values ADR 0096, sync encoding ADR 0098, typed curve targets, native curve units and material
  fields ADR 0099, native actions and `unwritten`, document role and VRM 1.0 bone names, toon features ADR 0100;
  #124 schemas and spec (ADR 0102); #125 per-extension migrations and frozen fixtures; format v36: packages in
  corpus/local need `civbb build` again). #128: build-stage timings and the texture cache
  (ADR 0097).
- Parity rerun after any change to materials, dynamics or the renderer: `tools/parity.sh --against <last run>
  <out dir>` (~1 min with a warm texture cache). The ADR 0084 table must not regress; Yumeka's rest worst view is 0.1197, 0.0003
  under red. Last full run: `corpus/local/unity-ref/scores/s60` (#60): every report byte-identical to s140.
- Session cost: ADRs 0091 and 0092 (2026-11-26). Measured over 9 sessions (`docs/iteration-baseline.md`): $5.5 and
  13.6 M cache-read tokens per closed issue, against $9.2 and 28.1 M before.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` (ADR 0045).

## Blockers and needs-user

- #157 (needs-user): try a real gamepad in `view --world`, smooth and `--turn snap` (steps in the issue).
- #136 (needs-user): connect a Tracy 0.14.1 profiler to `civbb-client` built with `--features profile` and post a
  screenshot (steps in the issue).
- Optional, from #140: this PC's power plan parks cores down to 4 % (`CPMINCORES`), so threads on idle cores wake
  late; the engine copes (ADR 0105), but timings depend on the plan.
- #126 (needs-user, Phase 4, not blocking): register a glTF vendor prefix (`CIVBB_` is provisional until the final name, D9).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-28 (26): #149 closed (ADRs 0116, 0117: `civbb-ui` with `draw`, `text` (cosmic-text 0.19, system fonts,
  one RGBA8 glyph atlas) and `radial`; `civbb_render::overlay`; client `menu` system and `Overlay` resource; Tab or
  Start opens the menu). Filed #158 (stick navigation, M1.4). Next: #153.
- 2026-11-28 (27): #153 closed (basisu's XUASTC weight wraps by design; dev overflow checks off for it, all 13
  corpus packages decode in dev). #154 closed (ADR 0118: `Body::height` by posture 1.6/1.0/0.5, getting up only where
  `WorldMesh::fits`). #70 closed (ADR 0119: floor sitting and lying in the default Action layer on `civbb/pose`, X;
  flight along the view, F, where `WorldRules::flight`). Filed #159, #160, #161. Next: start M1.4.
- 2026-11-28 (28): M1.4 started, #162 to #174 filed. #60 closed (ADR 0120: MSAA by resolve, multiview by view mask
  and `SV_ViewID`; viewer `--msaa` default 4, parity 1, s60 byte-identical to s140). #158 closed (`Ring::stick`,
  `ExpressionMenu::stick`, `Pad::menu_button`). Next: #160.
