# 0049: PhysBone chains step as the VRChat SDK's runtime does, fitted to its recordings

Status: accepted
Date: 2026-11-19

## Context

ADR 0015 solved chains with a hand-derived XPBD model and ADR 0016 added limits and colliders; both were unverified.
#75 recorded the VRChat SDK 3.10.6 PhysBone runtime by reflection (`tools/unity-truth/Assets/Editor/VrcPhysBones.cs`,
ADRs 0046 and 0047 did the same for constraints and contacts): a four-bone chain on a moving root, 81 generated
cases at 60 Hz (both versions and integration types, one parameter at a time, sweeps of pull, spring, momentum,
stiffness, gravity and falloff, limits, colliders, immobile, scale) and 18 of them at 90 and 30 Hz. The 0015 model was
4 to 57 cm off. A throwaway numpy model fitted one force at a time (LESSONS-anim.md); the Rust solver follows it and
`crates/dynamics/tests/vrc_physbone.rs` replays every case. The SDK is not used or copied: only its recorded output.

## Decision

Supersedes ADR 0015 and the frame, polar shape, collider and order points (1 to 4) of ADR 0016. Unity space below
(gravity is -Y); `len` is the bone's animated world length.

1. **Time.** Fixed steps of 1/60 s. Frame time accumulates; each full step sees the animated pose interpolated to
   its time within the frame (position lerp, rotation nlerp per node). Leftover time is simulated as a partial step
   from the last full one for the output only (pull and spring or momentum scaled by the fraction) and not kept.
   Frames are clamped to 0.1 s. The SDK counts a chain's first frame twice (the replay passes `2 dt`).
2. **Version.** The format gains `Chain::version` (`Version_1_0`, `Version_1_1`, the SDK's names; format v12); the
   exporter writes it. VRM springs map to 1.0 (their gravity is a force).
3. **Rest offset.** A child's rest offset is its animated (or posed) offset from its parent, turned by the parent
   bone's rotation from its animated to its simulated direction (the root's children: unturned).
4. **Gravity.** Falloff first: `gravity *= 1 - falloff * max(0, cos)`, `cos` between the bone's last simulated
   offset and its animated one. Version 1.1 bends the rest direction: `normalize(lerp(dir, down, |g|))` towards
   down (up for negative gravity), so a bone settles at `tan = g / (1 - g)` whatever the pull. Version 1.0 adds
   `g * len` down per step after the pull (equilibrium `tan = g / pull`; transients not fitted, #83).
5. **Integration**, towards `target = parent + rest`:
   - simplified: `s = 0.99 spring`; `x = pos + s u + (1 - s) pull (target - pos)`, where `u` is the last step's
     displacement before the length constraint;
   - advanced: `x = pos + momentum (pos - prev)`, then `x += pull (target - x)`, then
     `x = lerp(x, parent + last offset, stiffness)` (the bone's offset from its parent after the last step).
   Pull 0 leaves a chain where it is, gravity included.
6. **Immobile.** Before a step, particles and their previous positions move by `immobile` of the frame's motion
   since the last step (rigidly for `AllMotion`, translation only for `World`); the last offsets do not.
7. **Limits.** After integration, before the length: the limit frame is the parent node's animated rotation, turned
   with the parent bone (point 3), then by the shortest rotation from Y to the child's local direction, then by the
   limit rotation. Angle: a cone. Hinge: about frame X. Polar: yaw (about Z, travel along X) as a latitude and
   pitch (about X, travel along Z) as a longitude, each clamped on its own (a box in angle space; the 0016 ellipse
   was wrong: the corners reach `acos(cos yaw cos pitch)`).
8. **Length, then colliders.** The bone gets its length; then each collider pushes the bone end by the bone's
   (parent-to-end segment's) depth at its closest point, along that point's normal, and the bone gets its length
   again. Inside shapes and planes test the deeper end. A push removes the inward part of the simplified velocity.
   Bones can sink into colliders and pass ones they hit head-on, as the SDK's do.

## Consequences

- At 60 Hz, version 1.1 replays within 0.5 mm in 55 of its 62 cases. Bounds pin the rest per case: version 1.0 (#83);
  partial-step output at 90 Hz (1 to 6 mm, 6 cm with a collider and spring), collider velocity (1.5 cm), polar
  corners (7 mm), falloff transients (1 mm), spring 1 (#84). 30 Hz is exact.
- Motion never stretches or squishes a bone; only grabs do (ADR 0015 point 5 kept, with posing and the animator
  values of its points 5 and 7). Grab, pose, multi-child roots and nested chains remain unverified against the SDK.
- A frame at a rate that is not a multiple of 60 Hz costs a partial step on top of its full steps.
- Replacing the 0015 model changes every avatar's secondary motion; the Unity reference renders (ADR 0048) are re-run
  to see PhysBone pose noise drop.
