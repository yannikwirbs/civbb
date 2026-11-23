# 0076: glTF input accepts KHR_mesh_quantization and refuses required Draco and meshopt compression

Status: accepted
Date: 2026-11-23

## Context

The glTF importer refused every file that requires an extension gltf-json does not know (#45), including
`KHR_mesh_quantization`, `KHR_draco_mesh_compression` and `EXT_meshopt_compression`. Quantization only widens the
component types of vertex attributes, morph targets and animation outputs; our bounded accessor reader already
converts and normalises all of them. Draco and meshopt need a decoder: Draco's is C++ (no maintained Rust
decoder), meshopt's a C++ library or a decoder we would write and fuzz. The hard rules require parsers of
untrusted input to be bounded, fuzzable Rust.

Who produces what: our Unity exporter writes plain accessors; VRM exporters (UniVRM, VRM Add-on for Blender) do
not compress; Blender's glTF exporter offers Draco, off by default; gltfpack and glTF-Transform write meshopt and
quantization. None of the 54 local corpus files uses any of the three.

## Decision

- Accept `KHR_mesh_quantization`: drop it from `extensionsRequired` before gltf-json validates. Dequantisation is
  the node transforms (kept as they are) plus glTF normalisation in `accessor.rs`; texture coordinates use the
  existing `KHR_texture_transform` scale and offset.
- Refuse files that require `KHR_draco_mesh_compression` or `EXT_meshopt_compression` with an error that asks for
  an export without mesh compression. Files that only use them carry uncompressed fallback data by both specs,
  and that is read.

## Consequences

- No native decoder joins the trust boundary. Compressed files are an upload-size optimisation; our own package
  format compresses after import anyway.
- The four Khronos `glTF-Quantized` samples import and rest within 0.1 % of their originals' bounds (positions
  within 3 µm on Avocado, normals within 0.35 degrees). FLIP against the originals: Duck green, Avocado and Lantern
  yellow because the quantized files' tangents were re-orthogonalised (the files differ, not the import).
- Revisit with a pure-Rust meshopt decoder if creators bring meshopt files (gltfpack output) in numbers.
