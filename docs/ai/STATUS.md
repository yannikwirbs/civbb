# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`, superseded ADRs in `docs/adr/README.md`.

**Milestone:** M1.1 Native content model (https://github.com/yannikwirbs/civbb/milestones).

## Next

#128 first (owner's call, 2026-11-26: parity reruns take 25 min, about 93 % of it `civbb build`; s116 build times
per export are in the issue). Start by adding per-stage build timings; cache encoded textures only if they
dominate. Then #118, then #119 to #125 in number order (events are Phase 2, #127). #118 (ADR 0088 point 4): `Entry` needs an
optional range (int `[min, max]`; float `[min, max]` plus `bits`, default 8) and an encoding derived from it (bool 1
bit, ranged int ceil(log2(span + 1)), ranged float `bits`, triggers unsynced); decide what an unranged synced int or
float costs (ADR). `StateTable::synced_bits` (format/state.rs) is the only cost function; its users are
`validate::state` (cap `Caps::synced_bits`, 1,024), `pipeline/src/package.rs` (stats) and the VRM importer
(`vrm/expressions.rs`, adds 8 per entry). Lowering in `gltf_import/unity_state.rs::lower`: Int 0..255, Float -1..1
in 8 bits, so the corpus totals stay unchanged (check with `civbb build` stats on `corpus/local/unity-glb`). Platform
entries are never synced (ADR 0096).

## Now

- **Phase 0's gate is passed** (ADR 0084; table of all 13 Unity exports there, every red state after rest
  itemised with a Phase 1 issue). Phase 1 is split into milestones M1.1 to M1.9: M1.1 Native content model (#100)
  first, then ECS client runtime, desktop mode/locomotion/IK, OpenXR, protocol/instance server/nodes, voice,
  minimal hub and join flow, avatar fidelity (#109, #107, #110, #92, #93, #94, #77), Phase 1 gate (#61, #62, #63,
  #104). Milestones without issues get them filed from their design doc sections by the session that starts them.
- **M1.1 (#100) is designed**: ADRs 0085 to 0090, design doc §9. Done: #113 to #117 (state table ADR 0095, chain
  outputs, platform values ADR 0096; format v28: packages in corpus/local need `civbb build` again).
- Parity rerun after any change to materials, dynamics or the renderer: `tools/parity.sh --against <last run>
  <out dir>` in the background. The ADR 0084 table must not regress; Yumeka's rest worst view is 0.1197, 0.0003
  under red. Last full runs: `corpus/local/unity-ref/scores/s116` (#116) and `s117` (#117), both equal
  to s115 and the table in every state.
- Session cost: ADRs 0091 and 0092 (2026-11-26). After about five sessions, compare `python
  tools/session-stats.py --since 2026-11-26` with the baseline in `docs/iteration-baseline.md` ($9.2 and 28 M
  cache-read tokens per closed issue) and record the result there.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` (ADR 0045).

## Blockers and needs-user

- #126 (needs-user, not blocking): register a glTF vendor prefix (`CIVBB_` is provisional until the final name, D9).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-26: session cost. `tools/session-stats.py` baseline (docs/iteration-baseline.md), project settings drop
  unused tools (ADR 0091, first-call context 47 k to 22 k tokens), session shape and `/continue` (ADR 0092),
  nextest in the gate (ADR 0093, 19 to 12 s), LSP (ADR 0094), helpers `tools/issue.sh`, `ci.sh`, `parity.sh`,
  `context.py`. Next: #115.
- 2026-11-26 (2): #115 state table (ADR 0088 points 1 and 2, ADR 0095): `civbb_format::state`, every parameter
  reference an entry index, Unity dialect 1 lowered in `gltf_import/unity_state.rs`, `rest` keeps the two defaults,
  parity against Unity steps through the stored hand-off. Format v26. Next: #116.
- 2026-11-26 (3): #116 chain outputs (`Chain::outputs`, one entry per output, written by `Island::step`; format v27)
  and #117 platform values (`state::Platform`, `platform/` names, read-only to content, ADR 0096; format v28).
  Parity s116 and s117 equal to s115. #128 filed (parity
  is build-bound). Next: #128, then #118.
