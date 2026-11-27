# 0107: Platform values from the desktop player

Status: accepted
Date: 2026-11-27

## Context

Platform values (ADR 0096) keep VRChat's built-in meanings and units so legacy animators work unchanged (design doc
§7). Until now instances held their `at_rest` values (ADR 0055). #142 drives them from the desktop player (ADR 0106).
ADR 0096 left `AngularY`'s unit to M1.3. VRChat's public documentation gives meanings and ranges but not every unit
or the desktop values; no corpus avatar reads `AngularY` (Cokoro, HAOLAN and Yumeka declare it, no blend tree or
transition uses it).

## Decision

1. **The player system sets them**, each frame before the island jobs, on the player's island only, through
   `Island::set_platform` (an entry the animator lacks is skipped, the value converted to the declared type). Remote
   and other islands keep their values until network sync (M1.5) provides them.
2. **Velocity** is the player's displacement over the last step divided by its time, in avatar space in VRChat's
   convention: `VelocityX` positive to the avatar's right, `VelocityZ` positive forward, `VelocityY` positive up, in
   m/s; `VelocityMagnitude` the length of all three.
3. **`AngularY` is radians per second, positive turning right** (Unity's clockwise-from-above about +Y), from the
   mouse yaw of the step. Unverified against VRChat; no corpus content depends on it. If content shows degrees,
   convert at import (ADR 0096 point 2).
4. **Posture.** C toggles crouching, Z toggles prone (each toggle again stands up). `Upright` is 1 standing, 0.55
   crouching, 0.25 prone: inside the crouch and prone bands locomotion layers commonly use. Crouching halves the
   movement speeds, prone quarters them.
5. **Desktop tracking**: `TrackingType` 3 (head and hands, which VRChat documents for desktop users), `VRMode` 0.
6. **Gestures**: left Shift with F1 to F8 sets the left hand's gesture 0 to 7, right Shift the right hand's; a
   gesture holds until changed. `GestureLeftWeight`/`GestureRightWeight` are 1 for a fist (gesture 1), else 0:
   a key closes the fist all the way.
7. **`AFK`** is 1 while the window is unfocused.
8. `Grounded` is the player's ground contact. `Seated` and `InStation` stay 0 until stations (#147); scale and eye
   height values with #147.

## Consequences

- Corpus avatars that bring their own locomotion layer (Cokoro, HAOLAN, Yumeka) animate as the player walks and runs;
  avatars on the SDK's default layers still need #143.
- Gamepads (#148) and VR controllers (M1.4) set gestures and posture through the same `Input` fields.
