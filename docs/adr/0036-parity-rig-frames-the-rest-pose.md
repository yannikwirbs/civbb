# 0036: The parity rig frames what the avatar shows at rest, in world space

Status: accepted
Date: 2026-11-05

## Context

#69: ADR 0033 frames the four parity views from "the avatar's bind-pose bounds": every mesh's stored positions,
without node transforms or skinning. Mesh positions are in their own space, so the bounds can miss what the GPU
draws: Seed-san's rest pose is wider than its stored positions and the front view cut off both hands, so FLIP never
saw them. This supersedes the bounds the rig uses in ADR 0033; everything else about the rig stays.

## Decision

`civbb_client::parity::rest_bounds` steps one island of the reference avatar for `FRAMES_PER_STATE` frames at rest
(as the rest state renders), places every vertex as the GPU does (shape weights, then skinning, or the mesh's node
for rigid meshes: `parity::placed`, which the pipeline's stage tests now share), and takes the bounds of the meshes
on active nodes. The rig centres on those bounds and keeps the 90 % fill.

## Consequences

- Every corpus/local avatar fits all four views at rest: the avatar stays at least 29 pixels from the image border
  in every view (Seed-san's front view 42). Views that show nothing are the flat single-sided Khronos samples seen
  edge-on or from behind (Triangle, SimpleSkin and the like), which no framing changes.
- Hidden meshes (props off at rest) no longer widen the framing; a state that shows a big prop may still leave the
  frame, as ADR 0033 already notes.
- The rig costs one island and three steps more per parity run; negligible next to the renders.
