# 0063: The humanoid body pose, clips' root-motion settings, and hips at rest without one

Status: accepted
Date: 2026-11-21

## Context

Humanoid clips carry the body pose as `RootT` (centre of mass over `Animator.humanScale`) and `RootQ` (body
orientation). Our runtime left the hips at rest and skipped those curves (#52), so sitting, crouching and AFK clips
played standing. Recordings in `tools/unity-truth` (Unity 6000.3; the same clips behave alike in the SDK host project's
2022.3):

- `BodyPose.cs`: the centre of mass from Unity's mass table at segment midpoints matches within 5 mm, the orientation
  (upper arms' and legs' midpoints and left-to-right vectors, relative to the rest pose's) within 0.33 degrees; the hips
  placed from Unity's own body pose land within 5 mm and 0.33 degrees. Without an upper chest (9 of 13 corpus
  avatars) Unity keeps the chest's mass group: the chest's segment runs to the neck and the upper chest's mass sits
  three quarters of the way up it (within 7 mm; free placements fit no better than 3.5 mm). Missing toes drop out.
- `RootMotion.cs`: VRChat plays avatars with `applyRootMotion` off, and Unity extracts root motion from the body pose
  before discarding it, per clip `AnimationClipSettings`. 516 cases (every combination of the three bake, three
  keep-original, feet and loop-pose flags, with and without orientation and level offsets, a looping and a
  non-looping clip whose body moves and turns) fit one model to 2e-7: the pose is the body relative to a root `X`
  with rotation `(bake ? Y(start) : Y(now)) * (original ? Y(start)^-1 : 1) * Ry(offset)` (`Y` the body rotation's twist
  about +Y) and position, per axis group (height, horizontal), `(bake ? start : now) - (original ? start : 0)`. Loop
  pose then closes the cycle on that root-relative pose: at normalised time `w` it adds `w` times the end-to-start
  translation in the body's frame and turns by the end-to-start rotation with its vector part scaled by `w`. The level
  offset lowers the result. Feet-based height changed nothing (no foot goal curves); no corpus clip uses it.
- The corpus (57 humanoid clips with root curves): 26 fully baked based upon the original (taken as keyed), 14
  unbaked with only the original height (Cokoro's locomotion and AFK, Tessakuchan's idles, sits and AFK: their yaw and
  horizontal position are dropped), a few with level offsets or body-orientation-based baked rotation; loop pose on
  five, three of them zero-length. No clip sets a cycle offset, mirror, orientation offset or feet-based height.

## Decision

1. Format v22: `Humanoid::human_scale` (the exporter writes `Animator.humanScale`; 0 takes the rest pose's
   centre-of-mass height), `Humanoid::center_of_mass` and `Clip::root: Option<RootSettings>` (bake and keep-original flags, loop pose,
   orientation offset about our +Y, level); `None` takes the body curves as keyed. The exporter writes the settings of
   every humanoid clip.
2. Binding gives a clip's body curves 7 contiguous `Target::Body` slots (rotation blended as a quaternion);
   `body::RootMotion` applies the settings at sampling time, so layers blend root-relative poses as Unity does.
   The body slots follow the avatar mask's Root part (probed).
3. `body::BodyRig::place` puts the hips where the muscle pose's centre of mass and orientation meet the body pose,
   only when some layer wrote a body slot that update. The mass model is anchored to Unity's rest centre of mass
   (`Humanoid::center_of_mass`, exported with the root at the origin): the model's error at rest (2 to 8 mm on the
   corpus) becomes a constant offset in the hips' frame, leaving 2 to 4 mm on recorded corpus clips (#96). With no
   clip setting the body pose the hips stay at rest; Unity would put the body at position 0 and sink the avatar, but
   VRChat's locomotion always sets it. Body slots default
   to the rest pose's body pose, so a Write Defaults state reverts towards rest. `Reference.Render` resets the hips
   to rest unless body-curve clips carry the full weight of a layer that reaches the pose: where they blend with an
   emptied SDK clip Unity takes body position 0 from it and sinks the avatar, while our runtime blends towards rest.
4. Not modelled: a clip's cycle offset, feet-based height, mirrored body poses, additive body poses.

## Consequences

Sits, crouches and AFK poses take their height, and clips authored with root motion lose it as in VRChat. The hips'
2 to 4 mm error shows in parity at rest on small avatars (Cokoro 0.109 to 0.200, Tessakuchan 0.102 to 0.142), where
the old references held the hips at rest on both sides; #96 is the exact centre of mass. Revisit point 4 when a
corpus clip uses one of those settings.
