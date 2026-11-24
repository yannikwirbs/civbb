# 0081: How the Unity CPU baselines are measured

Status: accepted
Date: 2026-11-24

## Context

Phase 0's gate (ADR 0004, ADR 0078) needs the CPU cost per avatar of "the same avatar in Unity with an Avatars 3.0
emulator, plus what existing optimisers achieve" (design doc §21), next to ours (`civbb-client measure`: one worker
pinned to a performance core, ADR 0080). #38's plan left open where Unity runs, how its many threads are counted,
and what to do if the Editor's numbers do not represent the client.

Two findings decide it (2026-11-24, `corpus/local/unity-export-vrc`, Unity 2022.3.22f1, VRChat SDK 3.10.6):

1. **The SDK's dynamics are not Burst-compiled.** None of the job structs in the SDK's `VRC.Dynamics.dll`
   (`PhysBoneManager+PhysBoneJob`, `ReadBoneJob`, `VRCConstraintJob`, the contact broadphase jobs) carries
   `[BurstCompile]`, and `VRC.Primitives.dll` ships its own stub `Unity.Burst.BurstDiscardAttribute`. Burst itself
   works in the same batch-mode Editor (a probe job: 63 ms Burst against 541 ms managed); disabling Burst
   (`--burst-disable-compilation`) leaves PhysBone time unchanged (151 against 156 ms for 40 Yumeka). So in Unity
   with the SDK, in the Editor or in any player built from it, PhysBones, contacts and VRChat constraints run as
   managed jobs; the VRChat client is not bound by that. 40 Yumeka copies spend 156 of 168 ms in `PhysBoneJob`.
2. **The animator is engine code.** Mecanim (`Animators.*`, `Director.*`) is native and the same engine the
   client runs, so the Editor's animator cost is a fair stand-in for the client's.

## Decision

1. **Where:** Editor play mode in batch mode, in the SDK host project, by `Civbb.Exporter.Baseline.Measure`
   (`tools/unity-exporter`; runner `corpus/unity_baseline.py`). The baseline the design doc names is an Editor tool,
   and a standalone player would not change finding 1. Playable layers are set up as `Reference.Render` sets them
   (ADR 0048: VRChat's group order, Additive additive, Action at weight 0, descriptor masks, the SDK's proxy clips
   played empty, parameter drivers and layer controls hooked; layer controls set their goal weight at once).
2. **Same work as our bench:** copies on a square grid 1 m apart, each swaying sideways with the bench's formula,
   one parameter flipped every 30 frames in the controllers' parameter order, frame time pinned to 1/90 s
   (`Time.captureDeltaTime`); 60 warm-up and 300 measured frames; 0, 1 and 40 copies.
3. **Same core:** Unity's job workers off (`JobsUtility.JobWorkerCount = 0`, every job runs on the main thread)
   and the process confined to the logical CPU `measure` pins to. A frame's `PlayerLoop` time is then all the CPU
   work Unity did for it on one performance core, as our one worker's frame time is.
4. **Per avatar:** (`PlayerLoop` at N copies − at 0 copies) / N, so the Editor's fixed per-frame work drops out.
   Split by markers: animator = `Update.DirectorUpdate` + `PreLateUpdate.DirectorUpdateAnimationBegin` + `…End`;
   SDK dynamics = the three `VRCAvatarDynamics*` markers; Unity constraints = `PreLateUpdate.ConstraintManagerUpdate`.
5. **What the gate compares:** the total (literally the design doc's baseline) and the animator alone, which is the
   like-for-like part. The SDK-dynamics part is reported but not used to call our runtime cheaper: it measures
   managed SDK code the client does not run.
6. **Optimiser:** Anatawa12's AvatarOptimizer (MIT, via VPM with NDMF, in a copy of the host project, local only),
   its Trace and Optimize component added to each prefab and run through NDMF before measuring the same way.
   d4rk's optimizer is not measured: AvatarOptimizer is the one the design doc names first, and one optimiser
   answers whether compiling beats existing optimisers.

7. **Emulator check** (`-civbbEmulator`, Lyuma's Av3Emulator 3.5.0, MIT, local only): one Yumeka's `PlayerLoop`
   is 4,564 µs under the emulator against 4,477 µs with our setup (+2 %). The emulator evaluates its graph from a
   script and still created a non-local clone with clones switched off, so its animator time could not be split
   out; the 5 % animator check of #38's plan is inconclusive (#104). It cannot change the gate: the
   compiled avatars' animator is at least 3.0 times cheaper than Unity's, so only a setup that made Unity's
   animator more than three times cheaper would.

## Consequences

- Unity's PhysBone numbers are an upper bound on the client's; any claim about PhysBone cost against VRChat needs a
  client measurement, which this project cannot make (no VRChat client or API use).
- `PlayerLoop` also holds per-avatar work our bench does not do (transform change dispatch, the SDK's managed
  bookkeeping); the animator split is the cleaner comparison.
- The Editor-against-player check of #38's plan is not done: it cannot move the SDK's dynamics, and the animator is
  native in both. Revisit if a future SDK ships Burst-compiled jobs.
