# 0103: The client runtime on bevy_ecs: island entities, one task pool, a frame schedule and a render thread

Status: accepted
Date: 2026-11-27

## Context

Design doc §5 specifies `bevy_ecs` used standalone, a work-stealing scheduler with fibers for long jobs, a thread
table (orchestrator, workers = cores minus 2, render submit, audio, network, streaming), a ten-step frame pipeline in
which frame N's simulation overlaps frame N-1's rendering, per-thread arenas, and per-system timing with Tracy.
ADR 0003 kept Phase 0 on rayon over plain per-avatar data and moved all of that to the client; ADR 0020 built the
avatar islands (`Model`, `Island`, `frame::step`) on `core::jobs`. Today the viewer steps the islands, then skins and
draws them, on one thread in sequence. M1.2 (#130 to #136) moves the client onto the design. The doc does not say what
an entity is, which pool runs the jobs, what a fiber is in Rust, or how the render thread meets parity renders.

## Decision

1. **Entities.** One entity per avatar island: the `Island` component (unchanged, still the unit of parallel work)
   plus small components for owner id, local flag and origin. Not one entity per bone or node: §5 makes the avatar
   the island, and per-node entities would scatter one avatar's hierarchy across archetype tables. World objects
   become entities when worlds arrive (Phase 2). Engine crates (`anim`, `dynamics`, `render`) stay free of ECS types
   (ADR 0003); `civbb-core` depends on `bevy_ecs` and re-exports it, `civbb-client` defines the components and the
   systems that call the engine crates. `cargo xtask arch` needs no change.
2. **Schedule.** One `Frame` schedule whose system sets follow §5's order and run chained: `PosesIn`,
   `NetworkApply`, `Scripts`, `CommandApply`, `Avatars` (the island system), `CrossAvatar` (the contact pass),
   `WorldPhysics`, `Visibility`, `Extract`. Sets without systems yet are declared anyway so later milestones only add
   systems. Within a set the multi-threaded executor runs non-conflicting systems in parallel from their declared
   access (§5 "Scheduling rules"). Time (`dt`) and the contact permissions are resources.
3. **One task pool.** The client uses bevy_tasks' pools, not rayon: `ComputeTaskPool` with cores minus 2 workers (at
   least 1) for the schedule and the island jobs (`par_iter_mut` with a batch of 1 island, counters per worker through
   `bevy_utils::Parallel`), `AsyncComputeTaskPool` and `IoTaskPool` for streaming. Worker threads pin themselves
   to performance cores in `on_thread_spawn` (ADR 0080's CPU sets), I/O threads prefer efficiency cores. One pool
   avoids two schedulers oversubscribing the same cores. **Fallback:** if the bench (#132) at 100 and 1000 bloated
   avatars on 1 and 16 workers is more than 10 % slower than rayon right before the switch, the island system keeps
   calling rayon inside itself and rayon's pool takes the worker count instead; the schedule then runs single-threaded.
4. **No stackful fibers.** Rust has no maintained cross-platform fiber library, and stackful fibers break thread-local
   state (arenas, Tracy zones). Long jobs (package decode, texture transcode, later scripts at their fuel limit) are
   async tasks on `AsyncComputeTaskPool`/`IoTaskPool` that yield at await points; their results enter the world at the
   start of a frame. This is the doc's intent ("yield instead of blocking a worker") without fibers.
5. **Extract and the render thread.** The `Extract` set copies what the renderer reads (skin matrices, shape weights,
   changed material records, activity, camera) into a render snapshot. A render thread owns the `Renderer` and
   records and submits snapshot N-1 while the schedule runs frame N; the hand-over is a channel of capacity 1, so
   simulation is never more than one frame ahead. The main thread owns the window and event loop and runs the
   schedule (the orchestrator). Parity and headless captures run synchronous: extract, then render the same frame on
   the calling thread, so reports stay byte-identical.
6. **Arenas and timing.** Hot loops use scratch that lives in the island or the worker and is cleared, not freed,
   each frame (#135); `bumpalo` only if that is not enough. The `Counters` of ADR 0020 stay always on and gain one
   entry per schedule system; Tracy zones (`tracy-client`) per system, island job and render pass sit behind an
   off-by-default `profile` feature (#136).
7. **Threads not yet built.** Audio (M1.6) and network (M1.5) threads come with their milestones; this ADR only
   reserves their place in the schedule (`NetworkApply`) and the pinning rule (latency-critical threads on
   performance cores).

## Consequences

- Supersedes ADR 0003 (its Phase 0 scope has ended) and point 1's `jobs` part of ADR 0020 once #132 lands; ADR 0020's
  island order and contact timing stay.
- `frame::step` and `core::jobs` disappear; the bench, the viewer and `tests/frame.rs` drive the schedule. Frame
  results must not change: parity stays byte-identical to s129 and the frame tests pass unchanged.
- The viewer's frame costs max(simulation, recording) instead of their sum, at one frame of latency. OpenXR (M1.4)
  re-reads poses at submit time (late latching, §5 step 10), which this split already allows.
- Revisit point 3 if bevy_tasks' scheduling shows up in Tracy as a measurable share of the frame.
