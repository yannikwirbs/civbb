# 0105: Island jobs on an own fork-join pool that wakes every worker at once and spins briefly

Status: accepted
Date: 2026-11-27

## Context

ADR 0103 point 3 runs the island jobs on a bevy_tasks pool, one task per island. #140 measured the vsync-paced
viewer (release, 100 bloated avatars, 13 workers): the island step took 1.35 ms wall for 6.6 ms of summed job CPU
(0.51 ms per worker), the extract step 0.126 ms for 0.40 ms (0.031 ms per worker). A probe of when each task started
showed only about 5 of 13 tasks starting within 65 µs; the rest started about 1.1 ms later, once the early threads ran
out of work. Two causes, separated by experiment:

- async-executor (under bevy_tasks) wakes one sleeping thread per notification, and a woken thread wakes the next:
  a chain that stalled after about 5 threads. One task per worker pulling islands from a shared iterator moved the
  extract step only to 0.113 ms.
- Windows wakes threads late on idle cores (this power plan parks down to 4 % of cores, `CPMINCORES`). With a
  condvar broadcast instead of the chain, still only about 5 threads started promptly; with an idle-priority
  busy loop keeping every core awake, 9 did and the island step fell to 0.82 ms.

## Decision

1. `civbb_core::jobs::Pool` is an own fork-join pool (about 100 lines of std, two `unsafe` lifetime erasures):
   `broadcast(job)` publishes the closure under a mutex, wakes every pool thread with one `notify_all`, runs the
   closure on the calling thread, then clears it and waits only for the pool threads that took it in time. Threads
   that wake after the calling thread finished skip that job, so a late waker never delays the step.
2. `jobs::islands` broadcasts one closure that takes the next island from a shared iterator until none are left
   (dynamic balance, no per-island task), summing counters under a mutex. Nothing allocates per call, so frames
   with threads are allocation-free too (#139's leftover).
3. After a job a pool thread polls for the next one for 200 µs before it sleeps. That keeps the threads (and their
   cores) awake across the short gap between the island step and the extract step, and measured best of 0, 50, 100
   and 200 µs.
4. bevy_tasks stays for package loading (`IoTaskPool`, ADR 0104); `civbb-core` no longer depends on it. This
   supersedes ADR 0103 point 3 for the island and extract jobs (no `ComputeTaskPool`, no fallback to rayon).

## Consequences

- Viewer, 100 bloated, 13 workers: simulation CPU 1.52 to 1.00 ms/frame; island step wall 1.35 to about 0.85 ms,
  extract 0.126 to about 0.087 ms (within 2x of CPU per worker, #140's bar). Bench (back to back): 1 worker unchanged
  (6.73 ms), 16 workers 0.75 to 0.69 ms.
- The spin costs up to 12 threads x 200 µs of CPU per parallel step and competes with the render thread for cores
  (render thread 0.69 to 0.74-0.84 ms per frame). Revisit when the render thread or audio becomes the critical path,
  or on battery-powered PCs: make `SPIN` a setting or drop it there.
- Part of the remaining gap belongs to the OS power plan, not the engine; the owner's plan parks cores (needs-user
  note in #140). Players' plans differ: measure with the plan in the report.
- The pool has no work stealing or nested jobs. Concurrent broadcasts (tests sharing `frame::workers()`) are sound:
  each caller runs its own job to the end and waits until no pool thread runs any job, but they share the threads
  poorly, and a pool thread's panic may surface in the other caller. When systems in the same set run in parallel
  (bevy's multi-threaded executor, ADR 0103 point 2), revisit: one job queue per broadcast, or a pool per set.
