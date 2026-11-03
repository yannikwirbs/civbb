# 0030: PhysBone chains are not merged at compile time; our solver has no per-chain batch cost to save

Status: accepted
Date: 2026-11-03

## Context

#32: design doc §21 lists "PhysBone merging" among the Phase 0 compiler optimisations, and the issue asks to merge
chains with identical parameters into fewer solver batches. In VRChat each PhysBone component has a fixed cost,
so optimisers merge components with equal settings under one root with `multiChildType = Ignore`. Our solver
(ADR 0015) already stores every chain's particles in one structure-of-arrays list; a chain adds only its frame
lookup, two small per-frame buffers and a loop over its particles. LESSONS: measure a compile-time optimisation
in our runtime before shipping it.

## Decision

1. **No merging stage.** A throwaway release benchmark stepped 18 five-bone chains with equal settings under one
   parent (VRM 0.x bone groups import like this: AliciaSolid has 18 chains) against the same bones as one chain
   rooted at that parent with `MultiChild::Ignore`: 25.0 to 25.3 µs per step for the 18 chains, 26.8 to 27.4 µs
   for the merged one (one more particle, the pinned common root). Merging would cost time.
2. It would also change behaviour: grabs, poses and `_IsGrabbed`-style parameters are per chain, so a merged
   chain could be held by one hand only, and graded parameters would be evaluated against a different depth.
3. The issue closes with this ADR. Dynamics cost is per particle (about 230 ns per particle per frame on the
   reference PC here); that is where to optimise, in the runtime.

## Consequences

- `civbb build` keeps chains as imported. VRChat content converted later (M0.10) that its creators merged
  stays merged; splitting is not needed either.
- Revisit if the solver gains per-chain batch work (GPU dynamics, SIMD over chains with equal settings), and
  measure again then.
