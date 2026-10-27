# 0003: Phase 0 uses rayon over plain per-avatar data; bevy_ecs and the fiber scheduler come with the client

Status: accepted
Date: 2026-10-28

## Context

Design doc §5 specifies `bevy_ecs` with a custom work-stealing scheduler with fibers. Phase 0 only needs N independent avatar islands (animator, constraints, dynamics) run in parallel and timed.

## Decision

Phase 0 runs avatar islands with `rayon` over contiguous per-avatar data. The avatar runtime crates (`anim`, `dynamics`) expose plain data and pure step functions with no ECS types, so they drop into `bevy_ecs` systems later unchanged.

## Consequences

The scaling measurement in Phase 0 is valid for the island model, which is the part that matters. ECS integration and the custom scheduler are Phase 1 work.
