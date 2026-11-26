# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`, superseded ADRs in `docs/adr/README.md`.

**Milestone:** M1.1 Native content model (https://github.com/yannikwirbs/civbb/milestones).

## Next

#119 (ADR 0089 points 1, 2 and 4: typed curve targets by index, paths resolved by the Unity importer, prune/merge/
atlas remap through one function). A large refactor across `format`, `anim` (`node_paths` and path matching leave
it) and `pipeline`: take it alone in a fresh session. Then #120 to #125 in number order (events are Phase 2, #127).
Parity reruns are now ~1 min when no texture changed (`corpus/local/texture-cache`, ADR 0097); an empty cache
(first run, or after a toolchain bump: delete it) takes ~15 min. Last full run: `corpus/local/unity-ref/scores/s118`.

## Now

- **Phase 0's gate is passed** (ADR 0084; table of all 13 Unity exports there, every red state after rest
  itemised with a Phase 1 issue). Phase 1 is split into milestones M1.1 to M1.9: M1.1 Native content model (#100)
  first, then ECS client runtime, desktop mode/locomotion/IK, OpenXR, protocol/instance server/nodes, voice,
  minimal hub and join flow, avatar fidelity (#109, #107, #110, #92, #93, #94, #77), Phase 1 gate (#61, #62, #63,
  #104). Milestones without issues get them filed from their design doc sections by the session that starts them.
- **M1.1 (#100) is designed**: ADRs 0085 to 0090, design doc §9. Done: #113 to #118 (state table ADR 0095, chain
  outputs, platform values ADR 0096, sync encoding ADR 0098; format v29: packages in corpus/local need `civbb build`
  again). #128: build-stage timings and the texture cache (ADR 0097).
- Parity rerun after any change to materials, dynamics or the renderer: `tools/parity.sh --against <last run>
  <out dir>` (~1 min with a warm texture cache). The ADR 0084 table must not regress; Yumeka's rest worst view is 0.1197, 0.0003
  under red. Last full runs: `corpus/local/unity-ref/scores/s128a`/`s128b` (#128, cold and warm cache)
  and `s118` (#118), reports identical to s117 and the table in every state.
- Session cost: ADRs 0091 and 0092 (2026-11-26). After about five sessions, compare `python
  tools/session-stats.py --since 2026-11-26` with the baseline in `docs/iteration-baseline.md` ($9.2 and 28 M
  cache-read tokens per closed issue) and record the result there.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` (ADR 0045).

## Blockers and needs-user

- #126 (needs-user, not blocking): register a glTF vendor prefix (`CIVBB_` is provisional until the final name, D9).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-26 (2): #115 state table (ADR 0088 points 1 and 2, ADR 0095): `civbb_format::state`, every parameter
  reference an entry index, Unity dialect 1 lowered in `gltf_import/unity_state.rs`, `rest` keeps the two defaults,
  parity against Unity steps through the stored hand-off. Format v26. Next: #116.
- 2026-11-26 (3): #116 chain outputs (`Chain::outputs`, one entry per output, written by `Island::step`; format v27)
  and #117 platform values (`state::Platform`, `platform/` names, read-only to content, ADR 0096; format v28).
  Parity s116 and s117 equal to s115. #128 filed (parity
  is build-bound). Next: #128, then #118.
- 2026-11-26 (4): #128 per-stage build timings (`compress` is ~all of a build) and an opt-in texture cache (ADR
  0097; parity rerun 15 min cold, 1 min 12 s warm, byte-identical packages); #118 sync encoding per entry
  (`Entry::range`, `sync_bits`/`encode`/`decode`, ADR 0098, format v29; corpus totals unchanged). Next: #119.
