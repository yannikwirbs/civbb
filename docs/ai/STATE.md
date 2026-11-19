# State

What exists, by area. Read the section for the area you touch; update it when that area changes.

## Unity exporter and ground truth

- Exporter (#36) runs in the VRChat SDK host project `corpus/local/unity-export-vrc` (Unity 2022.3.22f1, SDK
  3.10.6; #37 closed) on all 11 local avatar packages (5 earlier Booth plus Cokoro, Little Blackbird, Ostgaroa,
  inuinu, KohzieBoi, Megumi); 13 exports in `corpus/local/unity-glb/` all import and build (table on #36). VRChat
  components are read by type and field name, no SDK reference (ADR 0042): descriptor playable layers (groups
  Base, Additive, Gesture, Action, FX; Action starts at 0; SDK default layers and proxy clips referenced, not
  copied), behaviours, viewpoint, visemes, blink, eye look, expressions, PhysBones and colliders, contacts, VRChat
  constraints. `X_avatar` carries muscle frames and the face. Unity constraints export the target's local
  transform as at-rest (`Object.Instantiate` resets it). `Batch.Import` survives the SDK's domain reload.
- Materials (ADR 0043): `X_toon_material` is the material as Unity saved it plus the shader GUID; the importer
  fingerprints GUID (lilToon variant table), name, markers, and maps lilToon, Poiyomi, UTS2, MToon, Standard,
  VRChat Mobile and unlit to Toon/PBR/Unlit (`gltf_import/unity_material.rs`). Toon materials name their light
  model (ADR 0050): lilToon, Poiyomi, UTS2 clamp light plus ambient; MToon, VRM and VRChat Mobile add the ambient. Approximations until Unity reference
  renders score them.
- Verified against Unity (`tools/unity-truth/Assets/Editor/Exported.cs` records into `corpus/local/unity-truth`,
  `crates/pipeline/tests/unity_export.rs` compares, local only): muscle, rotation, Euler, scale and blendshape
  curves of HAOLAN, Tessakuchan, Cokoro, Little Blackbird and Yumeka clips exact; 10 Unity constraints within
  0.0002 degrees; Yumeka's 4 VRChat constraints evaluated by the SDK's own editor runtime within 0.0001. The SDK
  runtime is the ground truth for #75: `tools/unity-truth/Assets/Editor/VrcConstraints.cs` (73 generated
  constraint-option cases, ADR 0046) and `VrcContacts.cs` (39 contact cases, ADR 0047) run in the SDK host project
  by reflection and write committed goldens (`crates/anim/tests/unity/vrc_constraints.json`,
  `crates/dynamics/tests/unity/vrc_contacts.json`), all replayed exactly. PhysBones too (ADR 0049):
  `VrcPhysBones.cs` (81 cases at 60 Hz, 18 at 90 and 30 Hz, `crates/dynamics/tests/unity/vrc_physbones*.json`);
  `tests/vrc_physbone.rs` replays them: version 1.1 within 0.5 mm in 55 of 62 cases at 60 Hz and at 30 Hz, the rest
  pinned as bounds (version 1.0 transients #83; 90 Hz output, collider velocity, polar corners #84).
- The muscle rig leaves the hips to the body pose (#52); real muscle frames had turned Yumeka over.
- Reference renders (#79, ADR 0048): the host project has lilToon 2.4.1 (embedded package), Poiyomi 7.3.050 and
  UTS2 2.0.9 (unpacked into `Assets/`, GUIDs kept), so all 13 exports render with their shaders.
  `Civbb.Exporter.Reference.Render` (`tools/unity-exporter/Editor/Reference.cs`, `tools/unity-batch.sh --graphics`)
  renders a prefab with a `civbb-client rig` hand-off's rig, light and states in play mode, the playable layers
  run as our runtime runs them (SDK proxy clips empty, humanoid only with muscle clips, hips at rest until #52,
  descriptor masks applied). Not yet part of an export or `civbb build --report` (`unity_parity`).
- VRChat components reach the compiler only through the Unity exporter (ADR 0037); the compiler never parses
  `.unitypackage` or Unity YAML.
- Exporter (ADR 0038): UPM package `tools/unity-exporter` in this repo (not a separate repo), excluded from the
  workspace; README has the batch commands for both host projects.
- Source extensions (ADR 0041, 0042, 0043): `X_*` payloads are the format types' serde JSON with glTF node indices
  (`gltf_import/source.rs`), except `X_avatar` (`{humanoid, viewpoint, muscle_frames, face}`) and per-material
  `X_toon_material`; `extras.active: false` marks inactive nodes; a `null` key tangent is a step.
  `civbb_corpus::gltf::to_source_glb` writes them (round-trip test).
- Curves (ADR 0039): the format stays Hermite; the exporter will bake weighted tangents, split equal key times and
  mirror position x, Euler and quaternion y/z. No Booth clip uses either (28,693 keys).
- Constraint fields (ADR 0040, 0046, format v10): `Property::Constraint { kind, field }` binds weight, active, source
  weights and freeze of the first constraint of a type on a node; per-island values and frozen placements live in
  `constraint::Scratch`; `Instance::write_constraints` after `write_nodes`. Solve In Local Space, Freeze To World
  (captures the animated placement, kept while inactive) and Rebake Offsets When Unfrozen (per-island offsets) run
  as the SDK does (ADR 0046), including its scale-offset power `n / total weight`.
- Unity ground truth: `tools/unity-truth` runs in batch mode and records JSON into crate test dirs (ADR 0013,
  `tools/unity-truth/README.md`). Muscle space and constraints (44 cases, batch play mode) are verified against it;
  animator goldens are still hand-derived (#54).

## Compiler (`civbb-pipeline`)

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
  - `atlas` (ADRs 0029, 0035): materials with the same model and UV slots share an atlas (16-texel gutters that
    wrap the texture, 16-texel aligned so mips 0 to 4 match, shelf packed within the platform cap, best-compressed
    PNG); whole-tile UV offsets are shifted back; animated materials,
    UV1, `_ST` tiling, tile-crossing triangles and textures used elsewhere are left alone. Materials then equal
    become one so `merge` joins their draws. AliciaSolid: textures 7 to 3, materials 12 to 10, draws 13 to 12.
  - `lod` (ADR 0031, format v8): up to three levels per mesh from meshoptimizer (`meshopt` 0.7), aiming at 1/2,
    1/4, 1/8 of the triangles with error caps 0.5/1.5/4 % of the mesh extent, kept only at <= 80 % of the level
    above. A level is an index list over a vertex prefix (vertices reordered), so it skins and applies shapes to
    fewer vertices; skin anchor and shape motion are simplification attributes, submesh borders locked
    (`LockBorder`). Non-manifold edges (3+ faces) may open. AliciaSolid 31.8k, 16.4k, 9.2k, 7.8k triangles;
    HAOLAN 146k to 21k. `Mesh::level(n)` gives a level as a mesh. Runtime switching is #63 (Phase 1).
  - `compress` (ADRs 0029, 0035): every texture to KTX2 with `basisu_c_sys` (C++ Basis Universal, bindgen needs
    libclang): the first of UASTC, XUASTC 90, XUASTC 75 no larger than the source blob, else the smallest (JPEG
    sources grow). Mips from `civbb_core::mips` (the renderer uses the same for RGBA8), single-threaded per texture
    (reproducible), at most 4 encoder threads (~3 GB per 4096-texel encode), sides capped at 4096 desktop / 2048
    Quest. Booth avatars 0.80 to 1.00x their source texture bytes; HAOLAN builds in ~90 s.
  - `collapse` (ADR 0028): plain bool toggle layers (two constant clips, zero-duration transitions) become one
    layer per group: a `Start` state, then a Write Defaults on direct tree of 1D trees weighted by `civbb/One`.
    Bloated 48 layers to 1, animator update 2.34 to 2.08 µs. Direct and 1D blend weights no longer allocate.

## Parity

- FLIP parity (ADR 0033): `civbb-client parity <reference> <test> --key <k> [--images <dir>]` renders both avatars
  with the reference's rig (640x640, front/left/back/right framed from the bind-pose bounds) in every state
  (`civbb_client::parity::states`: rest, then each parameter set in turn; the stage tests call it too), reads
  pixels back (`Renderer::read_last`) and scores each view with FLIP (`flip-rs`, checked against NVIDIA's
  `flip-evaluator`) over the avatar's pixels: mean, p99, traffic light (green mean <= 0.02 and p99 <= 0.2, red
  mean > 0.1 or p99 > 0.5). `civbb build --report --measure` compares the compiled package with the import before
  any stage and adds `parity` to the report plus `r.parity/*.png`. The rig frames the rest pose's world-space
  vertices of shown meshes (`parity::rest_bounds`, ADR 0036). Untextured corpus/local files score exactly 0;
  textured ones are green 13 of 16 since ADR 0035 (AliciaSolid 0.025, Avocado 0.026, AlphaBlendModeTest 0.045
  accepted yellow). lean and bloated are green in every state (`crates/pipeline/tests/parity.rs`).
- Unity as the reference (ADR 0048): `civbb build --unstaged` packages the import, `civbb-client rig <pkg>` writes
  the hand-off (`parity::Handoff`: rig, light, dt, parameters, states), the exporter renders `<state>-<view>.png`,
  and `civbb-client parity <dir> <pkg>` scores against them (`"reference": "unity"`). All 13 exports red at rest;
  since the toon light models (ADR 0050, #81) Inuinu (UTS2) 0.09 to 0.21 and Taro 0.11 to 0.21 best, Blackbird
  0.77 to 0.83 worst; per-family tables on #79, #81 and #82.

## Renderer and viewer

- Toon outlines (ADR 0034): inverted hulls, a second indirect draw per outlined toon submesh (`Shader::Outline`,
  front faces culled), width in metres times the width texture's G channel sampled in the vertex shader, unlit
  outline colour. Material records are 84 words.
- The viewer's title and `--frames` summary report draws and skinned meshes of the last frame. KTX2 textures are
  checked (header, level index, one 2D image) and transcoded to BC7 by the pure-Rust `basisu` crate (ETC1S, UASTC
  and XUASTC); the renderer uploads BC7 levels and device selection requires BC textures. RGBA8 textures get CPU
  mips (`civbb_core::mips`), no GPU blits. Fuzz target `ktx2_transcode`.
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

## Runtime (`core`, `anim`, `dynamics`, `client`)

- Animated materials (ADR 0025): `civbb_anim::material` maps Unity property names (`_Color`, `_EmissionColor`,
  `_MainTex_ST`, `_Glossiness`, MToon/lilToon/Poiyomi/UTS2 names) to native fields in Unity's units (sRGB colours,
  smoothness, V-up offsets); `Target::Material(mesh, field, component)` per renderer. `Model::animated_materials`
  and `Island::materials` give each island its own records for animated renderers; the viewer rewrites changed
  records through `Forward::add`'s upload pass.
- `civbb-core`: shared math, `jobs::islands` (one rayon job per avatar), per-system `Counters`. `civbb-client`:
  `Model`/`Island` (animator, constraints, IK stub, dynamics, world poses) and `frame::step` with the contact pass
  feeding next frame's parameters (ADR 0020); `civbb-client bench [--package <pkg> --key <key>]` prints JSON (#23,
  #57).
- Benchmark on the reference PC (release): 100 bloated avatars 3.23 ms/frame on 1 thread, 0.43 ms on 16 (7.5x);
  1000 avatars 32.6 ms to 3.7 ms (8.9x), measured before the corpus got constraints. Corpus constraints (#57): lean
  2 twist helpers, bloated 67 (twists and tail bones). The constraint pass places only the nodes constraints read
  and keeps its scratch per island (#66): lean 0.97 to 0.49 µs, bloated 12.7 to 9.45 µs per avatar on one worker.
- `civbb-anim`: curve binding, clip sampling, state machines, blend trees, Write Defaults, built-ins, behaviours,
  humanoid muscles and constraints (ADRs 0007 to 0011, 0014, 0018, 0019). `civbb-dynamics`: PhysBone chains as the SDK
  steps them (ADR 0049: fixed 1/60 s steps plus a partial output step, versions 1.0/1.1, simplified and advanced
  integration, immobile, limits, segment colliders) with grab/pose (ADR 0015's, unverified), and contacts with the
  cross-avatar grid broadphase (`contact::step`, ADR 0017): sphere, capsule and box shapes, face proximity, the
  component's node apart from the root transform, Proximity from a sender's second frame (ADR 0047). Format v13
  (`rebake` ADR 0046, contact shapes ADR 0047, PhysBone `version` ADR 0049, `Toon::light` ADR 0050).

## Test data

- Test avatars: `civbb-corpus` (`lean()`, `bloated()`, `generate`, `gltf::to_glb`), and locally `corpus/local/khronos/` (23 Khronos samples) `corpus/local/vrm/` (AliciaSolid 0.x, Seed-san and the twist sample 1.0) and `corpus/local/glb/` (Booth FBX conversions: HAOLAN, Taro, Tessakuchan, Yumeka, Little Blackbird, Ostgaroa, Cokoro), `corpus/local/unity-glb/` (13 exporter outputs, listed with their prefabs in its `exports.txt`) and `corpus/local/unity-truth/` (Unity recordings); see `corpus/README.md`. `civbb inspect <file>` prints what an avatar contains.
