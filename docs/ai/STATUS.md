# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`, superseded ADRs in `docs/adr/README.md`.

**Milestone:** M1.1 Native content model (https://github.com/yannikwirbs/civbb/milestones).

## Next

#125 (per-extension versions, `civbb_format::migrate`, frozen v1 fixtures per extension; events are Phase 2, #127).
#124 is closed: JSON shapes (ADR 0102, format v36), schemas `docs/spec/schema/` (`cargo xtask schema`; the importer's
`parse` in `gltf_import/native.rs` is where `version` is checked today, so the migration hook goes there or in
`civbb_format::migrate` called from it), prose `docs/spec/`. A v1 fixture per extension can start from the corpus's
`to_source_glb` output (lean); the schema test already extracts extension JSON from a GLB. #129 is small and in the
same importer area. The fuzz crate is outside the gate: `cargo check --manifest-path fuzz/Cargo.toml --bins`.

## Now

- **Phase 0's gate is passed** (ADR 0084; table of all 13 Unity exports there, every red state after rest
  itemised with a Phase 1 issue). Phase 1 is split into milestones M1.1 to M1.9: M1.1 Native content model (#100)
  first, then ECS client runtime, desktop mode/locomotion/IK, OpenXR, protocol/instance server/nodes, voice,
  minimal hub and join flow, avatar fidelity (#109, #107, #110, #92, #93, #94, #77), Phase 1 gate (#61, #62, #63,
  #104). Milestones without issues get them filed from their design doc sections by the session that starts them.
- **M1.1 (#100) is designed**: ADRs 0085 to 0090, design doc §9. Done: #113 to #121 (state table ADR 0095, chain
  outputs, platform values ADR 0096, sync encoding ADR 0098, typed curve targets, native curve units and material
  fields ADR 0099, native actions and `unwritten`, document role and VRM 1.0 bone names, toon features ADR 0100;
  #124 schemas and spec (ADR 0102); format v36: packages in corpus/local need `civbb build` again). #128: build-stage timings and the texture cache
  (ADR 0097).
- Parity rerun after any change to materials, dynamics or the renderer: `tools/parity.sh --against <last run>
  <out dir>` (~1 min with a warm texture cache). The ADR 0084 table must not regress; Yumeka's rest worst view is 0.1197, 0.0003
  under red. Last full run: `corpus/local/unity-ref/scores/s123` (#123): every state identical to s122.
- Session cost: ADRs 0091 and 0092 (2026-11-26). Measured over 9 sessions (`docs/iteration-baseline.md`): $5.5 and
  13.6 M cache-read tokens per closed issue, against $9.2 and 28.1 M before.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` (ADR 0045).

## Blockers and needs-user

- #129 (found): contact volumes' `component` node is never remapped by the dialect importer or pruning.
- #126 (needs-user, not blocking): register a glTF vendor prefix (`CIVBB_` is provisional until the final name, D9).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-26 (9): fuzz CI fixed (targets still used `Avatar`/`p.avatar`). #123 toon materials as base fields plus
  named features (ADRs 0090, 0100, format v35), `Material::order`; renderer records pinned first
  (`material_records.rs`) and byte-identical after, a black rim now packs power 0 (pixel-identical). Parity s123
  identical to s122 in every state. Next: #124.
- 2026-11-26 (10): #124 part 1: ADR 0101 (`CIVBB_*` layout), native importer `gltf_import/native.rs`, corpus writes
  `CIVBB_*` (dialect-1 writer removed), material-records golden re-blessed (lean-source now keeps its toon
  materials). JSON shape change parked red on local branch `wip/124-json-shapes`. Found #129. Next: #124 continued.
- 2026-11-27 (11): #124 closed: JSON shapes squashed onto main (dialect Debug dumps equal before/after, goldens
  re-blessed; recordings, golden cases and unity-truth recorders converted, Goldens/Constraints re-recorded
  identically), ADR 0102, format v36; schemars schemas + `tests/schema.rs` + `cargo xtask schema`; toon material
  accepted unknown fields (flatten), fixed; `docs/spec/`. Parity not rerun (serde-only change). Next: #125.
