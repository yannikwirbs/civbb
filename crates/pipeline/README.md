# civbb-pipeline

The content compiler (design doc §9). Glue crate: nothing depends on it.

- `gltf_import`: glTF 2.0 (`.gltf`, `.glb`) to a `civbb_format::Avatar`. Untrusted input: counts are capped before
  allocation, accessors are read by our own bounded reader (`accessor.rs`), the result passes `validate`.

- `gltf_import/vrm.rs`: VRM 0.x and 1.0 on top of the glTF import: humanoid, viewpoint, spring bones, MToon,
  expressions (ADR 0006 has the mapping and its heuristics).

Local conformance runs: put Khronos samples in `corpus/local/khronos/` and VRMs in `corpus/local/vrm/` (both gitignored);
`cargo test -p civbb-pipeline -- --nocapture` imports each and prints what it found.
