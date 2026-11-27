# 0106: The desktop player: input resource, kinematic root, first- and third-person camera

Status: accepted
Date: 2026-11-27

## Context

M1.3 (#141) starts desktop mode (design doc §18: Windows flat desktop with mouse and keyboard): one person walks a
converted avatar around. The viewer so far shows copies in a grid from an orbit camera. The design doc does not say
how input reaches the frame, what moves the avatar, where the desktop camera sits or what speeds are used. Worlds,
world physics (Jolt, §4) and the default locomotion animation come later in M1.3 (#143, #144).

## Decision

1. **Input is a resource sampled at "poses in".** The window thread writes an `Input` resource (move axes, run,
   jump, mouse look in radians) before each frame; the player system runs in `Step::PosesIn` (design doc §5 step
   1) and consumes the look delta and the jump press. Gamepads (#148) and OpenXR (M1.4) write the same resource.
2. **The player drives the island's root, kinematically.** A `Player` resource holds the local island's entity,
   position, yaw, view pitch, vertical speed and camera mode. Each frame it moves on the ground plane y = 0, turns the
   island's `origin` by yaw about +Y (avatars face +Z) and leaves the animator alone; root motion in clips is not
   applied. Collision with world geometry replaces the plane in #144.
3. **Speeds are the legacy world defaults**: walk 2 m/s, run 4 m/s (forward and back only), strafe 2 m/s, jump
   speed 3 m/s, gravity 9.81 m/s², so locomotion blend trees tuned to them blend as their creators expect. Diagonal
   input is not faster than straight input.
4. **Keys**: WASD move, Shift held runs, Space jumps, mouse looks (yaw turns the avatar, pitch only the view, within
   ±89°), V cycles the camera through orbit, first person and third person, Escape releases the cursor and a click
   captures it again. The cursor is confined and hidden while captured; look uses raw mouse motion.
5. **Camera.** First person: the eye at the avatar's `viewpoint` placed by the island's origin (not following the
   animated head: no bobbing; IK, #145, moves the head to the view instead), looking along yaw and pitch. Third
   person: 1.5 viewpoint heights behind the viewpoint along the view direction, looking at it. Orbit keeps the viewer's
   orbit camera; movement keys still move the player there. The viewer starts in orbit so measured runs and parity
   keep their framing.
6. **The own head is hidden in first person.** The viewpoint sits inside the head's meshes, so the extract step
   gives every skinning matrix placed by the humanoid head or a node below it a zero linear part at the head's
   position: those vertices collapse to a point, are culled and cost nothing to shade. Simulation (dynamics,
   contacts) still sees the full head; only what this client draws changes. Mirrors and cameras (extra views) will
   need the head drawn and get their own snapshot flag then.
7. **Remote copies and the bench are unchanged**: only the island the player names moves.

## Consequences

- Built-in parameters from the player (`VelocityX`, `Grounded`, ...) are #142; until then avatars slide in their
  current pose.
- Speeds per world (world settings) arrive with worlds in Phase 2; the constants become the defaults then.
