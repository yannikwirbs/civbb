# 0022: Compute skinning with fixed-point sparse blendshapes, buffers by device address

Status: accepted
Date: 2026-11-03

## Context

#25: design doc §6 says skinning runs in compute once per visible avatar per frame into a buffer both eyes read,
blendshapes are sparse deltas, and only shapes with non-zero weight this frame are applied (legacy avatars carry
hundreds of shapes, a handful active; the bloated corpus avatar has 1,200). The format stores each shape's deltas
in strictly ascending vertex order with position and normal offsets, and four joint influences per vertex
(`format::mesh`). Several active shapes often move the same vertex, so a scatter of their deltas races. The doc
does not say how shaders reach buffers, how GPU memory is allocated, or how per-frame data gets to the GPU.

## Decision

1. **Two dispatches per mesh, batched across meshes.** `morph` runs one thread per delta of every active shape
   (a CPU-built list of shape, first delta and weight; each thread finds its shape by binary search) and adds
   `weight * delta` into a per-vertex accumulator. `skin` runs one thread per vertex: rest vertex plus accumulator,
   then linear blend skinning of position, normal and tangent (renormalised; tangent `w` kept). Normals use the
   skinning matrix itself, as Unity's skinning does, not its inverse transpose. `Skinning::record` takes every
   mesh of the frame and records four barriers in total, whatever the mesh count.
2. **Fixed-point integer atomics.** The accumulator holds six `i32` per vertex at 2^20 units per metre (about a
   micrometre, range ±2 km); deltas are rounded per thread and added with `InterlockedAdd`. Integer addition is
   associative, so the result is identical whatever order threads run in, and only core atomics are needed
   (float atomics are an extension Quest GPUs lack). Meshes with no active shape skip the clear and `morph`.
3. **Buffers by device address.** `bufferDeviceAddress` (Vulkan 1.2 core) is now required. Shaders receive
   buffer addresses in a 64-byte push constant block; there are no descriptor sets for compute. This is the
   direction bindless (#26) goes as well.
4. **Per-frame data in the command buffer.** Joint matrices and the active shape list are written with
   `vkCmdUpdateBuffer` (64 KiB chunks) into the mesh's own buffers, so frames in flight never share a host buffer
   and nothing needs a per-frame ring. Barriers order last frame's reads before the writes.
5. **Memory: `gpu-allocator`.** One allocation per buffer would hit `maxMemoryAllocationCount` (4096) with a few
   hundred avatars. Static mesh data uploads once through a staging buffer and a one-shot submit.
6. **No graph buffer resources yet.** Skinning records its own barriers inside a pass without attachments; the
   output is left readable by vertex input, vertex and compute shaders. Buffers become graph resources when a
   second consumer needs ordering (#26).
7. **The caller passes skinning matrices** (joint world matrix times inverse bind) and weights; computing them
   from the pose is the client's job (#27). A mesh without a skin is skinned to one joint, its node's matrix.

## Consequences

- One code path for any number of active shapes; cost scales with active deltas, not with shapes.
- A shape delta smaller than half a micrometre times its weight is lost; a sum beyond ±2 km overflows. Neither
  occurs on real avatars.
- The output buffer is single-buffered: the next frame's skinning waits for this frame's draws. Revisit if a
  profile shows the stall (double the output per frame in flight).
- The device test compares every mesh of the bloated corpus avatar with a dense CPU reference; CI only checks the
  active list.
