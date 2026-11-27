# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`, superseded ADRs in `docs/adr/README.md`.

**Milestone:** M1.3 Desktop mode, locomotion and IK (https://github.com/yannikwirbs/civbb/milestones).

## Next

#144 (test world and character controller). Its ADR decides Jolt (C bindings, MSVC and Linux builds) against an own
capsule sweep over static meshes; the player (`crates/client/src/player.rs`) now falls to a flat ground at y 0 and
sets `Grounded` from that, so the controller replaces that step. Then the queue in order (#145 IK, #146, #147, #148,
#149, #153, #70). M1.2 stays open only for #136's owner check (Tracy screenshot); close M1.2 once posted.

## Now

- **M1.3 started**: #141 desktop player (ADR 0106), #142 its platform values (ADR 0107), #143 built-in locomotion
  (ADR 0108: generated muscle-space clips grounded per avatar, proxy substitutes, default Base and Action layers
  merged at load by the viewer only), #152 default Gesture layer (ADR 0109). Emotes are #151 (M1.8).
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
  under red. Last full run: `corpus/local/unity-ref/scores/s140` (#140): every report byte-identical to s129.
- Session cost: ADRs 0091 and 0092 (2026-11-26). Measured over 9 sessions (`docs/iteration-baseline.md`): $5.5 and
  13.6 M cache-read tokens per closed issue, against $9.2 and 28.1 M before.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` (ADR 0045).

## Blockers and needs-user

- #136 (needs-user): connect a Tracy 0.14.1 profiler to `civbb-client` built with `--features profile` and post a
  screenshot (steps in the issue).
- Optional, from #140: this PC's power plan parks cores down to 4 % (`CPMINCORES`), so threads on idle cores wake
  late; the engine copes (ADR 0105), but timings depend on the plan.
- #126 (needs-user, Phase 4, not blocking): register a glTF vendor prefix (`CIVBB_` is provisional until the final name, D9).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-27 (18): #140 closed (ADR 0105): an own fork-join `jobs::Pool` replaces bevy_tasks for island and extract
  jobs (broadcast wake, 200 µs spin). Viewer 100 bloated: simulation 1.52 to 1.00 ms/frame. Parity s140
  byte-identical. Next: M1.3.
- 2026-11-27 (19): M1.3 issues filed; #141 (desktop player, ADR 0106) and #142 (platform values, ADR 0107) closed.
  Ended without the end-of-session steps; session 20 pushed its commits after a green gate.
- 2026-11-27 (20): #143 closed (ADR 0108: `civbb_anim::defaults`, generated locomotion, proxy substitutes, default
  Base and Action layers, grounded per avatar; viewer only, parity untouched) and #152 (ADR 0109: default Gesture
  layer per hand). All 13 corpus exports validate merged; KohzieBoi captured idle, walking, running, crouched,
  prone. Filed #151, #153. Next: #144.
