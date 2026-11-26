# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`, superseded ADRs in `docs/adr/README.md`.

**Milestone:** M1.1 Native content model (https://github.com/yannikwirbs/civbb/milestones).

## Next

#124 continued, then #125 (events are Phase 2, #127). Done: ADR 0101 and the native `CIVBB_*` importer
(`gltf_import/native.rs`); the corpus writes `CIVBB_*` (`to_source_glb`), round trip in `tests/native_import.rs`.
**Uncommitted work lives on the local branch `wip/124-json-shapes`** (red, not pushed): the ADR 0086 point 3 JSON
shapes (named fields instead of newtypes and tuples, snake_case, `type`-tagged enums, omitted `None`s,
`deny_unknown_fields`) and the dialect-1 rewrite `gltf_import/unity_shapes.rs`; 18 tests still fail. Its state, the
failing list and the verification plan (Debug dumps of the dialect fixtures before and after, then re-bless) are in
the #124 comment of 2026-11-26. Rebase it on main, finish, write ADR 0102 (JSON rewrite of dialect 1 instead of
copied types, superseding ADR 0085 point 2), bump FORMAT_VERSION to 36, then `schemars` + `cargo xtask schema` and
`docs/spec/`. The fuzz crate is outside the gate: `cargo check --manifest-path fuzz/Cargo.toml --bins`.

## Now

- **Phase 0's gate is passed** (ADR 0084; table of all 13 Unity exports there, every red state after rest
  itemised with a Phase 1 issue). Phase 1 is split into milestones M1.1 to M1.9: M1.1 Native content model (#100)
  first, then ECS client runtime, desktop mode/locomotion/IK, OpenXR, protocol/instance server/nodes, voice,
  minimal hub and join flow, avatar fidelity (#109, #107, #110, #92, #93, #94, #77), Phase 1 gate (#61, #62, #63,
  #104). Milestones without issues get them filed from their design doc sections by the session that starts them.
- **M1.1 (#100) is designed**: ADRs 0085 to 0090, design doc §9. Done: #113 to #121 (state table ADR 0095, chain
  outputs, platform values ADR 0096, sync encoding ADR 0098, typed curve targets, native curve units and material
  fields ADR 0099, native actions and `unwritten`, document role and VRM 1.0 bone names, toon features ADR 0100;
  format v35: packages in corpus/local need `civbb build` again). #128: build-stage timings and the texture cache
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

- 2026-11-26 (8): #122 document root and VRM 1.0 bone names (format v34): `Document` + `Role::Avatar(Avatar {humanoid,
  viewpoint, face, expressions, meta})`, `Package::with_document`/`document`; `HumanBone` thumbs metacarpal,
  proximal, distal, camelCase serde; Unity names via `civbb_anim::muscle::unity_bone`. Dialect goldens equal after
  the mapping; parity s122 identical to s121. Session cost measured: $5.5 per closed issue (was $9.2). Next: #123.
- 2026-11-26 (9): fuzz CI fixed (targets still used `Avatar`/`p.avatar`). #123 toon materials as base fields plus
  named features (ADRs 0090, 0100, format v35), `Material::order`; renderer records pinned first
  (`material_records.rs`) and byte-identical after, a black rim now packs power 0 (pixel-identical). Parity s123
  identical to s122 in every state. Next: #124.
- 2026-11-26 (10): #124 part 1: ADR 0101 (`CIVBB_*` layout), native importer `gltf_import/native.rs`, corpus writes
  `CIVBB_*` (dialect-1 writer removed), material-records golden re-blessed (lean-source now keeps its toon
  materials). JSON shape change parked red on local branch `wip/124-json-shapes`. Found #129. Next: #124 continued.
