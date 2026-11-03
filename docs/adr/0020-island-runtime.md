# 0020: The avatar island runtime: `core` runs jobs and counts, `client` wires the systems

Status: accepted
Date: 2026-11-03

## Context

#22 asks for the per-avatar island runtime of design doc §5 step 5 (animator, constraints, full-body IK, bone
dynamics, one job per avatar, rayon across avatars per ADR 0003), the step 6 contact pass feeding receiver values
into next frame's animator parameters, and per-system timing counters (§5 "Scheduling rules": lightweight counters
in release). The design doc puts "ECS, job system, math, asset handles" in `core` and makes it an engine crate:
engine crates depend only on `core` and `format`, and `anim` and `dynamics` are engine crates. So `core` cannot call
the animator or the solver; `client` is the crate that "wires the systems together". The doc does not say how a
slot pose becomes node transforms, how often each system runs, what an IK placeholder does, or how time is counted.

## Decision

1. **Split.** `civbb-core` holds what every engine crate can share: `math` (the helpers that `dynamics::math` and
   `anim` duplicated, now one copy; Unity's quaternion normalisation and Euler order win where they differed),
   `jobs` (`islands`: one rayon job per island via `with_max_len(1)`, counters summed by fold and reduce; `pool`
   for a fixed worker count) and `counters` (`System`, `Counters`). The concrete island (`Model`, `Island`) and the
   frame (`frame::step`) live in `civbb-client`. `cargo xtask arch` needs no change.
2. **Island order,** every frame and every system at the frame's `dt` (no distance-based rates yet, §8):
   1. Node transforms and own active flags reset to rest; last frame's receiver values are written into their
      animator parameters (coerced to the parameter's type, ADR 0011); receivers whose parameter the animator lacks
      are ignored.
   2. Animator update, then `Instance::write_nodes`: position, rotation and scale slots overwrite their components,
      the human pose overwrites the bones it frames (ADR 0014), an active slot of 0.5 or more makes the node active.
      The 0.5 threshold is ours, **unverified** (parity, M0.9).
   3. Constraints, then IK, then dynamics on the local pose.
   4. World placements (`core::math::world_poses_into`) and activity including ancestors, for the contact pass and
      later the renderer.
3. **IK is a stub** that keeps its place and its counter; the solver is Phase 1 work (§7 "Full-body IK").
4. **Contact pass.** After every island job finishes, one `civbb_dynamics::contact::step` over all islands on the
   calling thread, timed as `Contacts`. Splitting it by grid cell across workers is a later change (ADR 0017).
5. **Counters.** Each system's time is measured with `Instant` around its call and summed per system over all
   islands, so island systems report CPU time; the caller measures wall time per frame. `World` is its own system
   so placement cost is not hidden in dynamics. Counters are always on: one clock read pair per system per avatar.

## Consequences

- `anim` and `dynamics` share one math module; their Unity-recorded and hand-derived tests pass unchanged.
- The benchmark (#23) can report per-system CPU time per avatar next to wall time per frame and its scaling over
  worker counts.
- Islands allocate a little per frame inside the systems (the constraint pass's world list, the solver's world
  poses, the human pose); per-thread arenas (§5) are a later optimisation if the benchmark shows them.
- When `bevy_ecs` arrives (ADR 0003), `Island::step` becomes a system body and `jobs::islands` the scheduler's job.
