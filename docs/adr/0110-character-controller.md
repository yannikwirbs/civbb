# 0110: An own capsule character controller over static triangles until worlds need rigid bodies

Status: accepted
Date: 2026-11-27

## Context

#144 needs a world to walk in and a character controller (gravity, jump, slopes, steps, sliding along walls,
`Grounded` from ground contact) before worlds exist as content (Phase 2). The design doc names Jolt through C
bindings for world physics (§4) and steps it on the worker pool (§5 step 7). Jolt is C++: its Rust bindings
(`joltc-sys` and the like) build it with CMake and a C++ compiler on every platform. This machine has no CMake, the
CI's Linux and fuzz jobs would need it too, and the fuzz build already trips over native dependencies
(`LESSONS-parsers.md`). Today there is exactly one moving body, the local player, against static geometry; nothing
needs rigid bodies, joints or a broadphase over thousands of shapes yet.

## Decision

1. **An own kinematic controller now, Jolt when worlds need rigid bodies** (world content, Phase 2). The controller
   is `civbb_client::controller`: a `WorldMesh` of static triangles and a `Body` (position, vertical speed,
   grounded) stepped by a wanted horizontal velocity, a jump and `dt`. That is the shape of Jolt's
   `CharacterVirtual` (a kinematic capsule moved against the world, not a rigid body), so Jolt can take its place
   behind the same `Body::step` when it arrives; the ADR that brings Jolt supersedes this one.
2. **Shape and limits.** A vertical capsule of radius 0.25 m and height 1.6 m standing on the avatar's origin.
   Slopes up to 45° are walkable and steps up to 0.3 m are climbed, Unity's `CharacterController` defaults, so
   legacy worlds' stairs and ramps play as they were built. Crouching and prone do not shrink the capsule yet.
3. **Method: overlap resolution in substeps.** A move is cut into substeps of at most half the radius; after each,
   every triangle closer to the capsule's axis than the radius pushes it out. A walkable contact (normal at most
   45° from up) pushes straight up, so standing on a ramp does not slide and walking up it keeps the horizontal
   speed; any other contact pushes along its normal, which slides along walls and down steep slopes. When a
   grounded horizontal move is blocked, the move is tried again lifted by the step height and set down after; it
   is kept if it gets further and ends on walkable ground. A grounded body that walks off an edge is snapped down
   by up to the step height (stairs and ramps down), else it falls. `Grounded` is a walkable contact within 1 cm.
4. **Brute force over triangles.** Every triangle is tested on every iteration; the test world has a few hundred.
   A spatial grid comes with real worlds, or Jolt does.
5. **The test world is generated content**: `civbb_corpus::world()` (CC0, deterministic) builds a document of rigid
   meshes with PBR colours: ground, a walkable and a too-steep ramp, stairs up to a platform, walls. It uses the avatar role until Phase 2 adds a world role, and the viewer shows it as an island
   (`civbb-client view --world`) and builds the `WorldMesh` from its meshes. Without a world the player stands on
   a flat ground plane at y 0, as before.
6. **Where it runs.** The player system steps the controller in `Step::PosesIn` on the schedule's thread: one body
   costs microseconds. World physics as its own step (§5 step 7) arrives with Jolt.

## Consequences

- No native dependency, no CMake in CI; the controller is a few hundred lines with tests over the test world.
- Static triangles only: no moving platforms, no pushing objects, no collision between players. These need Jolt and
  are part of the world work in Phase 2.
- Fast moves are substepped, so cost grows with speed; at walking and falling speeds that is a few substeps.
