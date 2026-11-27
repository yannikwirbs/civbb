# 0114: Avatar scaling by eye height, and stations as client components

Status: accepted
Date: 2026-11-27

## Context

#147 asks for avatar scaling with eye-height calibration and for stations (seats), desktop scope (design doc §7). The
format has the platform values `ScaleFactor`, `ScaleFactorInverse`, `EyeHeight`, `EyeHeightPercent`, `Seated` and
`InStation`, but no world format yet (Phase 2), so a station cannot come from content. The design doc fixes neither
the eye-height limits, nor what scales with the avatar, nor how a desktop player enters a station.

## Decision

1. **Eye height sets the scale.** The player chooses an eye height (`Player::eye_height`; `None` is the avatar's own,
   its `viewpoint` height). The avatar scales uniformly by eye height over its own (`Island::origin.scale`), so its
   viewpoint, first- and third-person camera, PhysBones and IK goals (avatar space, ADR 0113) scale with it. Chosen
   heights stay within 0.2 to 5 m (`EYE_MIN`, `EYE_MAX`); worlds may narrow this once they exist. Desktop keys: `=`
   and `-` multiply the eye height by 1.1 or divide it.
2. **What scales**: the scaled world is the same world seen by a bigger or smaller player, time unchanged. Walk, run
   and strafe speeds, the capsule's radius, height and step scale by the factor; the jump speed by its square root,
   so the jump's height scales by the factor under the world's gravity, which does not scale.
3. **Platform values**: `ScaleFactor` = the factor, `ScaleFactorInverse` = its inverse, `EyeHeight` = the eye height
   in metres, `EyeHeightPercent` = the eye height's place between 0.2 and 5 m, 0 to 1. The velocity values are in
   avatar metres (world velocity over the factor), so a scaled avatar's locomotion layer picks the gait it would at
   its own size.
4. **Calibration ratio is unchanged by scaling.** `Calibration::ratio` maps tracking metres to avatar metres, and
   avatar space is before `origin.scale`; a scaled avatar's goals scale with the avatar. The play space maps to the
   world at `ratio * scale` (M1.4's OpenXR source).
5. **Stations are client components** until worlds carry them: `Station { seat, yaw, exit, seated }` on an entity.
   The seat is where the avatar's origin goes, facing `yaw`; `seated` plays the default sit pose (`Seated`, ADR
   0108's Action layer); `InStation` is set either way. While in a station the player does not move or turn the body
   (the view still pitches); jumping or the use key leaves to `exit`. A station's own animation (a controller
   override) and station rules (immobilise, no exit) come with world content. Desktop: E uses the nearest station
   whose seat is within 1.5 m (scaled), or leaves.
6. **Test world**: a bench with one station (`civbb_corpus::world::STATION`), added by `view --world`.

## Consequences

- Speeds, jump height and the viewpoint follow the scale in tests; the capsule takes the scale too, so #154 (posture)
  scales its heights from the same field.
- A sit pose is grounded at the floor (ADR 0108), so the seat height fits avatars of the bench's size only; stations
  that adjust the seat to the avatar wait for world content.
