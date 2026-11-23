# 0074: PhysBone output between fixed steps, and step timing within a frame

Status: accepted
Date: 2026-11-23

Supersedes ADR 0049's partial output step (the leftover time as a step with pull, spring and gravity scaled by the
fraction) and its interpolation of the animated pose for steps within a frame. Refines ADR 0068 (falloff's cosine).

## Context

At frame rates other than 60 Hz the SDK steps at fixed 1/60 s and shows something between steps. Our partial step
left the 90 Hz recordings 1 to 6 mm off (`collider_sway` 46 mm) while 30 Hz (whole steps) replayed exactly (#84).
Logging the SDK's per-root timing (`PhysBoneManager+ChainRoot`: `realTime`, `executions`), per-bone state
(`prevEndPoint`, `endPoint`, `prevVector`, `prevVelocity`) and the transforms each 90 Hz and 144 Hz frame, by
reflection (`PbState.cs`), and teacher-forcing one bone at a time against it showed:

1. `endPoint` is the shown end, `prevEndPoint` the last full step; full steps from the stored state are exact.
2. The shown direction is not a partial step: it is a linear blend of two directions, normalised. From the bone's
   last full-step end, measured from its (shown) parent in bone lengths and not normalised, towards this frame's full
   step (taken at the frame's pose, collided, not limited), by `t = r / (1 - pull (1 - r))`, `r` the leftover fraction
   of a step. Limits apply after the blend. Exact for pull 0.2 and 0.6 and `r` from 1/12 to 11/12 in every
   integration and version (angle fraction within 1e-4).
3. The step count per frame is the SDK's single-precision accumulation: `while time >= 1/60 { time -= 1/60 }`. At
   90 Hz every third frame ends at 0.99999976 of a step and takes none; our `+1e-6` tolerance stepped early (36 to 90
   frames off per recording).
4. Step `j` of a frame runs against the animated pose interpolated from the *last step's* pose (itself interpolated) to
   this frame's by `j / executions` (executions = accumulated time in steps), not between consecutive frames; colliders
   interpolate from the last *frame's* placement by the same fraction (the SDK's `LerpColliders`).
5. Falloff's cosine reads the bone's offset as last shown (the output between steps when that came later), not the
   last full step's. At 60 and 30 Hz the two coincide.

## Decision

`State::step` counts steps as in 3, interpolates as in 4 (keeping the last step's pose and the last frame's pose),
and outputs as in 2; `Particles::shown` carries 5. Forces in the output step are the full step's.

## Consequences

Every 90 Hz case replays within 0.2 mm (most within 0.06 mm), the 90 Hz bounds are gone except `collider_sway` (1.1 mm
at one frame where a bone's start touches the sphere, which float noise decides either way). 60 and 30 Hz are
unchanged. The state keeps one more animated pose per avatar (the last frame's). Why the blend factor has this form is
not known; it is fitted, and a SDK update could change it (rerecord `vrc_physbones_90hz.json`).
