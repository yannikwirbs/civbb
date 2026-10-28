# 0016: Chain colliders and rotation limits

Status: accepted
Date: 2026-10-28

## Context

#19 adds PhysBone sphere, capsule and plane colliders and angle, hinge and polar limits to the solver of ADR 0015.
VRChat's documentation says what they are (a cone of `Max Angle` around an axis set by `Rotation`, a hinge in a
plane set by `Rotation`, a polar limit sweeping a hinge across yaw, colliders with an inside option, a per-bone
`Radius`) but not the frames, the order or the collision model.

## Decision

1. **Limit frame.** Per particle: the parent node's animated world rotation, swung by the shortest rotation that
   takes its Y onto the bone's rest direction (Unity bones point along Y), then turned by the limit's `rotation`
   (Euler degrees, ADR 0007 order). The frame's Y is the limit axis; with zero rotation it is the rest direction.
2. **Shapes.** Angle: the bone stays within `max_angle` of the axis. Hinge: the bone turns only about the frame's
   X, within `max_angle` either way. Polar: an elliptical cone, `max_pitch` (turning about X, travel along Z) and
   `max_yaw` (turning about Z, travel along X), with the bound between them following the ellipse in angle space.
   Angles are graded per particle and clamped to 0 to 180 degrees.
3. **Colliders.** Each particle is a sphere of its graded `radius`. Spheres and capsules (along local Y, `height`
   including the caps) push it out to their radius plus its own, or with `inside` keep it within their radius
   minus its own; planes (normal local Y) push it to the positive side, or with `inside` the negative one. Shapes
   follow their node's animated world pose and scale.
4. **Order per substep.** After pull and stiffness (ADR 0015 §3): set the bone to its rest length along its
   direction clamped by the limit; then up to 8 passes of (all the chain's colliders, then the length clamped to
   the squish and stretch range), stopping early when no collider moved it. Alternating the two slides a bone
   along a surface; a single pass can settle with the bone re-extended through it. Limits do not re-apply after
   collisions, so a collider wins over a limit.

## Consequences

- Hand-derived tests cover each limit shape and frame turn, a tilted cone, floor, ball and capsule contact angles,
  bone radius, and squish from an inside collider.
- Collision is discrete: a particle that moves further than a collider's thickness in one substep can pass
  through it (gravity 1 without pull moves a bone its whole length per 1/60 s step under ADR 0015's units).
  Revisit with parity measurements (M0.9) or if real avatars show it.
- Unverified against PhysBones: bones colliding as end spheres rather than capsules along the bone, the frame
  convention, polar's ellipse, collider-over-limit precedence, and plane `inside`. Global colliders and the
  allow-collision filter (other avatars' hands) come with contacts and the client.
