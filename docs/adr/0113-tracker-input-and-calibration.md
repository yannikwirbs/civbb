# 0113: Tracker input as a resource, and calibration by eye height and per-device offsets

Status: accepted
Date: 2026-11-27

## Context

#146 asks for calibration that stores per-user offsets (design doc §7) and a tracker input API that third-party
sources feed poses through (§17): OpenXR trackers in M1.4, OSC trackers in Phase 2, camera-based tracking. The IK
solver (ADRs 0111, 0112) takes goals in avatar space; devices report poses in the user's tracking space, worn
somewhere on the body at an unknown offset from the bone, on a user whose size differs from the avatar's.

## Decision

1. **Devices by role** (`civbb_anim::tracking`): `Role` names what a device is worn on (head, hands, hips, chest,
   feet, elbows, knees); `Devices` is a fixed array of an optional pose per role in tracking space, the user's metres
   with the floor at y 0, already mapped by the source so that the avatar's origin and forward are its own. Mapping
   the play space to the avatar (locomotion, the avatar following the headset) is the source's job, M1.4's for
   OpenXR.
2. **Calibration** is one user's for one avatar: the user stands in the avatar's rest pose (a T-pose for most
   avatars), the head device at the eyes. `ratio` = the avatar's eye height (`viewpoint`) over the head device's;
   each device's offset is its bone's rest placement in the frame of the device scaled by `ratio`. Each frame a goal
   is the scaled device carried with its offset; elbows and knees give the bend points. A role calibrated without a
   device that frame, or tracked but not calibrated, sets nothing.
3. **Input API: an ECS resource, not a trait.** The client's `Trackers` resource holds this frame's `Devices`, a
   `calibrate` request and the calibrations by package id. Sources are systems in `PosesIn` that write the devices;
   `trackers_system` runs after the player's, calibrates when asked and sets the player island's `targets`. No
   source registry or trait object: a new source is one more system.
4. **Debug source**: `Scripted` (`view --trackers 3|6|10`) wears devices on the player's avatar at rest, scaled to a
   user 1.1 times its size, calibrates on the first frame, then squats, sways, nods, circles the hands and lifts the
   feet.

## Consequences

- Calibration round-trips: devices worn with arbitrary mounts by a scaled user put every tracked bone of a scripted
  pose back within 1 mm (`crates/anim/tests/ik.rs`).
- Calibrations live in memory keyed by the process-local package id. Keeping them per user and avatar across
  sessions needs stable avatar ids and a settings store (#155).
- The eye-height ratio scales tracking space uniformly; separate arm-span scaling, if users want it, is a later
  option on the same struct.
