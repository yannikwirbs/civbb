# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`, superseded ADRs in `docs/adr/README.md`.

**Milestone:** M1.2 ECS client runtime (https://github.com/yannikwirbs/civbb/milestones).

## Next

#134: packages as shared handles loaded on the I/O pool; avatars spawn and despawn at runtime. Where things are
after #133: one avatar per runtime: `Runtime::extract_to_snapshot(Arc<Document>)` puts an `extract::Avatar`
resource in the world, and `Snapshot.islands[i]` maps to `Gpu.instances[i]`/`own[i]`/`maps[i]` by query order
(= spawn order while nothing despawns). #134 must give each island its package handle (the extract system reads
its `Document` per island) and key GPU instances by entity or handle, not by index; the render thread
(`render_thread.rs`) owns `Gpu`, so uploads of a newly loaded package must reach it through its channel. Despawn
breaks the query-order assumption in `islands_system`/`contacts_system` too (contact order matters for results).
Then #135, #136, #137 (extract per island on the pool; ~0.4 ms of the 1.6 ms simulation thread at 100 bloated).

## Now

- **Phase 0's gate is passed** (ADR 0084; table of all 13 Unity exports there, every red state after rest
  itemised with a Phase 1 issue). Phase 1 is split into milestones M1.1 to M1.9: M1.1 Native content model (#100)
  first, then ECS client runtime, desktop mode/locomotion/IK, OpenXR, protocol/instance server/nodes, voice,
  minimal hub and join flow, avatar fidelity (#109, #107, #110, #92, #93, #94, #77), Phase 1 gate (#61, #62, #63,
  #104). Milestones without issues get them filed from their design doc sections by the session that starts them.
- **M1.2 started** (ADR 0103): #130 ADR, #131 bevy_ecs world + `Frame` schedule (`civbb_core::schedule`,
  `civbb_client::frame::Runtime`, islands as entities), #132 bevy_tasks pool (rayon gone; bench within 10 %, see
  STATE.md), #133 extract snapshot + render thread (viewer pipelined). Open: #134 to #137. M1.1 closed 2026-11-27 (#100; #126 moved to Phase 4).
- **M1.1 (#100) is done**: ADRs 0085 to 0090, design doc §9. Done: #113 to #121 (state table ADR 0095, chain
  outputs, platform values ADR 0096, sync encoding ADR 0098, typed curve targets, native curve units and material
  fields ADR 0099, native actions and `unwritten`, document role and VRM 1.0 bone names, toon features ADR 0100;
  #124 schemas and spec (ADR 0102); #125 per-extension migrations and frozen fixtures; format v36: packages in
  corpus/local need `civbb build` again). #128: build-stage timings and the texture cache
  (ADR 0097).
- Parity rerun after any change to materials, dynamics or the renderer: `tools/parity.sh --against <last run>
  <out dir>` (~1 min with a warm texture cache). The ADR 0084 table must not regress; Yumeka's rest worst view is 0.1197, 0.0003
  under red. Last full run: `corpus/local/unity-ref/scores/s133` (#133): every report byte-identical to s129.
- Session cost: ADRs 0091 and 0092 (2026-11-26). Measured over 9 sessions (`docs/iteration-baseline.md`): $5.5 and
  13.6 M cache-read tokens per closed issue, against $9.2 and 28.1 M before.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` (ADR 0045).

## Blockers and needs-user

- #126 (needs-user, Phase 4, not blocking): register a glTF vendor prefix (`CIVBB_` is provisional until the final name, D9).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-27 (12): #125 closed: `civbb_format::migrate` (steps per extension, newest = 1 + steps, typed errors),
  importer and schemas use it; frozen v1 fixtures + `host.glb`, test-only v1 -> v3 chain. #129 closed: contact
  `component` remapped by the dialect importer and prune. Parity s129 byte-identical to s123. Next: close #100/M1.1,
  file M1.2.
- 2026-11-27 (13): M1.1 closed (#100; #126 to Phase 4). M1.2 filed (#130 to #136). #130 ADR 0103 (client runtime on
  bevy_ecs), #131 ECS world + frame schedule, #132 bevy_tasks pool (rayon removed). Parity s131, s132 byte-identical
  to s129. Next: #133.
- 2026-11-27 (14): #133 closed: `Extract` step into a `Snapshot` resource (`civbb_client::extract`), render thread
  owning `Renderer` + `Gpu` (`render_thread.rs`), two snapshots circulating; the viewer's title and summary report
  simulation, schedule and render-thread time. 100 bloated: 2.13 ms serial to 1.57-1.89 ms. Parity s133
  byte-identical to s129. #137 filed. Next: #134.
