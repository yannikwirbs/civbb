# 0125: The VR player: sticks through the gamepad's logic, room scale by recentring the play space, posture from the head's height

Status: accepted
Date: 2026-11-28

## Context

#165: design doc §18 (comfort: snap and smooth turning) and §7 (IK from the headset) for the player in `view --xr`.
ADR 0124 maps the play space to tracking space by a fixed half turn about +Y with the play space's origin at the
avatar's, and left the stage (the play space in the world) to this issue. Legacy animators read `VRMode`,
`TrackingType` and `Upright`. Nothing decided how room-scale walking, a body that does not turn with the head, or
stations combine with the desktop player's capsule (ADRs 0106, 0110, 0114).

## Decision

1. **The play space moves relative to the avatar** (supersedes point 3 of 0124): `Vr::play` is tracking space from
   the play space, a pose in the user's metres that starts as 0124's half turn. Devices are `play` applied to the
   play-space poses; the stage is the avatar's origin carrying `play` scaled by the calibration's ratio (1 before
   one) times the player's scale, so the eyes see from where the IK puts the head. `Camera::stage` hands it to the
   render thread with the frame's snapshot; without a VR player the stage stays below the orbit camera (0123).
2. **Room scale recentres the head every frame**: the head's horizontal offset from the avatar's origin, in tracking
   space, becomes `Input::step` (in world metres) and is taken off `play`. The capsule moves by it through the world
   like walking (`Body::step`, platform velocities included), so walls and steps act on it; when a wall stops the
   capsule the play space has shifted anyway, so the view stays on the capsule and never passes the wall. The head
   device then sits above the origin; the calibration's head offset absorbs the viewpoint's forward offset.
3. **The body turns after the head** once the head is more than 45° off its facing, by the excess; `play` turns back
   by the same angle, so the room stays put. Seated, floor-posed, or sitting down or posing this frame, neither
   recentring nor following happens: the head moves freely in a play space fixed to the seat (stations keep
   working; "the play space follows the seat").
4. **Sticks through the gamepad's `Pad`**: the left stick, turned from the steering direction (the head's yaw, or the
   left controller's aim with `--steer hand`) into the body's forward and right, and the right stick's x feed a
   `Pad` in the `Vr` resource, so dead zones, `--turn smooth|snap` (30° steps, re-armed below 0.3) and
   click-to-run match ADR 0115; the right stick does not pitch. Turns pivot on the head, which room scale keeps
   above the origin. Speeds are the desktop's. New actions in the one action set: a `stick` per hand (thumbstick,
   or the Vive's trackpad), `run` (left stick click), `jump` (A on the right hand; the right menu button on the
   Vive wand). The simple controller binds none of them.
5. **Posture from the head's height**: once calibrated, `Input::height` = head height × ratio / the avatar's eye
   height, clamped to 0..1, is `Upright` and picks the posture by the default locomotion layer's bands (below 0.7
   crouching, below 0.4 prone; `Posture::at`), still only getting up where the capsule fits (0118). Before
   calibration the player stands.
6. **Platform values**: `Player::vr` holds the `TrackingType` (`tracking_type`: 6 with calibrated hips and feet, 4
   with hips, else 3), `VRMode` is 1 while it is set; `IsLocal` was already the island's. VR is always first person
   with the own head hidden.

## Consequences

- `crates/client/tests/vr.rs` drives all of it on the frame schedule with scripted `HeadsetPoses`; bindings,
  comfort and the feel of recentring are checked only in a headset (#174).
- Leaning moves the avatar instead of bending it (the head is always recentred); a dead zone before the body
  follows the head is a later refinement if the headset check asks for one.
- The stage scales with the calibration ratio: the view's scale jumps once when calibrating.
- The first viewer frame has `dt` 0 and cannot move the capsule: that frame's room-scale offset is dropped.
