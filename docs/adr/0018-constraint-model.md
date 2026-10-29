# 0018: Constraints follow Unity's semantics, sorted by dependency at load (format v6)

Status: accepted
Date: 2026-10-29

## Context

#21 adds position, rotation, scale, parent, aim and look-at constraints (design doc §7: topologically sorted at
compile time, evaluated in one linear pass). Avatars come with Unity constraints or VRChat constraints; VRChat
documents its own as "a like-for-like replacement of Unity's constraints system" evaluated "from the base of the
chain to the tip" (creators.vrchat.com/common-components/constraints), and converts Unity ones on load. Unity's
manual names the settings but not the maths, so `tools/unity-truth/Assets/Editor/Constraints.cs` records 44 cases
from Unity 6000.3 (ADR 0013). Constraints only run in play mode, so the recorder enters play mode in batch mode
(domain and scene reload off), resets the inputs every frame as an animator would, and records the settled pose.

## Decision

1. **Where.** `civbb-anim::constraint`: `Constraints::new` orders an avatar's constraints once at load; `evaluate`
   applies them to the animated local pose in one pass, between the animator and IK (§5 step 5). The order is
   derived at load rather than trusted from the package, because packages are untrusted and the sort is cheap.
2. **Order.** A constraint runs after every constraint whose target is, or is an ancestor of, a node it reads (its
   sources, its up node, its own target). Constraints on the same target and independent ones keep list order; a
   cycle falls back to list order for what remains. Unity also settled the reversed chain cases in one frame.
3. **Recorded semantics** (all world-space inputs come from the pose as constrained so far):
   - Inactive or without sources: the node keeps its animated values. Source weights all zero: the at-rest values.
   - Position: the weight-averaged source position, in the parent's space, plus `offset` in local units.
   - Rotation: from the first source with weight, nlerp towards each later one by `weight / total` (not a plain
     weighted average); in the parent's space, then times `offset` (Euler) on the right.
   - Scale: the weighted geometric mean of the sources' lossy scales, over the parent's lossy scale, times `offset`
     per axis. Unity computes it with an approximate `pow`, landing 0.3 to 0.9% above the exact mean (even with
     one source at 3.0); we use the exact mean and test scale cases to 1%.
   - Parent: each source places a point at `offset.translation` rotated by the source but **not** scaled by it,
     and a rotation `source * offset.rotation`; points are averaged and rotations blended as above.
   - Aim: `aim_axis` turns onto the direction to the averaged source position and `up_axis` as near as possible to
     the world up: scene Y, a vector, the direction to a node (`ObjectUp`), or a node's rotation applied to a vector
     (`ObjectRotationUp`); with `None`, the shortest turn from the identity. Then `offset` as for rotation.
   - Look-at: aim with Z and Y, up being scene Y or the direction to `world_up_node`, then turned by `-roll`
     degrees about local Z (also with an up node), then `offset`.
   - `weight` blends from the at-rest values in local space (lerp; nlerp for rotations). Unconstrained axes then
     keep the animated value; for rotations per Euler component (ADR 0007 order) of the weighted result.
4. **Format v6.** `Parent` gains `rotation_axes` (`axes` are its position axes); `Aim` and `LookAt` gain `offset`;
   `WorldUp::ObjectUp` loses its vector, which Unity does not use. Validation checks up nodes and that constraint
   numbers are finite.

## Consequences

- `tests/constraint.rs` checks all recorded cases (weights over and under 1, a zero first weight, four sources,
  offsets, at rest, axes, every world-up type, odd aim axes, chains in both creation orders) plus hand-derived
  order, tie, cycle and inactive cases.
- Unverified choices: a zero direction or parallel up (aim keeps the animated rotation; parallel up falls back to
  the shortest turn), non-positive scales (clamped before the log), negative source weights (treated as 0) and
  non-uniform scale under rotation (lossy, no shear, like `lossyScale`).
- Not yet: animated constraint weights, active flags and source weights, and VRChat's Solve In Local Space and
  Freeze To World (#56). VRChat runs its own implementation; parity (M0.9) checks it against these Unity numbers.
- The math helpers duplicate a few of `civbb-dynamics`'; both move to `civbb-core` with #22.
