# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues.

## Now

- Phase 0 (prove the avatar compiler). Current milestone: **M0.7 Renderer and viewer** (M0.6 closed).
- Next issue: #26 uber-shaders, bindless textures, indirect draws, then #27 desktop viewer in `civbb-client`, #49
  material curves. #24 and #25 are done.
- `civbb-render` (ADRs 0021, 0022): `Context` (Vulkan 1.3, validation in debug builds, device selection, one queue,
  gpu-allocator, buffers by device address), `Renderer` (FIFO sRGB swapchain with resize, two frames in flight),
  `Graph` (per-frame passes declaring colour attachments, sync2 barriers, dynamic rendering), `skin` (compute
  skinning, only non-zero-weight shapes via fixed-point atomics). `cargo run -p civbb-render --example clear` opens
  a window. Shaders are Slang in `crates/render/shaders/`, compiled by `cargo xtask shaders` (Vulkan SDK
  1.4.363.0 at `C:/VulkanSDK/1.4.363.0`) to committed SPIR-V; `ci` fails on a stale one.
- `civbb-core`: shared math, `jobs::islands` (one rayon job per avatar), per-system `Counters`. `civbb-client`:
  `Model`/`Island` (animator, constraints, IK stub, dynamics, world poses) and `frame::step` with the contact pass
  feeding next frame's parameters (ADR 0020); `civbb-client bench` prints JSON (#23).
- Benchmark on the reference PC (release): 100 bloated avatars 3.23 ms/frame on 1 thread, 0.43 ms on 16 (7.5x);
  1000 avatars 32.6 ms to 3.7 ms (8.9x). Dynamics is ~83 % of island CPU. Corpus avatars have no constraints and
  real converted avatars can't be benchmarked yet (#57, M0.9).
- `civbb-anim`: curve binding, clip sampling, state machines, blend trees, Write Defaults, built-ins, behaviours,
  humanoid muscles and constraints (ADRs 0007 to 0011, 0014, 0018, 0019). `civbb-dynamics`: XPBD PhysBone chains
  with grab/pose, colliders and limits (ADRs 0015, 0016), and contacts with the cross-avatar grid broadphase
  (`contact::step`, ADR 0017). Format v7.
- Unity ground truth: `tools/unity-truth` runs in batch mode and records JSON into crate test dirs (ADR 0013,
  `tools/unity-truth/README.md`). Muscle space and constraints (44 cases, batch play mode) are verified against it;
  animator goldens are still hand-derived (#54).
- Test avatars: `civbb-corpus` (`lean()`, `bloated()`, `generate`, `gltf::to_glb`), and locally `corpus/local/khronos/` (23 Khronos samples) `corpus/local/vrm/` (AliciaSolid 0.x, Seed-san and the twist sample 1.0) and `corpus/local/glb/` (Booth: HAOLAN, Taro, Tessakuchan, Yumeka, converted from FBX; see `corpus/README.md`). `civbb inspect <file>` prints what an avatar contains.
- Progress for the human: https://github.com/yannikwirbs/civbb/milestones

## Blockers and needs-user

- #37, #38 Unity tasks (M0.10, later). Sessions can run Unity themselves now; those two still need the human
  (Creator Companion projects, baselines).

## Decisions in flight

None. ADRs 0017 to 0020 are accepted. Dynamics constants and contact choices marked unverified wait
for parity (M0.9). In M0.10: #52 (body pose), #53 (muscle frames for glTF/VRM), #54 (Unity-recorded animator
goldens), #55 (box contacts, receiver value, body-part senders), #56 (animated constraint properties, VRChat
constraint options). In M0.9: #57 (benchmark packages and constraints).

## Session log

One line per session, newest last: date, what moved, what's next.

- 2026-10-28: repo, CI, xtask (ci/arch/codemap), ADRs 0001-0005, labels, 14 milestones, 43 issues. M0.1 and M0.2 done: `format` (avatar model, animator/dynamics/contacts/constraints/cost types, signed package, validation). #8 `civbb-corpus` generator done.
- 2026-10-28 (2): M0.3 done. `civbb-pipeline`: glTF importer with own bounded accessor reader (#9), VRM 0.x/1.0 (#10), `civbb` CLI (#12), fuzz crate + workflow (#43; found and fixed two gltf-crate panics). Format v2 adds `face`. Opened #44 (eye look, first person, licence), #45 (quantized/compressed glTF), #46 (VRM node constraints). Next: M0.4 from #13.
- 2026-10-28 (3): #11 done: four Booth avatars converted FBX to GLB with portable Blender, all import (up to 99k verts, 563 shapes). Their VRChat components sit in .unitypackage files; #47 decides how to read them.
- 2026-10-28 (4): M0.4 nearly done: new crate `civbb-anim` (#13 binding and sampling, #14 state machine, #15 blend trees, #16 Write Defaults and golden harness, #17 built-ins and behaviours), format v3 eye look, first person and licence (#44). ADRs 0007 to 0012. Opened #48 (muscles), #49 (material curves), #50 (weighted tangents), #51 (audio behaviour). Next: #48.
- 2026-10-28 (5): #48 muscles verified against Unity recordings (new `tools/unity-truth`, ADRs 0013, 0014, format v4), M0.4 closed. M0.5: new crate `civbb-dynamics`, #18 chain solver (ADR 0015), #19 colliders and limits (ADR 0016). Opened #52, #53, #54. Next: #20.
- 2026-10-29: M0.5 closed. #20 contacts and grid broadphase in `civbb-dynamics` (ADR 0017, format v5), #21 constraints in `civbb-anim` verified against 44 Unity recordings (ADR 0018, v6), #46 VRM node constraints as rest-relative kinds (ADR 0019, v7). Opened #55, #56. Next: M0.6 from #22.
- 2026-11-03: M0.6 closed. #22 new crates `civbb-core` (math moved from anim/dynamics, jobs, counters) and `civbb-client` (islands, frame, ADR 0020); #23 `civbb-client bench` JSON, 7.5x on 16 threads for 100 bloated avatars. Opened #57. Next: M0.7 from #24.
- 2026-11-03 (2): Vulkan SDK installed (winget). #24 new crate `civbb-render`: context, swapchain, frames in flight, render graph skeleton (ADR 0021); #25 compute skinning and sparse blendshapes, gpu-allocator, Slang toolchain with `cargo xtask shaders` (ADR 0022). Next: #26.
