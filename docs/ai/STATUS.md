# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`, superseded ADRs in `docs/adr/README.md`.

**Milestone:** M1.1 Native content model (https://github.com/yannikwirbs/civbb/milestones).

## Next

#115, then #116 to #125 in number order (events are Phase 2, #127). For #115 read ADRs 0085 and 0088, the issue's
comment (planned layout, the two-defaults pitfall) and the STATE.md sections for the compiler and the runtime.
#115 moves data, not evaluation: before changing anything, settle the two defaults (`Controller::start` uses the
expression default, `bind.rs` the animator default for parameter slots) and keep both behaviours. The animator
goldens, contact recordings, `report-lean.json` and corpus builds must not change; re-bless
`tests/golden/unity-dialect-1*.json` and check its diff only moves parameters into `state`. Bump `FORMAT_VERSION`
(packages in corpus/local then need `civbb build` again).

## Now

- **Phase 0's gate is passed** (ADR 0084; table of all 13 Unity exports there, every red state after rest
  itemised with a Phase 1 issue). Phase 1 is split into milestones M1.1 to M1.9: M1.1 Native content model (#100)
  first, then ECS client runtime, desktop mode/locomotion/IK, OpenXR, protocol/instance server/nodes, voice,
  minimal hub and join flow, avatar fidelity (#109, #107, #110, #92, #93, #94, #77), Phase 1 gate (#61, #62, #63,
  #104). Milestones without issues get them filed from their design doc sections by the session that starts them.
- **M1.1 (#100) is designed**: ADRs 0085 to 0090, design doc §9. Done: #113, #114.
- Parity rerun after any change to materials, dynamics or the renderer: `tools/parity.sh --against <last run>
  <out dir>` in the background. The ADR 0084 table must not regress; Yumeka's rest worst view is 0.1197, 0.0003
  under red.
- Session cost: ADRs 0091 and 0092 (2026-11-26). After about five sessions, compare `python
  tools/session-stats.py --since 2026-11-26` with the baseline in `docs/iteration-baseline.md` ($9.2 and 28 M
  cache-read tokens per closed issue) and record the result there.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` (ADR 0045).

## Blockers and needs-user

- #126 (needs-user, not blocking): register a glTF vendor prefix (`CIVBB_` is provisional until the final name, D9).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-24 (5): M0.11 closed: Phase 0's gate passed (ADR 0084). #105 Toon Standard ramp and shadow; #106 Poiyomi
  matcap replace, emission, second matcap, UV1 slots (format v25); #108 PhysBone limit rotations `qz qy qx`
  (recorded), Yumeka's skirt pushed by its hands (#92). No rest state red (5 green, 8 yellow). Epic #39 split into
  M1.1 to M1.9. Next: M1.1, #100.
- 2026-11-24 (6): M1.1, #100 designed: survey (no protocol/WIT crates yet; state and bindings must change before
  M1.2, materials and schema can wait), ADRs 0085 to 0090, design doc §9, issues #113 to #127 (#126 needs-user,
  #127 Phase 2). #113: runtime packages of another format version refused. #114: Unity dialect 1 frozen (generated
  fixtures, native golden, `civbb_unity_dialect` check). Next: #115.
- 2026-11-26: session cost. `tools/session-stats.py` baseline (docs/iteration-baseline.md), project settings drop
  unused tools (ADR 0091, first-call context 47 k to 22 k tokens), session shape and `/continue` (ADR 0092),
  nextest in the gate (ADR 0093, 19 to 12 s), LSP (ADR 0094), helpers `tools/issue.sh`, `ci.sh`, `parity.sh`,
  `context.py`. Next: #115.
