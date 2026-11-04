# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues.

## Now

- Phase 0 (prove the avatar compiler). Current milestone: **M0.9 Reports and parity** (M0.8 closed).
- Next issue: #35 FLIP parity, then #58 toon outlines, #65 KTX2 sizes, #66 constraint pass cost.
- Per-avatar report (ADR 0032): `civbb build <src> --key <k> --report r.json [--measure]` writes `r.json` and `r.md`:
  stats after import and after each stage (`civbb_pipeline::compile` runs them), package size, and with
  `--measure` the output of `civbb-client measure` (run as a process from next to `civbb`; release only): `bench`
  on one worker and the viewer's frames on an offscreen target (`Renderer::headless`), GPU null without a device.
  Golden for lean counts: `crates/pipeline/tests/golden/report-lean.json` (`CIVBB_BLESS=1` rewrites it). All 31
  files in corpus/local build with measured reports (summary on #34): e.g. Yumeka draws 23 to 7, 563 shapes to 0,
  7.6 µs CPU and 17.9 µs GPU per avatar; HAOLAN textures 19.0 to 3.3 MB, 33.5 µs GPU.
- Compiler stages in `civbb-pipeline`, run by `civbb build` between import and packaging, each proven
  behaviour-identical by tests that step islands before and after:
  - Order in `civbb build`: `prune`, `atlas`, `merge`, `lod`, `collapse`, `compress`.
  - `prune` (ADR 0026): zero-weight skin joints, unreferenced nodes (ancestors and chain descendants kept) and
    shapes nothing drives (clips, face binds, Unified Expressions and ARKit names, non-zero rest weight).
  - `merge` (ADR 0027): meshes under the same `Active`-animated nodes (same first-person mode and vertex streams)
    become one skinned mesh, rigid ones skinned to their node; same-material opaque/cutout submeshes join. Meshes
    with renderer or material curves, blended submeshes, shared nodes or colliding shape names stay apart. Draws per
    copy: Yumeka 23 to 7 (20 skinned meshes to 1), AliciaSolid 20 to 13, HAOLAN 15 to 9. Bloated only loses one
    mesh: every outfit and prop has its own toggle.
  - `atlas` (ADR 0029): materials with the same model and UV slots share an atlas (8-texel gutters, 4-texel
    aligned, shelf packed within the platform cap); whole-tile UV offsets are shifted back; animated materials,
    UV1, `_ST` tiling, tile-crossing triangles and textures used elsewhere are left alone. Materials then equal
    become one so `merge` joins their draws. AliciaSolid: textures 7 to 3, materials 12 to 10, draws 13 to 12.
  - `lod` (ADR 0031, format v8): up to three levels per mesh from meshoptimizer (`meshopt` 0.7), aiming at 1/2,
    1/4, 1/8 of the triangles with error caps 0.5/1.5/4 % of the mesh extent, kept only at <= 80 % of the level
    above. A level is an index list over a vertex prefix (vertices reordered), so it skins and applies shapes to
    fewer vertices; skin anchor and shape motion are simplification attributes, submesh borders locked
    (`LockBorder`). Non-manifold edges (3+ faces) may open. AliciaSolid 31.8k, 16.4k, 9.2k, 7.8k triangles;
    HAOLAN 146k to 21k. `Mesh::level(n)` gives a level as a mesh. Runtime switching is #63 (Phase 1).
  - `compress` (ADR 0029): every texture to KTX2 with `basisu_c_sys` (C++ Basis Universal, bindgen needs
    libclang), UASTC for normal maps, ETC1S otherwise, single-threaded per texture (reproducible), sides capped at
    4096 desktop / 2048 Quest. AliciaSolid package blobs 5.6 MB to 2.8 MB, Seed-san 8.6 to 4.9 MB.
  - `collapse` (ADR 0028): plain bool toggle layers (two constant clips, zero-duration transitions) become one
    layer per group: a `Start` state, then a Write Defaults on direct tree of 1D trees weighted by `civbb/One`.
    Bloated 48 layers to 1, animator update 2.34 to 2.08 µs. Direct and 1D blend weights no longer allocate.
- The viewer's title and `--frames` summary report draws and skinned meshes of the last frame. KTX2 textures are
  checked (header, level index) and transcoded to BC7 by the pure-Rust `basisu` crate; the renderer uploads BC7
  levels and device selection requires BC textures. Fuzz target `ktx2_transcode`.
- Animated materials (ADR 0025): `civbb_anim::material` maps Unity property names (`_Color`, `_EmissionColor`,
  `_MainTex_ST`, `_Glossiness`, MToon/lilToon/Poiyomi/UTS2 names) to native fields in Unity's units (sRGB colours,
  smoothness, V-up offsets); `Target::Material(mesh, field, component)` per renderer. `Model::animated_materials`
  and `Island::materials` give each island its own records for animated renderers; the viewer rewrites changed
  records through `Forward::add`'s upload pass.
- `civbb-render` (ADRs 0021 to 0023): `Context` (Vulkan 1.3, validation in debug builds, device selection, one
  queue, gpu-allocator, buffers by device address, bindless and multi-draw-indirect features), `Renderer` (FIFO
  sRGB swapchain with resize, two frames in flight, GPU timestamps `gpu_ms`, `blocked_ms`), `Graph` (colour and
  depth attachments, pooled transient images, buffer resources, sync2 barriers, dynamic rendering), `Scene`
  (shared rest-vertex, attribute, delta, index and skinned buffers, one material buffer, 4096-slot bindless
  textures), `skin` (compute skinning, sparse blendshapes via fixed-point atomics), `Forward` (twelve pipelines:
  PBR, Toon, Unlit x blend x culling; draws batched into runs, one indirect multi-draw per run, vertices pulled by
  address). Shaders are Slang in `crates/render/shaders/` (`skin`, `mesh`), compiled by `cargo xtask shaders`
  (Vulkan SDK 1.4.363.0 at `C:/VulkanSDK/1.4.363.0`) to committed SPIR-V; `ci` fails on a stale one.
- Viewer (ADR 0024): `civbb-client view [<pkg> --key <key> | --avatar lean|bloated] [--copies N] [--frames F]
  [--warmup W] [--lod L]`; the title shows triangles per copy; `--frames` summarises the F frames after a 60-frame
  warm-up (ADR 0032).
  glTF/VRM go through `civbb build` first (client may not depend on pipeline). Release, reference PC: 100 bloated
  avatars 1.72 ms CPU per frame (systems 0.53 ms), 0.56 ms GPU; 4 Yumeka 1.5 ms CPU, 0.21 ms GPU. AliciaSolid
  renders textured and toon-shaded. Debug builds are ~30x slower on the CPU (validation).
- `civbb-core`: shared math, `jobs::islands` (one rayon job per avatar), per-system `Counters`. `civbb-client`:
  `Model`/`Island` (animator, constraints, IK stub, dynamics, world poses) and `frame::step` with the contact pass
  feeding next frame's parameters (ADR 0020); `civbb-client bench [--package <pkg> --key <key>]` prints JSON (#23,
  #57).
- Benchmark on the reference PC (release): 100 bloated avatars 3.23 ms/frame on 1 thread, 0.43 ms on 16 (7.5x);
  1000 avatars 32.6 ms to 3.7 ms (8.9x), measured before the corpus got constraints. Corpus constraints (#57): lean
  2 twist helpers, bloated 67 (twists and tail bones); bloated one worker 30.0 to 42.9 µs per avatar, of which
  constraints 12.1 µs and dynamics 25.7 µs; the constraint pass is slow (#66).
- `civbb-anim`: curve binding, clip sampling, state machines, blend trees, Write Defaults, built-ins, behaviours,
  humanoid muscles and constraints (ADRs 0007 to 0011, 0014, 0018, 0019). `civbb-dynamics`: XPBD PhysBone chains
  with grab/pose, colliders and limits (ADRs 0015, 0016), and contacts with the cross-avatar grid broadphase
  (`contact::step`, ADR 0017). Format v8 (LOD levels, ADR 0031).
- Unity ground truth: `tools/unity-truth` runs in batch mode and records JSON into crate test dirs (ADR 0013,
  `tools/unity-truth/README.md`). Muscle space and constraints (44 cases, batch play mode) are verified against it;
  animator goldens are still hand-derived (#54).
- Test avatars: `civbb-corpus` (`lean()`, `bloated()`, `generate`, `gltf::to_glb`), and locally `corpus/local/khronos/` (23 Khronos samples) `corpus/local/vrm/` (AliciaSolid 0.x, Seed-san and the twist sample 1.0) and `corpus/local/glb/` (Booth: HAOLAN, Taro, Tessakuchan, Yumeka, converted from FBX; see `corpus/README.md`). `civbb inspect <file>` prints what an avatar contains.
- Progress for the human: https://github.com/yannikwirbs/civbb/milestones

## Blockers and needs-user

- CI for cdbac2b, 938fd3f and later had not run when the session ended (GitHub Actions partial outage,
  2026-11-04); `cargo xtask ci` passed locally. First thing next session: `gh run list`, fix anything red.

- #37, #38 Unity tasks (M0.10, later). Sessions can run Unity themselves now; those two still need the human
  (Creator Companion projects, baselines).

## Decisions in flight

None. ADRs 0017 to 0032 are accepted. LOD follow-up: #63 runtime level switching in renderer and governor. Renderer follow-ups: #58 toon outlines (M0.9, parity needs them), #59 GPU
culling, #60 MSAA and multiview, #61 pipeline cache and freeing scene memory, #62 clustered lights and shadows
(Phase 1). Dynamics constants and contact choices marked unverified wait
for parity (M0.9). In M0.10: #52 (body pose), #53 (muscle frames for glTF/VRM), #54 (Unity-recorded animator
goldens, now also material curve defaults, sRGB and `_ST` conventions per ADR 0025), #55 (box contacts, receiver value, body-part senders), #56 (animated constraint properties, VRChat
constraint options). In M0.9: #65 (KTX2 can be larger on disk than PNG/JPEG), #66 (constraint pass cost).

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
  --frames` warm-up. #57 closed: `bench --package`, corpus constraints. Opened #65, #66. Next: #35.
