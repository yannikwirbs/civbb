# Sessions

One line per session, newest last: date, what moved, what's next. Append at the end of every session.

- 2026-10-28: repo, CI, xtask (ci/arch/codemap), ADRs 0001-0005, labels, 14 milestones, 43 issues. M0.1 and M0.2 done: `format` (avatar model, animator/dynamics/contacts/constraints/cost types, signed package, validation). #8 `civbb-corpus` generator done.
- 2026-10-28 (2): M0.3 done. `civbb-pipeline`: glTF importer with own bounded accessor reader (#9), VRM 0.x/1.0 (#10), `civbb` CLI (#12), fuzz crate + workflow (#43; found and fixed two gltf-crate panics). Format v2 adds `face`. Opened #44 (eye look, first person, licence), #45 (quantized/compressed glTF), #46 (VRM node constraints). Next: M0.4 from #13.
- 2026-10-28 (3): #11 done: four Booth avatars converted FBX to GLB with portable Blender, all import (up to 99k verts, 563 shapes). Their VRChat components sit in .unitypackage files; #47 decides how to read them.
- 2026-10-28 (4): M0.4 nearly done: new crate `civbb-anim` (#13 binding and sampling, #14 state machine, #15 blend trees, #16 Write Defaults and golden harness, #17 built-ins and behaviours), format v3 eye look, first person and licence (#44). ADRs 0007 to 0012. Opened #48 (muscles), #49 (material curves), #50 (weighted tangents), #51 (audio behaviour). Next: #48.
- 2026-10-28 (5): #48 muscles verified against Unity recordings (new `tools/unity-truth`, ADRs 0013, 0014, format v4), M0.4 closed. M0.5: new crate `civbb-dynamics`, #18 chain solver (ADR 0015), #19 colliders and limits (ADR 0016). Opened #52, #53, #54. Next: #20.
- 2026-10-29: M0.5 closed. #20 contacts and grid broadphase in `civbb-dynamics` (ADR 0017, format v5), #21 constraints in `civbb-anim` verified against 44 Unity recordings (ADR 0018, v6), #46 VRM node constraints as rest-relative kinds (ADR 0019, v7). Opened #55, #56. Next: M0.6 from #22.
- 2026-11-03: M0.6 closed. #22 new crates `civbb-core` (math moved from anim/dynamics, jobs, counters) and `civbb-client` (islands, frame, ADR 0020); #23 `civbb-client bench` JSON, 7.5x on 16 threads for 100 bloated avatars. Opened #57. Next: M0.7 from #24.
- 2026-11-03 (2): Vulkan SDK installed (winget). #24 new crate `civbb-render`: context, swapchain, frames in flight, render graph skeleton (ADR 0021); #25 compute skinning and sparse blendshapes, gpu-allocator, Slang toolchain with `cargo xtask shaders` (ADR 0022). Next: #26.
- 2026-11-03 (3): #26 GPU-driven forward pass in `civbb-render` (shared buffers, bindless, uber-shaders, indirect runs, graph depth/transients/buffers; ADR 0023), #27 desktop viewer `civbb-client view` (ADR 0024). Opened #58 to #62. Next: #49.
- 2026-11-03 (4): M0.7 closed: #49 material curves bound per renderer to native fields, per-island material records rewritten in the frame (ADR 0025). M0.8 started: #28 `civbb build` prunes unweighted joints, unreferenced nodes and undriven shapes (ADR 0026). Next: #29.
- 2026-11-03 (5): #29 mesh merging (ADR 0027; Yumeka 23 to 7 draws, viewer reports draws), #30 toggle layers to one direct blend tree (ADR 0028; blend weights no longer allocate, collapse was 4x slower before). Next: #31.
- 2026-11-03 (6): #31 texture atlasing and KTX2 (ADR 0029; AliciaSolid package 5.6 to 2.8 MB, one atlas, draws 13 to
  12, viewer transcodes to BC7), #32 closed by ADR 0030 (merged chains measured slower). Next: #33.
- 2026-11-03 (7): M0.8 closed. #33 skin-aware LOD chain with meshoptimizer (ADR 0031, format v8): three prefix
  levels per mesh, `civbb-client view --lod N`, AliciaSolid captured at each level. Opened #63. The fuzzer found a `basisu` overflow panic on KTX2 header ranges, fixed (#64).
  Next: M0.9 from #34.
- 2026-11-04: #34 per-avatar report (ADR 0032): stats per compiler stage, `civbb-client measure` (bench on one
  worker plus headless viewer frames), `civbb build --report --measure`, reports for all 31 local files; `view
  --frames` warm-up. #57 closed: `bench --package`, corpus constraints. Opened #65, #66. Fuzzer found #67 (KVD alignment), fixed. Next: #35.
- 2026-11-04 (2): #35 FLIP parity (ADR 0033): `civbb-client parity`, `Renderer::read_last`, `flip-rs` (nv-flip's
  tinyexr clashes with basisu's at link time), report `parity` key, all 32 corpus/local files scored (table on #35).
  #58 toon outlines as inverted hulls (ADR 0034). Opened #68 (ETC1S loses visible detail), #69 (framing). Next: #65
  with #68.
- 2026-11-05: M0.9 closed. #65/#68 texture codec (ADR 0035): ETC1S could not reach green; per-texture ladder
  UASTC / XUASTC 90 / XUASTC 75 within the source size, shared CPU mips (`civbb_core::mips`, renderer too),
  wrapping 16-texel atlas gutters, encoder threads capped at 4; textured files green 13 of 16 (was 4). Found and
  fixed a `basisu` header-count overflow on XUASTC. #66 constraint pass 25 to 50 % faster. #69 rig frames the rest
  pose (ADR 0036). Next: M0.10 from #47.
- 2026-11-05 (2): #47 closed by ADR 0037 (exporter, not a unitypackage reader; Booth packages measured). #36
  started: `tools/unity-exporter` UPM package with batch import and a GLB writer (ADR 0038), compile-checked only:
  the Unity licence lapsed (#72). #50 closed by ADR 0039 (curves stay Hermite). #56: animated constraint fields,
  local space and freeze to world (ADR 0040, format v9). Opened #71, #72, #73. Next: #72 (human), then #55 or #71.
- 2026-11-06 (2): local `cargo fuzz` links on MSVC again (`flip-rs` behind `civbb-client`'s `parity` feature). Owner reactivated the Unity licence and updated the CLI (beta.17), Editor now 6000.3.28f1; #72 closed. Next: #36 (run the exporter).
- 2026-11-06 (3): #36 exporter run on the five Booth packages: `ImportPackage` is async (fixed), 13 prefabs export and match the Blender GLBs (table on #36). ADR 0041 `X_*` = format JSON; #71 importer part 1 (avatar, animator, constraints, dynamics, contacts, expressions, active). Exporter writes animators, clips, Unity constraints; all five build. Next: #37 (owner: VRChat SDK host project), meanwhile `X_toon_material`, #55, #73.
- 2026-11-06 (4): #37 closed. New avatars extracted (zips verified, deleted) and converted. Exporter on the SDK project: VRChat components, muscle frames, face (ADR 0042), `X_toon_material` (ADR 0043); exported clips and constraints match Unity exactly (`unity_export` test); constraint at-rest from the transform; hips left to the body pose. Opened #74, #75. Next: #75 (#73, #55).
- 2026-11-18: iteration cost tuning (`docs/iteration-baseline.md`): dev workspace crates at opt-level 1 (tests 82 to 14 s, gate 88 to 18 s), session state split by read frequency (ADR 0044; session start 70 to 17 KB), quiet `cargo xtask ci`, `cargo xtask test` (changed crates only), `tools/unity-batch.sh`. Next: #75 (#73, #55) as before.
