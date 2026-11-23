# 0070: PhysBone collider pushes slow the velocity by the SDK's friction and skip contact at a bone's start

Status: accepted
Date: 2026-11-23

Supersedes ADR 0049 point 8's "a push removes the inward part of the simplified velocity".

## Context

`collider_sway` (a sphere on the moving root sweeping through a swinging chain) was 1.5 cm off, the velocity after a
push being the unfitted part (#84). The SDK's per-bone state, read by reflection as in ADR 0068, gives the stored
velocity on every pushed step, and its manager has a static `COLLISION_FRICTION` of 0.25. Regressing the stored
velocity against the unpushed one: the difference is exactly -0.25 times the whole velocity, normal and tangential
alike, on every pushed step but one. That one, where the SDK stored the unpushed velocity and did not move the bone,
is the only contact whose closest point on the bone was the bone's start (its parent's end).

## Decision

1. A collider push scales the step's velocity by `1 - 0.25` instead of removing its inward part (per push; chains
   with several colliders pushing in one step are unrecorded).
2. A contact whose closest point on the bone is the bone's start does not push the bone.

## Consequences

`collider_sway` replays within 0.05 mm at 60 and 30 Hz (was 1.5 cm). The velocity rule matters for simplified
integration only; advanced integration takes its velocity from positions and is unchanged. The fat inside capsules
(#90) and 90 Hz output (#84) keep their bounds.
