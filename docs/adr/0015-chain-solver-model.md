# 0015: PhysBone chains are solved as XPBD particles against the animated pose

Status: accepted
Date: 2026-10-28

## Context

Design doc §7 asks for an XPBD chain solver that reproduces the documented PhysBone parameters (#18). VRChat's
documentation (creators.vrchat.com, "PhysBones") defines what each parameter is for but not the integration
math, and its SDK may not be used (design doc §20). Without a reference implementation, the model below is ours;
the parity check (M0.9) measures it against recordings of real avatars and tunes it.

## Decision

`civbb-dynamics` (engine crate) compiles an avatar's chains into one structure-of-arrays particle list (`Rig`:
chains contiguous, parents first, parameters evaluated per particle from their graded curves at normalised depth,
clamped to their documented ranges) and keeps per-avatar `State`.

1. **Particles.** One per node below a chain root, ignored subtrees excluded, at most 256 per chain (the PhysBone
   component limit); with a non-zero endpoint, one extra tip per leaf. The root particle is pinned to the animated
   pose; with several children and `MultiChild::Ignore`, so are its children. A root with one child always aims at
   it.
2. **Rest vector.** Each particle's rest offset from its parent is the animated pose's offset in world space (or
   the posed offset, below). The pull target is the parent's simulated position plus that offset.
3. **Step.** Frames are clamped to 0.1 s and split into substeps of at most 1/60 s. Parameters refer to a
   reference step of 1/60 s, converted per substep `h` (`r = h * 60`). Per free particle, parents first:
   - inertia: `v = (x - x_prev) * spring^r` (`spring` is momentum with advanced integration);
   - gravity: acceleration `gravity * len / ref²` downward, times `1 - falloff * max(0, cos)` where `cos` compares
     the bone's direction with its rest direction;
   - pull: an XPBD constraint to the target with compliance `ref² (1 - pull) / pull`, so it moves
     `h² / (h² + compliance)` of the way. At equilibrium a bone sags `tan θ = gravity (1 - pull) / pull` at any `h`
     (hand-derived and tested at 60 and 144 Hz);
   - stiffness (advanced only): the particle stays within `2 len (1 - stiffness)` of the target;
   - length: the bone keeps its rest length. Motion never stretches; only grabs do, within
     `[len (1 - max_squish), len (1 + max_stretch)]`.
4. **Immobile.** Before stepping, particles (and their previous positions, so no velocity appears) move by
   `immobile` of the chain frame's motion since the last step: the root's parent (the avatar origin for the root
   node), rigidly for `AllMotion`, translation only for `World`.
5. **Grab and pose.** A grab holds one particle at a world target: the path from the first free ancestor points
   straight at it, the grabbed bone stretched or squished to reach. Releasing with posing allowed stores the
   current shape relative to the chain frame as the new rest offsets until the next grab or `unpose`.
6. **Output.** Parents first, each chain node keeps its animated local pose and then swings so its rest direction
   to each aimed child points at the simulated child (the root aims per `MultiChild`; other nodes average all
   children). Grab stretch scales the child's local translation. Chains read the animated pose of nodes outside
   themselves, so a chain inside another chain's subtree follows that chain's animated, not simulated, pose.
7. **Animator values.** `_IsGrabbed`, `_IsPosed`, `_Angle` (last particle's angle from its rest direction over
   180 degrees), `_Stretch` and `_Squish` (the most stretched or squished bone relative to its max) per chain.
   Writing them to prefixed parameters is the client's job.

## Consequences

- Unverified, all to be measured at parity (M0.9) and tuned in a superseding ADR: the reference step, gravity's
  unit, pull as compliance, stiffness's distance, falloff by direction cosine, immobile as carried motion, grab as
  straight-line reach, averaging of branches, and nested chains reading animated poses.
- Not represented in the format yet, so fixed: stretch motion (0: motion never stretches), grab movement
  (immediate), snap to hand, allow-collision filtering, the root's simulation timing (fixed or real time). Add
  them when the exporter or parity needs them.
- Colliders and limits come with #19; the solver will apply them after the length step.
- The particle list is laid out for batching (flat arrays, chains contiguous); explicit SIMD across chains waits
  for the benchmark (#23) to show it matters.
- Non-uniform node scale is approximated by its X component.
