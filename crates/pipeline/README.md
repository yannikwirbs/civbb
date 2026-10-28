# civbb-pipeline

The content compiler (design doc §9). Glue crate: nothing depends on it.

- `gltf_import`: glTF 2.0 (`.gltf`, `.glb`) to a `civbb_format::Avatar`. Untrusted input: counts are capped before
  allocation, accessors are read by our own bounded reader (`accessor.rs`), the result passes `validate`.

VRM extensions (#10) and the `civbb` CLI (#12) build on this.
