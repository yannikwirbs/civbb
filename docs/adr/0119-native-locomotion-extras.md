# 0119: Native locomotion extras: floor poses in the default Action layer, flight in the player, gated by world rules

Status: accepted
Date: 2026-11-28

## Context

Popular avatars carry locomotion prefabs for poses, sitting and lying anywhere, height fixes and flight (#70). The
owner decided (2026-11-05): no auto-replacing (the compiler neither detects nor strips such prefabs; an avatar's own
layers run unchanged) and clean room (no code, clips or controllers copied). The design doc (§7) lists stations and
scaling but none of these. Open were: who permits flight, how the native parts coexist with an avatar's own layers,
and how the player asks for them. Worlds have no format role until Phase 2 and there is no instance server yet
(M1.5).

## Decision

1. **Poses are animation, in the default Action layer.** `civbb_anim::defaults` adds two generated clips, sitting
   on the floor and lying on the back, grounded per avatar like every default clip (ADR 0108 point 2: that is the
   height fix), and two states to the default Action layer it already adds when the Action group has no layers.
   They play on a content entry `civbb/pose` (int 0 none, 1 sitting, 2 lying; range 0..=2, synced by the owner so
   remote clients will see it) and raise the Action group like AFK and stations do.
2. **Coexistence by absence.** An avatar with Action layers of its own gets no default Action layer and so no
   `civbb/pose`; the player's pose request does nothing on it. Nothing native overrides the avatar's animator.
3. **Posing holds the body.** While posed the body neither walks nor turns (the view still pitches, as seated);
   moving, jumping or asking again ends the pose. The capsule keeps the posture's height (ADR 0118): a posed body
   does not move, so its shape matters only once it gets up.
4. **Flight is movement, in the player.** Flying turns gravity, steps and ground snapping off; the body moves along
   the view (yaw and pitch) at the walking or running speed, still sliding against the world's triangles. It works
   for every avatar; the animator sees `Grounded` off and plays its falling state.
5. **Worlds permit flight.** A `WorldRules` resource holds `flight`, off by default. The world format gets the
   rule in Phase 2 and the instance server rejects flying movement where the world forbids it (M1.5); until then the
   viewer's generated test world allows it. Poses need no permission.
6. **Input**: desktop keys X (cycle pose: none, sitting, lying) and F (flight on or off), gamepad D-pad down and
   up. A native page in the radial menu, for desktop and VR alike, comes with the VR menu (#158's milestone).

## Consequences

- Avatars without locomotion layers get poses and flight without paying layers or parameters in their package;
  `civbb/pose` exists only in the loaded document (ADR 0108 point 3), so packages, parity and reports do not move.
- Avatars that bring a locomotion prefab keep exactly what they had; their creators opt out by keeping it.
- `civbb/pose` takes 2 bits of the owner's synced state once avatars sync; legacy avatars near the 256-bit budget
  may need it excluded then.
- Flying shows the falling pose; a hovering pose comes with emotes (#151) if it is wanted.
