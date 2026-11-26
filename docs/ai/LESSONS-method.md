# Lessons: method

Read before writing a test meant to catch a bug, refactoring behind a pin, measuring or benchmarking, or starting a
run that takes minutes.

- Mutation-check equivalence tests, and make the mutated case reachable: a parameter driver that only fires on
  frame 1 (when every layer starts in its default state anyway) could not show a driver-timing bug.
- Before calling a test a check, make each case one the code under test could fail: a freeze test whose source
  never moved while frozen passed with freezing switched off. Mutate the branch and see the test fail.
- Goldens of floats: compare the serialised text, not re-parsed `serde_json::Value`s (without the `float_roundtrip`
  feature parsing is off by an ULP, so a freshly blessed golden "differs").
- Before/after binaries: `git worktree add --detach <scratchpad>/wt HEAD`, build there with
  `--target-dir <scratchpad>/wt-target`, then `git worktree remove --force` it. Piping `civbb build` into `head`
  makes it panic with "failed printing to stdout"; harmless.
- Behaviour pins for refactors: a standalone scratch crate in the scratchpad with path dependencies on the repo's
  crates (plus a copy pointing at a `git worktree` of the start commit for the baseline) stays out of git, the code
  map and `xtask ci`. Compare with `cmp` and a line count, never by counting `diff` lines: a harness whose setup
  panics under a silenced panic hook writes an empty file that "has no differing lines". Mutation-check the pin.
- After splitting a file, `cargo fix --allow-dirty -p <crate> --lib --tests` prunes the copied imports; it changes
  nothing while any compile error remains. Clippy may flag moved code it ignored inside a very long function
  (`manual_is_multiple_of` on 2026-11-24).
- Splitting uncommitted work into two commits: copy the files to the scratchpad first, revert one change with the
  Edit tool (Python replacements miss text that `cargo fmt` has reformatted), commit, then copy the files back.
- Benchmarks: `cargo run` defaults to debug; numbers only mean something from `--release` (the report's `profile`
  field says which). Don't benchmark while compiling: run timing batches alone (background them and wait).
- One-worker timings on the hybrid i5-12600K land on a P- or an E-core at random (up to 1.8x apart, a thread can
  migrate mid-run): pin them (`cmd //c "start /affinity 4 /wait /b <exe> ..."` from Git Bash, logical CPU 2 is a
  P-core; `taskset -c 2` on Linux) and run twice before believing a before/after difference (#103).
- A throwaway study that runs for minutes: check it uses more than one core (CPU seconds against wall time) and that
  its filters still apply after edits; a variant list that stopped honouring `STUDY_VARIANTS` ran full encodes per
  file and looked like a leak. Pipe long output through `grep --line-buffered` or progress stays invisible.
- Long measurement runs (the 2026-11-05 session took 202 min, about 60 of them avoidable): before starting a
  corpus-wide run, (1) run it on the smallest input first and on one real avatar, (2) after a minute check CPU
  seconds against wall time and committed memory, (3) drop variants the one-avatar run already rules out, (4) run
  one variant when only images or one property are needed, and (5) never end the turn just to wait: do the next
  independent work meanwhile.
- Before fitting an engine's behaviour from its outputs, read its own state and constants by reflection (field names
  from an `ApiProbe` run, then values): the SDK's per-bone `prevVelocity`, its `COLLISION_FRICTION`, Unity's
  `Avatar.GetAxisLength` and `humanDescription.skeleton` each turned an hour of ill-conditioned fitting into one
  clean regression (2026-11-23: #83, #84, #95, #96).
