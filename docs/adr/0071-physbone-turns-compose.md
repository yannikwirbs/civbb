# 0071: A PhysBone bone's turn composes on its parent's

Status: accepted
Date: 2026-11-23

Refines ADR 0049 points 3 and 7 (the parent bone's rotation from its animated to its simulated direction).

## Context

ADR 0049 turned a bone's rest offset and limit frame by its parent bone's rotation from animated to simulated
direction, taken as the shortest world rotation between the two. `limit_polar_diagonal` stayed 7 mm off (#84). In
the SDK's recorded state, expressed in those frames, the second and third bones passed their polar longitude limit
(21 to 23.5 degrees against 20), while the first bone clamped exactly. Composing the frames down the chain as node
rotations compose (each bone's swing from its carried rest direction to its new one, applied on its parent's
rotation) puts every bone within the limits and on them exactly where clamped (0.0008 degrees). The two differ only in
twist about the bone, which straight chains show only through limit frames.

## Decision

A bone's turn is `from_to(rest offset turned by the parent's turn, new direction) * parent's turn` (the chain root's
children: the plain swing). It turns children's rest offsets and limit frames as before.

## Consequences

`limit_polar_diagonal` replays within 0.05 mm (was 7 mm); every 60 Hz case is now within 0.5 mm except the fat inside
capsules (#90) and spring 1 (f32 rounding, ADR 0069). Bent chains (children off their parent bone's axis) now carry
the parent's twist into their rest offsets; no recording covers a bent chain yet.
