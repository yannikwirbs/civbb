# 0080: `measure` pins its worker to a performance core

Status: accepted
Date: 2026-11-24

## Context

`civbb-client measure` (ADR 0032) times `bench` on one worker; its CPU time per avatar feeds the per-avatar report,
`civbb build --measure` and the Phase 0 scorecard. The reference PC's i5-12600K is hybrid: logical CPUs 0 to 11 are
performance cores, 12 to 15 efficiency cores. Unpinned, the worker landed on either at random and could migrate
mid-run: Yumeka's import measured 185, 316 and 358 µs in three runs; pinned, 196 and 201 µs on a performance core
against 364 and 310 µs on an efficiency core (#103). The scorecard pinned the whole process from outside
(`start /affinity`), which nobody else calling `measure` did. #38 compares our CPU cost with Unity's, which is only
meaningful on the same core class.

## Decision

1. `measure` pins its one bench worker to the highest-numbered logical CPU of the fastest core class, and only that
   thread (the GPU half and the driver's threads stay free). On Windows the class is `EfficiencyClass` from
   `GetSystemCpuSetInformation` (higher is faster), the pin `SetThreadAffinityMask`, in processor group 0. The
   highest number keeps it off CPU 0, which takes most interrupts. On a non-hybrid CPU every core has the same class,
   so this pins to the last logical CPU.
2. The report records it: `cpu.pinned` = `{logical_cpu, efficiency_class, ran_on}`, `ran_on` read with
   `GetCurrentProcessorNumber` after the last measured frame. `null` means unpinned: another OS, or the OS refused
   (for example a process already confined to efficiency cores). The Markdown report says which.
3. Unity baselines (#38) are measured on the same core class (how Unity's job threads are confined is #38's call).
4. Other operating systems do not pin yet. The reference PC runs Windows (design doc's reference hardware); Linux
   pinning (`sched_setaffinity`, core class from `cpu_capacity` or `cpuinfo_max_freq`) needs `libc` and waits for a
   Linux machine whose numbers matter. `corpus/scorecard.py` refuses unpinned numbers.
5. Dependency: `windows-sys` 0.61 (Windows only), already in `Cargo.lock` through winit, so no new package.
6. Check: two consecutive pinned runs of one package agree within 10 % (`two_pinned_runs_agree_within_ten_percent`,
   `#[ignore]`d, run alone with `--release --ignored`).

## Consequences

- Timings are performance-core timings: about half what an efficiency core gives. The runtime governor's budgets
  (design doc §8) must read them as such.
- Multi-worker `bench` runs stay unpinned: they measure scaling, where the OS's placement is part of the result.
- Revisit if the reference PC changes to a CPU with more than one processor group or a favoured-core scheme where the
  highest-numbered fast core is not the fastest.
