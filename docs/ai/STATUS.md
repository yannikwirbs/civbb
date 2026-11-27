# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`, superseded ADRs in `docs/adr/README.md`.

**Milestone:** M1.2 ECS client runtime (https://github.com/yannikwirbs/civbb/milestones).

## Next

#140: short parallel steps lose ~0.1 ms to pool threads waking late (extract step 0.12 ms wall for 0.40 ms CPU on
~14 workers). Sessions cannot run Tracy (#136 is needs-user): measure with temporary `Instant` sums printed every
600 frames in the release viewer (`view --avatar bloated --copies 100 --frames 600`, baseline from a worktree),
reverted before committing. With threads, bevy_tasks' scope allocates ~2 per task; one job per worker chunk would
fix that too (#140 comment). After #140 M1.2 is done except #136's owner check.

## Now

- **Phase 0's gate is passed** (ADR 0084; table of all 13 Unity exports there, every red state after rest
  itemised with a Phase 1 issue). Phase 1 is split into milestones M1.1 to M1.9: M1.1 Native content model (#100)
  first, then ECS client runtime, desktop mode/locomotion/IK, OpenXR, protocol/instance server/nodes, voice,
  minimal hub and join flow, avatar fidelity (#109, #107, #110, #92, #93, #94, #77), Phase 1 gate (#61, #62, #63,
  #104). Milestones without issues get them filed from their design doc sections by the session that starts them.
- **M1.2 started** (ADR 0103): #130 ADR, #131 bevy_ecs world + `Frame` schedule (`civbb_core::schedule`,
  `civbb_client::frame::Runtime`, islands as entities), #132 bevy_tasks pool (rayon gone; bench within 10 %, see
  STATE.md), #133 extract snapshot + render thread (viewer pipelined), #134 package handles, runtime spawn and
  despawn, GPU instances by island (ADR 0104), #135 allocation-free island steps, #136 Tracy zones behind `profile` (code done; the owner checks it with a
  Tracy 0.14.1 server), #137 extract per island on the pool, #139 allocation-free frames on one worker. Open: #140. #138 (freeing GPU ranges, VRAM budget) is in M1.7. M1.1 closed 2026-11-27 (#100; #126 moved to Phase 4).
- **M1.1 (#100) is done**: ADRs 0085 to 0090, design doc §9. Done: #113 to #121 (state table ADR 0095, chain
  outputs, platform values ADR 0096, sync encoding ADR 0098, typed curve targets, native curve units and material
  fields ADR 0099, native actions and `unwritten`, document role and VRM 1.0 bone names, toon features ADR 0100;
  #124 schemas and spec (ADR 0102); #125 per-extension migrations and frozen fixtures; format v36: packages in
  corpus/local need `civbb build` again). #128: build-stage timings and the texture cache
  (ADR 0097).
- Parity rerun after any change to materials, dynamics or the renderer: `tools/parity.sh --against <last run>
  <out dir>` (~1 min with a warm texture cache). The ADR 0084 table must not regress; Yumeka's rest worst view is 0.1197, 0.0003
  under red. Last full run: `corpus/local/unity-ref/scores/s139` (#139): every report byte-identical to s129.
- Session cost: ADRs 0091 and 0092 (2026-11-26). Measured over 9 sessions (`docs/iteration-baseline.md`): $5.5 and
  13.6 M cache-read tokens per closed issue, against $9.2 and 28.1 M before.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` (ADR 0045).

## Blockers and needs-user

- #136 (needs-user): connect a Tracy 0.14.1 profiler to `civbb-client` built with `--features profile` and post a
  screenshot (steps in the issue); also look at worker wake-up gaps for #140.
- #126 (needs-user, Phase 4, not blocking): register a glTF vendor prefix (`CIVBB_` is provisional until the final name, D9).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-27 (15): #134 closed (ADR 0104): `package::Package` handles loaded on the I/O pool, `Runtime::spawn_package`
  and `despawn`, render-thread `gpu.rs` keyed by island with per-package free lists and rebuild on overflow; the
  viewer shows several packages. Parity s134 byte-identical; 100 bloated 1.72 ms/frame. #138 filed. Next: #135.
- 2026-11-27 (16): #135 closed: the dynamics solver keeps its per-frame buffers in `State`; island steps allocate
  nothing after warm-up (`tests/alloc.rs`). Bloated 100, 1 worker: 7.21 to 6.96 ms/frame. Parity s135
  byte-identical. `issue.sh queue` fixed for dotted milestones. #139 filed. Next: #136.
- 2026-11-27 (17): #136 code done (`civbb_core::profile`, tracy-client 0.19 behind `profile`; zones per system,
  island job, render pass; `System::Extract` counter), the Tracy check left to the owner (needs-user: running the
  downloaded tracy-capture was denied). #137 closed (extract on the pool: 0.35 to 0.12 ms, simulation 1.69 to 1.52
  ms), #139 closed (frames allocation-free on one worker; contact pass 0.042 to 0.024 ms). Parity s139 byte-identical.
  #140 filed. Next: #140.
