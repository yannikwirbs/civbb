# 0115: Gamepad input through gilrs, merged into the player's input, with snap or smooth turning

Status: accepted
Date: 2026-11-27

## Context

#148: design doc §18 lists mouse, keyboard and gamepad for flat Windows desktop, and snap and smooth turning among
the comfort options. The doc fixes no crate, button layout, dead zone, turn speeds or snap step. The player reads one
`Input` resource (ADR 0106) that the viewer fills from winit events; `Input::look` is a per-frame yaw and pitch.

## Decision

1. **gilrs 0.11** (default `wgi` backend on Windows: Windows.Gaming.Input; XInput pads and others) reads pads. It is
   the crate Bevy and most Rust games use. `gilrs::Gilrs::new()` failing leaves the viewer without pads.
2. **One merged pad.** `civbb_client::gamepad::Pad` keeps the sticks of all pads (any pad drives the player) and
   adds them on top of keyboard and mouse each frame (`Pad::apply`): movement is the sum, clamped to -1..1, run and
   look add. Disconnecting a pad or losing focus centres the sticks, as lost focus releases held keys.
3. **Layout (Xbox names)**: left stick moves, its click runs until the stick is centred again; right stick turns
   (x) and pitches the view (y, 90°/s at full deflection); A jumps, B crouches, Y lies prone, X uses a station
   (Space, C, Z, E); View/Back cycles the camera (V). Start is kept for the menu (#149). Sticks below 0.15 of
   full deflection (radial) count as centred.
4. **Turning is a setting** (`gamepad::Turning`): `Smooth(rad/s)`, default 120°/s at full deflection, or
   `Snap(rad)`, default 30°, fired when the stick passes 0.7 sideways and re-armed below 0.3. It applies to yaw
   only; the mouse is never snapped. The viewer takes it from `--turn smooth|snap`. VR thumbsticks reuse the type
   (M1.4). A snap is one frame's yaw, so the `AngularY` platform value spikes for that frame.
5. A seated player ignores yaw as before (ADR 0114); pitch still works.

## Consequences

- Tests feed gilrs `Button` and `Axis` values into `Pad` and through the `Input` resource into the player; a real
  controller check is the owner's (needs-user).
- On Linux gilrs links libudev: builds there need `libudev-dev` (both Linux CI jobs install it).
- No rebinding, per-pad settings, rumble or trigger use yet; settings persistence comes with the settings UI.
- Snap turning's comfort vignette and VR-side use belong to M1.4.
