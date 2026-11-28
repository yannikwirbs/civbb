# 0124: Headset and controller poses into `Trackers`: actions, sampling time, the play space's mapping, calibration

Status: accepted
Date: 2026-11-28

## Context

#164: design doc §5 step 1 samples OpenXR poses for the predicted display time; §7's IK takes head and hand goals.
ADR 0113 made trackers an ECS resource filled by source systems in `PosesIn`, in a tracking space "mapped by the
source so that the avatar's origin and forward are its own", and left that mapping to M1.4. ADR 0123 has the
simulation thread own `civbb_xr::Session` and wait for each frame; the play space is drawn at `xr::stage(camera)`
until the VR player (#165) places it. `view --xr` has no window, so no keyboard.

## Decision

1. **One action set, attached in `Session::new`**: a grip and an aim pose per hand (subaction paths
   `/user/hand/left`, `/right`) and a `calibrate` button, with suggested bindings for the Khronos simple
   controller, Oculus Touch, HTC Vive wand and Valve Index. Calibrate is the left hand's menu button, B on the
   Index (its menu button is the system's). Buttons, sticks and curls for locomotion and gestures are added to the
   same set by #165 and #166 (action sets cannot change once attached).
2. **Sampled on the simulation thread for the frame's predicted display time**: right after `xrWaitFrame`,
   `Session::poses(time)` syncs the actions and locates the VIEW space (the head) and the four action spaces in the
   session's stage (or local) space, the space `Frames` draws in. A pose counts only with both orientation and
   position valid; otherwise that role is `None` and the IK keeps the animated pose for it. The client stores them
   in the `HeadsetPoses` resource before the frame steps.
3. **Play space to tracking space is a half turn about +Y** (`tracking::from_play_space`): the play space's origin
   is the avatar's origin and its forward (-Z) the avatar's (+Z). Positions stay in the user's metres (ADR 0113:
   the calibration's ratio scales them to the avatar, the island's origin carries goals with the player's position,
   yaw and scale). Until #165 the user, drawn from the stage at the orbit camera, sees the avatar ahead mirroring
   them; #165 puts the stage at the player (`stage = avatar origin ∘ this half turn`, plus its room-scale shift),
   after which the general mapping is the avatar origin's inverse composed with the stage, the same pose from one
   place.
4. **Head and grips are the `Head`, `LeftHand` and `RightHand` devices**; aims are kept for pointers (#170). The
   other roles stay `None` until body trackers (#168).
5. **Calibration on the button's press** (pressed since the last sync), never automatically: the user stands in
   the avatar's rest pose holding the controllers and presses it; ADR 0113's calibration does the rest. Until then
   the headset drives nothing. `view --xr` refuses `--trackers` (two sources for one player).

## Consequences

- `crates/client/tests/tracking.rs` feeds scripted play-space poses through `HeadsetPoses` into `Island::targets`;
  the bindings and the runtime's sync are checked only in a headset (#174).
- A runtime with only a LOCAL space (floor not at y 0) cannot calibrate: ADR 0113 needs the head above the floor.
  No current PC runtime lacks STAGE; a floor offset is the fix if one does.
- Late latching (#167) re-locates the eyes on the render thread; the IK still uses the poses sampled after the wait,
  one frame's prediction earlier.
