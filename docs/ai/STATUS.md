# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`, superseded ADRs in `docs/adr/README.md`.

**Milestone:** M1.1 Native content model (https://github.com/yannikwirbs/civbb/milestones).

## Next

#116, then #117 to #125 in number order (events are Phase 2, #127). #116: chain outputs (`_IsGrabbed`, `_IsPosed`,
`_Angle`, `_Stretch`, `_Squish`) are computed (`civbb_dynamics::solver::ChainParams`) but nothing writes them to
the animator yet, so wiring them is new behaviour: rerun parity, a corpus avatar may animate on `_Angle`.
`Chain::parameter` is still the name prefix. Give `Chain` one `Option<EntryIndex>` per output, resolve
`<prefix>_<Output>` in `gltf_import/unity_state.rs` (the lowering of #115, ADR 0095; `X_dynamics` keeps the prefix
string, so lower it as receivers are) and write the values in `Island::step` next to the receivers. Raise them back
in `civbb_corpus::gltf` for the round-trip test.

## Now

- **Phase 0's gate is passed** (ADR 0084; table of all 13 Unity exports there, every red state after rest
  itemised with a Phase 1 issue). Phase 1 is split into milestones M1.1 to M1.9: M1.1 Native content model (#100)
  first, then ECS client runtime, desktop mode/locomotion/IK, OpenXR, protocol/instance server/nodes, voice,
  minimal hub and join flow, avatar fidelity (#109, #107, #110, #92, #93, #94, #77), Phase 1 gate (#61, #62, #63,
  #104). Milestones without issues get them filed from their design doc sections by the session that starts them.
- **M1.1 (#100) is designed**: ADRs 0085 to 0090, design doc §9. Done: #113, #114, #115 (state table, ADR 0095,
  format v26: packages in corpus/local need `civbb build` again).
- Parity rerun after any change to materials, dynamics or the renderer: `tools/parity.sh --against <last run>
  <out dir>` in the background. The ADR 0084 table must not regress; Yumeka's rest worst view is 0.1197, 0.0003
  under red. Last full run: `corpus/local/unity-ref/scores/s115` (#115, equal to the table).
- Session cost: ADRs 0091 and 0092 (2026-11-26). After about five sessions, compare `python
  tools/session-stats.py --since 2026-11-26` with the baseline in `docs/iteration-baseline.md` ($9.2 and 28 M
  cache-read tokens per closed issue) and record the result there.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` (ADR 0045).

## Blockers and needs-user

- #126 (needs-user, not blocking): register a glTF vendor prefix (`CIVBB_` is provisional until the final name, D9).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-24 (6): M1.1, #100 designed: survey (no protocol/WIT crates yet; state and bindings must change before
  M1.2, materials and schema can wait), ADRs 0085 to 0090, design doc §9, issues #113 to #127 (#126 needs-user,
  #127 Phase 2). #113: runtime packages of another format version refused. #114: Unity dialect 1 frozen (generated
  fixtures, native golden, `civbb_unity_dialect` check). Next: #115.
- 2026-11-26: session cost. `tools/session-stats.py` baseline (docs/iteration-baseline.md), project settings drop
  unused tools (ADR 0091, first-call context 47 k to 22 k tokens), session shape and `/continue` (ADR 0092),
  nextest in the gate (ADR 0093, 19 to 12 s), LSP (ADR 0094), helpers `tools/issue.sh`, `ci.sh`, `parity.sh`,
  `context.py`. Next: #115.
- 2026-11-26 (2): #115 state table (ADR 0088 points 1 and 2, ADR 0095): `civbb_format::state`, every parameter
  reference an entry index, Unity dialect 1 lowered in `gltf_import/unity_state.rs`, `rest` keeps the two defaults,
  parity against Unity steps through the stored hand-off. Format v26. Next: #116.
