# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`.

## Now

- **Phase 0 health check in progress** on branch `health/phase-0` (not merged yet): plan and progress in
  `docs/ai/reviews/phase-0-health-check.md`. Finish its "Still to do" list first, then merge into `main`.
- **Phase 0's gate is not passed (ADR 0078).** Milestone **M0.11 Phase 0 gate** comes before Phase 1: #38 (Unity
  CPU baselines) and #101 (compiled avatars against Unity's renders). Evidence command: `python corpus/scorecard.py`.
  Epic #39 is split only after M0.11. Phase 1 leftovers already filed: #60 MSAA and multiview, #61 pipeline cache, #62 clustered lights, #63 LOD switching, #70
  locomotion extras, #77, #92, #93, #94; #51 (audio behaviour) waits for an audio crate (no corpus avatar uses it).
- Blackbird's worst parity states (0.35) show the right wing set in a different arm pose (not yet investigated). The
  local branch `wip/52-body-pose` is merged into main (542bea6) and can be deleted.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` for this machine and CI (ADR 0045); bump it on purpose.
  `.githooks/pre-push` runs `cargo xtask ci` (enable per clone: `git config core.hooksPath .githooks`).
- Progress for the human: https://github.com/yannikwirbs/civbb/milestones

## Blockers and needs-user

- #38 baselines (moved to Phase 1; sessions can likely run the AV3 emulator and AvatarOptimizer themselves).

## Decisions in flight

None. ADRs 0017 to 0078 are accepted (0035 supersedes the codec, mip and gutter parts of 0029; 0036 the rig
bounds of 0033; 0046 the capture frame and rebaking of 0040; 0047 refines 0017's shapes and proximity; 0049
supersedes 0015 and the solver points of 0016; 0055 the frame count of 0033; 0062 the HDR conversion and reference
time of 0052; 0065 ADR 0010's WD-on empty states and 0028's `Start` state; 0066 the interruption points of 0008; 0067
the material default of 0025; 0068 the version 1.0 gravity of 0049; 0070 the collider velocity of 0049; 0074 the
partial output step and in-frame interpolation of 0049; 0072 corrects 0064's thumb account; 0077 defers ADR 0023
point 7's GPU culling to Phase 2, #59).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-23 (3): #84 closed (ADR 0074): from the SDK's per-root timing and per-bone state at 90 and 144 Hz, the
  output between steps blends the last full step's end (raw, from the shown parent) towards the frame's collided step
  by `r / (1 - pull (1 - r))`, limits after; steps counted in f32 as the SDK accumulates; step `j` against the pose
  interpolated from the last step's by `j / executions`, colliders from the last frame's; falloff reads the offset as
  shown. Every 90 Hz case within 0.2 mm (was 1 to 46 mm). #71 closed (fuzz seed for `X_toon_material`, clean run).
  Next: decide #36, then Phase 1.
- 2026-11-23 (4): #36 closed (ADR 0075): no NDMF/Modular Avatar/VRCFury in any of the 13 local packages (pathname
  and text scan with a positive control); pre-build #97 and export-time renders #98 to Phase 3. M0.10 closed, #38
  to Phase 1. #45 closed (ADR 0076): `KHR_mesh_quantization` accepted, required Draco/meshopt refused. #99 closed
  (ADR 0077): mesh instances culled on the CPU before skinning; #59 (GPU culling, Hi-Z) to Phase 2. #51 deferred
  (no corpus use, no audio crate). Next: split epic #39.
- 2026-11-24: Phase 0 health check, part 1 (branch `health/phase-0`, ADR 0078). Phase 0's gate is not passed:
  no Unity CPU baseline, no export green against Unity. M0.11 opened (#38, #101). Fixed three `lod` bugs on
  hostile input (a meshoptimizer assert, two invalid packages), added the `compile` fuzz target, and let parity
  score compiled packages against Unity. `corpus/scorecard.py` added. Next: the report's "Still to do".
