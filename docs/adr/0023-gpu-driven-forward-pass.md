# 0023: GPU-driven forward pass: shared buffers with vertex pulling, bindless textures, runs of indirect draws

Status: accepted
Date: 2026-11-03

## Context

#26: design doc §6 "GPU-driven path" puts all meshes in shared vertex and index buffers, all material parameters
in one storage buffer indexed per draw, textures behind descriptor indexing, and one indirect multi-draw per
pipeline. "Materials and shaders" asks for PBR and Toon uber-shaders (Toon with feature flags) plus Unlit, with
every pipeline known and built at load. ADR 0021 left depth attachments and graph-owned transient images to this
issue, and ADR 0022 point 6 left buffers out of the graph until a second consumer needs ordering; the skinned
output now has one (the draws). The doc does not say how draws find their data, how buffers are allocated, how
draws are grouped and ordered, or which lighting the shaders start from.

## Decision

1. **Shared buffers, vertices pulled by address.** A `Scene` owns six buffers: rest vertices (80 bytes, skinning
   input), attributes (UV0 and UV1, 16 bytes), blendshape deltas, mesh-relative `u32` indices, skinned vertices
   (48 bytes) and materials. A mesh is uploaded once (`MeshData`); each `SkinnedMesh` instance takes its own
   range of the skinned buffer, so N copies of an avatar share everything but their skinned output. Pipelines have
   no vertex input state: the vertex shader reads skinned vertices and attributes by device address at the bases
   in its draw record, and the one index buffer is bound once per pass. Ranges are bump-allocated from capacities
   given up front (`Capacity::for_meshes`); freeing comes with avatar streaming.
2. **A draw finds its record through `firstInstance`.** Each indirect command has one instance whose
   `firstInstance` is the draw's index; the vertex shader reads `SV_VulkanInstanceID` and indexes the draw records
   (material, skinned base, attribute base). This needs `multiDrawIndirect` and `drawIndirectFirstInstance`, both
   universal on the targets, and keeps a draw's identity inside the command, which a later culling pass that
   compacts commands needs (`gl_DrawID` would change with compaction).
3. **Bindless textures.** One descriptor set with one array of 4096 combined image samplers, partially bound,
   update-after-bind and update-unused-while-pending, so textures load while frames are in flight. One sampler:
   trilinear, repeat, up to 16x anisotropy. Shaders index with `NonUniformResourceIndex`. The renderer takes RGBA8
   texels (decoding is the caller's; KTX2 and BC7 come with #31) and generates mips by blits on upload.
4. **One material buffer.** Each material is a 288-byte record: colours, a parameter vector, a toon vector and six
   texture slots (scale-offset, bindless index or none, UV set), read by address. `material::pack` maps all three
   models into it.
5. **Twelve pipelines, grouped into runs.** A pipeline differs by shader (PBR, Toon, Unlit), alpha blending
   (blended draws test but don't write depth) and back-face culling; alpha cutoff is a shader branch (`discard`
   as demote), and Toon's features (shade texture, rim, matcap, emission, normal map) are branches on the record,
   not variants. All twelve are created up front. `batch` sorts opaque draws by pipeline and blended draws by
   render queue, then back to front by the caller's distance, and merges consecutive draws of one pipeline into a
   run: one `vkCmdDrawIndexedIndirect` with a draw count per run. Opaque content costs one call per pipeline;
   blended content one per change of pipeline in sorted order, which keeps blending correct.
6. **Shading model to start from.** Forward, one directional light and a uniform ambient term in a per-frame view
   record; the light colour is what a white Lambert surface facing it shows (no 1/π). PBR: GGX distribution,
   height-correlated Smith visibility, Schlick Fresnel. Toon: MToon 1.0 shading (shift, then a linear step whose
   width is `2 - 2 * toony`), parametric rim, matcap in MToon's view frame, emission. Depth is reverse Z with an
   infinite far plane in `D32_SFLOAT` (cleared to 0, `GREATER_OR_EQUAL`); the projection flips Y, so glTF's
   counter-clockwise fronts stay counter-clockwise on screen. These are approximations: parity with Unity's
   lilToon and Poiyomi is measured with FLIP in M0.9.
7. **The CPU writes the indirect commands for now.** Phase 0 draws a handful of avatars; the compute culling pass
   (stereo frustum and hierarchical Z) comes later and writes the same command format. Per-frame data (view,
   draw records, commands) is written with `vkCmdUpdateBuffer` into single buffers, as in ADR 0022 point 4.
8. **Graph: depth, transient images, buffers.** Passes take a depth attachment (`Load::Clear` clears depth to
   the first component). `Graph::transient` asks for an image by format, size and usage; `Graph::record` takes it
   from a `Transients` pool that keeps images across frames, remembers each one's last use (the next frame
   transitions from it rather than from undefined, which also orders it after the previous frame), and destroys
   an image no frame took for `FRAMES_IN_FLIGHT` frames. No aliasing yet. Buffers are imported with the accesses
   earlier work made (`prior`); a read waits for the last write once per reading stage, a write waits for the last
   write and every read since. Barriers are whole-buffer `VkBufferMemoryBarrier2`s. `Graph::record` now takes the
   context and the pool and can fail (a transient allocation).
9. **The skinned buffer is a graph buffer.** This replaces ADR 0022 point 6: the skinning pass declares
   `Scene::skinned` as a compute write, the forward pass as a vertex read, and the graph orders them and the next
   frame's write after this frame's draws. `Skinning::record` keeps its internal barriers and no longer ends with
   one for its readers.

## Consequences

- One code path for any number of meshes, materials and textures; the draw call count is the run count.
- Capacities must be known before loading; a full scene is an error, not a reallocation.
- Not yet, each with an issue: toon outlines (#58), GPU culling (#59), MSAA and multiview stereo (#60), a pipeline
  cache and freeing scene ranges and textures (#61), clustered lights and shadows (#62).
- Device tests render quads through every shader, depth, a bindless texture and blending and compare pixels, and
  draw the bloated corpus avatar without validation errors; CI checks batching, material records, the projection
  and graph planning.
