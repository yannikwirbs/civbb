# civbb-pipeline

The content compiler (design doc §9). Glue crate: nothing depends on it.

- `gltf_import`: glTF 2.0 (`.gltf`, `.glb`) to a `civbb_format::Avatar`. Untrusted input: counts are capped before
  allocation, accessors are read by our own bounded reader (`accessor.rs`), the result passes `validate`.

- `gltf_import/vrm.rs`: VRM 0.x and 1.0 on top of the glTF import: humanoid, viewpoint, spring bones, MToon,
  expressions (ADR 0006 has the mapping and its heuristics).

- `prune.rs`, `atlas.rs`, `merge.rs`, `lod.rs`, `collapse.rs`, `texture.rs`: compile stages `civbb build` runs in
  that order: prune unused joints, nodes and shapes (ADR 0026), atlas textures so materials that differ only in their
  textures become one (ADR 0029), merge meshes that show together (ADR 0027), build a skin-aware LOD chain with
  meshoptimizer (ADR 0031), collapse toggle layers into one direct blend tree (ADR 0028), compress textures to KTX2
  with Basis Universal under per-platform caps (ADR 0029). Each keeps the avatar's behaviour; tests compare islands,
  or texels at every vertex UV, before and after. The encoders (`basisu_c_sys`, `meshopt`) are C++ built by `cc`;
  `basisu_c_sys` needs libclang for its bindings and only sees decoded RGBA.
- `package.rs`: an import to a runtime package (texture blobs plus the avatar as root), and avatar stats.
- `report.rs`: `compile` runs the stages in order and records the per-avatar report (ADR 0032): stats after each
  stage and the package size as JSON and Markdown; `civbb build --report r.json [--measure]` writes it, `--measure`
  adds times from `civbb-client measure` (a release build next to `civbb`). The lean avatar's counts are pinned by
  `tests/golden/report-lean.json` (rewrite with `CIVBB_BLESS=1`).
- `main.rs`: the `civbb` CLI: `keygen`, `build`, `validate`, `inspect` (`cargo run -p civbb-pipeline --bin civbb`).

Local conformance runs: put Khronos samples in `corpus/local/khronos/` and VRMs in `corpus/local/vrm/` (both gitignored);
`cargo test -p civbb-pipeline -- --nocapture` imports each and prints what it found.
