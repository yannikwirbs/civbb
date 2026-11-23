# 0077: Cull mesh instances on the CPU before skinning; the GPU culling pass waits for worlds

Status: accepted
Date: 2026-11-23

## Context

Design doc §6 has two culling statements: a compute pass culls instances against the stereo frustum and a
hierarchical-Z buffer of the previous frame and writes the indirect commands (GPU-driven path, step 3), and
"skinning runs in compute once per visible avatar per frame". ADR 0023 point 7 left both for later; #59 asked for
the GPU pass. Phase 1 draws avatars only: tens of skinned mesh instances, few occluders. 100 bloated avatars take
0.56 ms of GPU time; the client already builds every instance's skinning matrices on the CPU each frame.

A per-draw GPU pass runs after skinning, so it cannot save skinning; compacting its commands with atomics would
also reorder blended draws within a run, which ADR 0023 sorts back to front.

## Decision

- The client culls whole mesh instances on the CPU, before skinning (`civbb_client::cull`): each mesh keeps a
  bind-space sphere per skinning matrix over the vertices weighted to it (one over all vertices when unskinned),
  padded by the largest sum of a vertex's blendshape delta lengths times this frame's largest absolute shape
  weight (weights are unclamped). The instance's world box is the box around the spheres moved by this frame's
  skinning matrices, radius scaled by the matrix's longest column. A skinned vertex is a convex combination of
  points inside its joints' spheres (weights sum to 1), so it lies in that box. A box wholly outside one plane of
  the view frustum (four sides and the near plane; the far plane is infinite) is culled: no skinning job, no draws.
- The GPU culling pass with hierarchical Z moves to Phase 2 (#59), when world geometry brings thousands of
  objects and real occluders. Whether it compacts commands (`vkCmdDrawIndexedIndirectCount`) or writes
  `instanceCount` 0 is decided there.

## Consequences

- Every vertex of the 13 local exports, in every parity state (7.3 million checks), lies in its box; the
  generated avatars are checked in CI, with shape weights of -3 and scaled matrices.
- Cost when nothing can be culled: about 2.5 µs of CPU per Yumeka per frame (40 copies in view: 1.03 to 1.13 ms
  render-side CPU per frame; GPU and draw counts unchanged).
- Boxes are loose where shapes have large deltas (Cokoro's reach 6.8 m across): such meshes are culled late. A
  per-shape reach weighted by each shape's own weight would tighten them if it matters.
- Stereo (#60) culls against the union of both eyes' frusta.
