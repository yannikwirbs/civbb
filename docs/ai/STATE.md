# State

What exists, by area. Read the section for the area you touch; update it when that area changes.

## Unity exporter and ground truth

- Exporter (#36) runs in the VRChat SDK host project `corpus/local/unity-export-vrc` (Unity 2022.3.22f1, SDK
  3.10.6; #37 closed) on all 11 local avatar packages (5 earlier Booth plus Cokoro, Little Blackbird, Ostgaroa,
  inuinu, KohzieBoi, Megumi); 13 exports in `corpus/local/unity-glb/` all import and build (table on #36). VRChat
  components are read by type and field name, no SDK reference (ADR 0042): descriptor playable layers (groups
  Base, Additive, Gesture, Action, FX; Action starts at 0; SDK default layers and proxy clips referenced, not
  copied; each layer's mask intersected with its group's descriptor mask, #80), behaviours, viewpoint, visemes, blink, eye look, expressions, PhysBones and colliders, contacts, VRChat
  constraints. The descriptor's 14 standard colliders (computed by the SDK's `UpdateAutoColliders`) export as
  `X_contacts` senders with VRChat's body-part tags (ADR 0061; global PhysBone colliders #92). Material-slot curves (`m_Materials.Array.data[n]`) export as `Property::MaterialSlot` keyed by glTF
  material index (ADR 0059, format v20); other object-reference curves are skipped and counted. State and blend
  tree child mirror flags and the mirror parameter export too (ADR 0060, v21). `X_avatar` carries muscle frames and the face. Unity constraints export the target's local
  transform as at-rest (`Object.Instantiate` resets it). `Batch.Import` survives the SDK's domain reload.
  Textures other than PNG/JPEG (PSD, TGA) are re-encoded as PNG from Unity's import (readable, uncompressed, then
  restored); normal maps in those formats are still skipped.
- Materials (ADR 0043): `X_toon_material` is the material as Unity saved it plus the shader GUID; the importer
  fingerprints GUID (lilToon variant table), name, markers, and maps lilToon, Poiyomi, UTS2, MToon, Standard,
  VRChat Mobile, TSF and unlit to Toon/PBR/Unlit (`gltf_import/unity_material.rs`). Toon materials name their light
  model (ADR 0050): lilToon, Poiyomi, UTS2 clamp light plus ambient (lilToon between `_LightMinLimit` and
  `_LightMaxLimit`, animatable, ADR 0054); MToon, VRM and VRChat Mobile add the ambient (Toon Standard clamps unless
  `_LimitBrightness` is off, and shades by its `_Ramp` with an albedo-coloured shadow, #105). Matcaps add, multiply
  or replace (MatCap Lit multiplies, ADR 0051) with a tint and a mask (lilToon `_MatCapColor`/`_MatCapBlendMask`,
  Poiyomi's add/multiply/replace weights and emission, both its matcaps, slots on their `_<Slot>UV`, #106, ADR
  0053); lilToon rim shade, the emission gradient's start and `_EmissionMainStrength`
  (ADRs 0052, 0053). Plain colours linearise as Unity does (`pow(x, 2.2)` above 1); `[HDR]` ones (lilToon, Poiyomi
  emission, UTS2 emissive) are taken as saved (ADR 0062). lilToon's shadow ramp in
  half-Lambert terms, shadow bands (strength, second shadow, main strength, border colour), rim ramp and masks, and
  emission main strength times the full albedo (ADR 0057), main texture tone correction (ADR 0058, format v19). Scores
  on #82 and #87. From each shader's source (#101): Mobile Toon Lit and MatCap Lit unshaded with no colour or
  emission; Toon Standard's emission times `_EmissionStrength` (its ramp is #105); lilToon's outline colour times
  `_OutlineTex` (`Toon::outline_tex`, format v23); UTS2's shade from `step - feather` to `step` in half-Lambert terms
  plus its second shade as lilToon's second band; Poiyomi without a `_ToonRamp` texture unshaded; TSF an unlit
  matcap (light clamped to exactly 1) times the texture by keyword.
- Verified against Unity (`tools/unity-truth/Assets/Editor/Exported.cs` records into `corpus/local/unity-truth`,
  `crates/pipeline/tests/unity_export.rs` compares, local only): muscle, rotation, Euler, scale and blendshape
  curves of HAOLAN, Tessakuchan, Cokoro, Little Blackbird and Yumeka clips exact; 10 Unity constraints within
  0.0002 degrees; Yumeka's 4 VRChat constraints evaluated by the SDK's own editor runtime within 0.0001. The SDK
  runtime is the ground truth for #75: `tools/unity-truth/Assets/Editor/VrcConstraints.cs` (73 generated
  constraint-option cases, ADR 0046) and `VrcContacts.cs` (39 contact cases, ADR 0047) run in the SDK host project
  by reflection and write committed goldens (`crates/anim/tests/unity/vrc_constraints.json`,
  `crates/dynamics/tests/unity/vrc_contacts.json`), all replayed exactly. Recorders that embed format JSON
  (`Constraints`, `VrcConstraints`, `VrcContacts`, and `Goldens` reading the case files) use the v36 shapes (ADR 0102;
  the two Vrc recorders' edits were not rerun: no SDK project here). PhysBones too (ADR 0049):
  `VrcPhysBones.cs` (88 cases at 60 Hz, 18 at 90 and 30 Hz, `crates/dynamics/tests/unity/vrc_physbones*.json`);
  `tests/vrc_physbone.rs` replays them: within 0.5 mm in 84 of 88 cases at 60 Hz (every version 1.0 case and falloff
  since ADR 0068, fitted to the SDK's per-bone state read by reflection; bones near rest snap onto it, ADR 0069; a collider push slows the velocity by the SDK's friction 0.25 and skips contact at a bone's start, ADR 0070; turns compose down the chain, ADR 0071), the rest
  pinned as bounds (spring 1 keeps f32 rounding; a bone fatter than its inside capsule flips sides on float noise,
  #90). All 18 cases at 90 Hz within 0.2 mm since ADR 0074 (output between steps, step timing; `collider_sway` 1.1 mm at
  one touching contact), 30 Hz exact. Inside capsules keep the tip within capsule radius minus
  bone radius, unclamped (negative puts it on the far side of the core). A bone's collision radius scales with its
  own node's world scale (an endpoint's with its leaf), recorded with scaled bones against unscaled colliders (#90).
- The body pose (#52, ADR 0063, format v22): the exporter writes `Animator.humanScale` and every humanoid clip's
  root-motion settings (`Clip::root`). `civbb_anim::body` has Unity's body pose as recorded (`BodyPose.cs`): centre of
  mass from Unity's mass table (within 0.6 mm; 1.3 mm without upper chest, whose mass then sits 5/6 up the
  chest-to-neck segment), orientation from the upper legs' and arms' midpoints and left-to-right
  vectors (0.33 degrees); `RootMotion` applies the bake/original/loop-pose/offset settings with root motion discarded
  (exact against `RootMotion.cs`'s 516 cases); `BodyRig::place` puts the hips when a layer wrote a body slot, which
  follows the mask's Root part; otherwise the hips stay at rest. Mass positions per bone fitted (ADR 0073: halfway, at
  the joint for hips, head, hands and feet, upper chest 7/12, left upper arm 1/8), anchored to Unity's rest centre of
  mass (`Humanoid::center_of_mass`, exported): hips within 1 mm of Unity's on recorded clips.
- Muscle frames for glTF/VRM humanoids (#53, ADR 0064): `civbb_anim::frames::derive` builds Unity's frames from the
  rest pose (post X along the bone, roll from a fixed avatar axis, pre = a fixed avatar-space turn of the post frame;
  `MuscleFrames.cs` variants, hand geometry included, within 0.05 degrees); the importer derives them when a humanoid
  has none. Derived from the skeleton each export's `Avatar` was built from (`BuildSkeleton.cs`; its thumbs and
  fingers differ from the prefab's rest), every export's frames match within 0.15 degrees, Blackbird's shoulders 1.75
  (ADR 0072).
- Reference renders (#79, ADR 0048): the host project has lilToon 2.4.1 (embedded package), Poiyomi 7.3.050 and
  UTS2 2.0.9 (unpacked into `Assets/`, GUIDs kept), so all 13 exports render with their shaders.
  `Civbb.Exporter.Reference.Render` (`tools/unity-exporter/Editor/Reference.cs`, `tools/unity-batch.sh --graphics`)
  renders a prefab with a `civbb-client rig` hand-off's rig, light and states in play mode, the playable layers
  run as our runtime runs them (SDK proxy clips empty, humanoid only with muscle clips, hips at rest when no playing clip has body curves (ADR 0063),
  descriptor masks applied, shader time pinned to 0, ADR 0062). It logs every state's layers (mixer and clip weights) and muscles (`CIVBB <state>`), and runs the SDK's
  parameter drivers and layer controls through their client hooks (ADR 0056). Not yet part of an
  export or `civbb build --report` (`unity_parity`): #98, Phase 3 (ADR 0075).
- No NDMF / Modular Avatar / VRCFury build before export (ADR 0075, #97, Phase 3): none of the 13 local packages
  uses them (survey on #36).
- Unity CPU baselines (#38, ADR 0081): `Civbb.Exporter.Baseline.Measure` (`tools/unity-exporter/Editor/Baseline.cs`,
  runner `corpus/unity_baseline.py`, `--graphics`) steps 0, 1 and 40 copies in play mode as our bench does, job
  workers off and the Editor confined to `measure`'s core; `PlayerLoop` less the 0-copy frame per copy is Unity's
  CPU per avatar, the director markers its animator. Output `corpus/local/unity-baseline/<unity|aao>/<name>.json`,
  read by `corpus/scorecard.py`'s second table. The SDK's PhysBones, contacts and VRChat constraints are managed
  jobs in Unity (no `[BurstCompile]` in `VRC.Dynamics.dll`), so only the animator is compared like for like.
  AvatarOptimizer 1.9.23 + NDMF 1.15.1 and Av3Emulator 3.5.0 live only in the gitignored host copy
  `corpus/local/unity-export-aao` (`-civbbOptimize`, `-civbbEmulator`; the emulator check is #104).
- VRChat components reach the compiler only through the Unity exporter (ADR 0037); the compiler never parses
  `.unitypackage` or Unity YAML.
- Exporter (ADR 0038): UPM package `tools/unity-exporter` in this repo (not a separate repo), excluded from the
  workspace; README has the batch commands for both host projects.
- Source extensions (ADR 0041, 0042, 0043): `X_*` payloads are the format types' serde JSON with glTF node indices
  (`gltf_import/source.rs`), except `X_avatar` (`{humanoid, viewpoint, muscle_frames, face}`, bones by Unity `HumanBodyBones` name) and per-material
  `X_toon_material`; `extras.active: false` marks inactive nodes; a `null` key tangent is a step.
  Hostile and mapping tests of `X_toon_material` in `tests/unity_material.rs`. Fuzz seeds: `lean-source.glb`
  (native since #124) and `toon-materials.gltf` (every shader family, #71 closed; the latter is
  `pipeline/tests/fixtures/toon-materials.gltf`, also pinned).
- Native extensions (ADRs 0086, 0101, #124): root `CIVBB_avatar`, `CIVBB_state`, `CIVBB_behaviour`
  (`animator`, `clips`), `CIVBB_constraints` (`constraints`), `CIVBB_dynamics`, `CIVBB_contacts`; `CIVBB_node`
  (`active`, `first_person`) on nodes; `CIVBB_materials_toon` (toon model plus `order`) on materials. Each the
  format type's serde JSON plus `"version": 1`, with glTF indices (mesh references are the node carrying the mesh,
  textures glTF textures), mapped by `gltf_import/native.rs`. `civbb_corpus::gltf::to_source_glb` writes them;
  `corpus_round_trips_through_native_extensions` compares everything but textures. VRM, `X_*` and `CIVBB_*` are
  mutually exclusive. Unknown fields fail everywhere (`CIVBB_materials_toon` takes `order` out, then parses `Toon`;
  serde `flatten` would accept unknowns).
- JSON shapes (ADRs 0086 point 3, 0102, format v36): unit enums snake_case strings, data enums `{"type": ...}`
  objects, named fields instead of newtypes and tuples, `None`s omitted, `deny_unknown_fields`; `HumanBone` stays
  camelCase (VRM names); a `null` key tangent is a step. Package metadata (`package.rs`, `cost.rs`) untouched.
- Schemas and spec (ADR 0086, #124): `civbb_pipeline::gltf_import::schemas()` (schemars derives on the format types,
  plus `version`, root `additionalProperties: false`) -> `docs/spec/schema/*.schema.json`; `tests/schema.rs` fails
  when stale (`cargo xtask schema` rewrites) and validates the corpus's `CIVBB_*` JSON (`jsonschema` dev-dep).
  Prose per extension in `docs/spec/` (README: common rules and the caps table).
- Versions and migrations (ADR 0087, #125): `civbb_format::migrate` (`STEPS`: one `Step {extension, from, run}` per
  breaking change on the JSON map; newest version = 1 + the extension's steps; `migrate` checks `version`, runs
  the steps, returns the map without it; `MigrateError` names the extension). The importer's `parse` and the toon
  path (migrate, then take `order`) go through it; the schemas' `version.maximum` is `newest`. Frozen fixtures
  `format/tests/fixtures/<ext>/v<N>.json` plus `host.glb` (lean corpus avatar with actions, a muscle frame and a
  platform entry, `CIVBB_*` stripped; the dynamics fixture has one hand-added chain). `pipeline/tests/
  native_fixtures.rs` imports every version of each with the newest of the others (`CIVBB_node` on the first mesh
  node, toon on every material); `format/tests/migrate.rs` runs a test-only v1 -> v2 -> v3 chain and checks one
  fixture per version. Never edit a fixture; a new version adds a step and `v<N+1>.json`.
- Native toon model (ADRs 0090, 0100, #123, format v35): `Toon` = base colour, shade colour/texture/shift/toony, plus
  optional features `light_clamp` (none: additive ambient), `normal`, `emission`, `matcaps` (list, at most
  `material::MATCAPS` = 2), `rim` (with its ramp), `rim_shade`, `shade` (second band, border), `color_adjust`,
  `outline`. Family tables set a feature where the source has it on; curves on an absent feature's fields do
  nothing. `Material::order` is the offset within the alpha pass, `Material::queue()` the sort key.
  `tests/material_records.rs` pins `civbb_render::material::pack` output (golden for generated content;
  `CIVBB_RECORDS=<parity dir>` ignored test dumps compiled exports and local VRMs to diff two runs).
- Unity dialect 1 (ADR 0085, #114): the `X_*` shapes as format v25 wrote them are now an import dialect, not the
  native format. Frozen fixtures `pipeline/tests/fixtures/unity-dialect-1*.gltf` (generated, every variant of every
  `X_*` part but `X_toon_material`; never regenerate) import to `tests/golden/unity-dialect-1*.json` (re-bless with
  `CIVBB_BLESS=1` when the native model changes, and read the diff). `asset.extras.civbb_unity_dialect`: absent is
  1, newer refused (`UNITY_DIALECT` in `source.rs`). The native model's redesign (ADRs 0085 to 0090) is M1.1:
  #115 to #125. Since v36 the dialect's old serde shapes are rewritten as JSON before parsing
  (`gltf_import/unity_shapes.rs`, ADR 0102); extend it when a native shape changes again.
- Curves (ADR 0039): the format stays Hermite; the exporter will bake weighted tangents, split equal key times and
  mirror position x, Euler and quaternion y/z. No Booth clip uses either (28,693 keys).
- Constraint fields (ADR 0040, 0046, format v10): `Property::Constraint { kind, field }` binds weight, active, source
  weights and freeze of the first constraint of a type on a node; per-island values and frozen placements live in
  `constraint::Scratch`; `Instance::write_constraints` after `write_nodes`. Solve In Local Space, Freeze To World
  (captures the animated placement, kept while inactive) and Rebake Offsets When Unfrozen (per-island offsets) run
  as the SDK does (ADR 0046), including its scale-offset power `n / total weight`.
- Unity ground truth: `tools/unity-truth` runs in batch mode and records JSON into crate test dirs (ADR 0013,
  `tools/unity-truth/README.md`). Muscle space and constraints (44 cases, batch play mode) are verified against it;
  animator goldens are recorded too (`Goldens.cs`, ADR 0065; transform masks and interruption sources recorded, ADR 0066;
  material curves on renderers with differing materials, ADR 0067).

## Compiler (`civbb-pipeline`)

- Runtime packages (ADR 0087, #113): `Package::decode_verified` refuses a `Header::format_version` other than
  `FORMAT_VERSION` (`PackageError::FormatVersion`); after a bump, rebuild packages in corpus/local with `civbb build`.

- glTF input (ADR 0076): `KHR_mesh_quantization` accepted (integer attributes, morph deltas and animation outputs
  through the normalising accessor reader); required Draco or meshopt compression refused with an export hint.
- State table (ADR 0088, 0095, 0096, 0098, #115 to #118, format v29): `civbb_format::state` (`StateTable`, `Entry {name, kind, default, range,
  rest, sync, saved, external}`, `EntryIndex`), `Document::state`. Animator conditions, blend inputs, speed/offset/
  time/mirror inputs, drivers, menu controls (`Option<EntryIndex>`, puppet axes `Vec<Option<_>>`), contact
  receivers and chain outputs (`Chain::outputs`, one per grabbed/posed/angle/stretch/squish; dialect 1's prefix
  resolves `<prefix>_IsGrabbed` etc.) reference entries; the animator has no parameter table and `Expressions` only the menu. The Unity
  importer lowers dialect 1 in `gltf_import/unity_state.rs` (animator parameters in order, then expression-only ones;
  expression default starts, animator default is `rest`; unknown names `None`); `civbb_corpus::gltf` raises entries
  back to dialect 1. Validation checks every reference and unique names. Sync encoding (ADR 0098, #118, format v29):
  `Entry::range` (`Range::Int {min, max}`, `Range::Float {min, max, bits 2..=16}`), `Entry::sync_bits` (bool 1,
  trigger 0 and never synced, ranged int ceil(log2(span + 1)), ranged float `bits`, unranged 32), `encode`/`decode`
  (floats on `2^bits - 2` steps, midpoint exact); one budget `Caps::synced_bits` 1,024. Lowered expression
  parameters get Int 0..255 / Float -1..1 8 bits, VRM weights Float 0..1 8 bits: corpus totals unchanged. Platform values (ADR 0096): `state::Platform`, entries named `platform/...`
  (VRChat's names via `vrchat_name`, renamed by the importer, raised back by the corpus writer and the parity
  hand-off), never synced/saved/external and written by no content (validated; the importer drops driver writes).
- Curve targets (ADRs 0089, 0099, #119, #120, format v31): `animator::Target` is `Node {node, property}`
  (`Translation`, `Rotation`, `EulerRotation {axis, order}` in radians, `EulerOrder::Zxy` only, `Scale`, `Active`),
  `Mesh {mesh, property}` (`Visible`, `Weight(shape)` 0..1, `Material {field: MaterialField, component, srgb}`,
  `MaterialSlot`), `Humanoid(Muscle {bone, axis} | Body)`, `Entry`, `Constraint {constraint, field}`; avatar mask
  `transforms` are node indices. The Unity importer converts units (degrees, 0..100, smoothness to roughness, V-up
  offsets to stored ones per clip) and maps material names per shader family (`unity_material::curve_field`,
  `Importer::families`); components of an animated material field that no clip animates are baked into the
  renderer's materials (`unity_clip/material.rs`, ADR 0099). The corpus writer raises units back and refuses
  material curves. Dialect 1's path bindings are resolved in `gltf_import/unity_clip.rs` (path to node,
  the node's mesh, shape by name, entry by name or VRChat built-in name, first constraint of the type on the node;
  what resolves to nothing is dropped with one `X_animator: N curves dropped: <reason>` warning per reason); the
  corpus writer raises targets back to paths. Prune, merge and atlas remap targets through `renumber::Renumber`
  (curves on removed elements go). Merge no longer needs its path rules (a mesh's path its own, curves naming shapes
  of another mesh). `civbb_anim::bind` checks only what indices cannot say (float entries, muscle frames, material
  fields, slot keys, constraint sources).
- Actions and unwritten rules (ADR 0085 point 4, #121, format v32): `State::unwritten` is `Unwritten::Hold | Rest`
  (Write Defaults off / on; a state without a motion still writes nothing), `State::actions` are `Action::Entries
  {owner_only, ops: Vec<EntryOp>}` (set, add, random, copy), `PoseSource(Vec<(PosePart, PoseSource)>)`,
  `LayerWeight`, `GroupWeight`, `Locomotion`; the layer mask is `animator::Mask`. Dialect 1's `write_defaults` and
  `behaviours` are rewritten by `unity_state::lower_states` (forked `XBehaviour`) before parsing; the corpus writer
  raises them back (`raise_states`). Runtime: `civbb_anim::action` (was `behaviour`), `Instance::pose_sources`.
- Document root (ADR 0085 points 5 and 6, #122, format v34): `civbb_format::Document` holds the shared parts
  (nodes, meshes, materials, textures, state, animator, clips, dynamics, contacts, constraints) and `role: Role`;
  `Role::Avatar(Avatar {humanoid, viewpoint, face, expressions, meta})` (viewpoint left `Humanoid`). Access with
  `doc.avatar()` / `avatar_mut()` (`Option`, so Phase 2's `Role::World` adds no panics); `Package::with_document`
  sets the content kind from the role, `Package::document` decodes. Avatar-role checks in `validate/avatar.rs`.
  `HumanBone` is VRM 1.0's set (thumb metacarpal, proximal, distal), serialised camelCase (`leftThumbMetacarpal`);
  Unity names map by `civbb_anim::muscle::unity_bone(_name)` (Unity's thumb proximal is the metacarpal): X_avatar
  (forked `XMuscleFrame`), VRM 0.x, the corpus writer and the Unity recordings in anim tests. VRM 1.0 parses as is.
- Importer layout (health check): `gltf_import.rs` (entry, parsing, URI resolver, buffers, hierarchy) with
  `gltf_import/{material,mesh,animation,source,unity_clip,unity_material,unity_state}.rs` and `vrm.rs` (entry, humanoid, node constraints,
  first person) with `vrm/{springs,mtoon,expressions,meta}.rs`. vrm keeps its own `qmul`/`rotate`/`from_to` (core's
  round differently); the rest of its math is `civbb_core::math`. Validation (`civbb_format::validate`) checks one
  avatar part per method, animator checks in `validate/animator.rs`; PhysBone collider radius and height must be
  finite.
- Per-avatar report (ADR 0032): `civbb build <src> --key <k> --report r.json [--measure]` writes `r.json` and `r.md`:
  stats after import and after each stage (`civbb_pipeline::compile` runs them), package size, and with
  `--measure` the output of `civbb-client measure` (run as a process from next to `civbb`; release only): `bench`
  on one worker and the viewer's frames on an offscreen target (`Renderer::headless`), GPU null without a device.
  Golden for lean counts: `crates/pipeline/tests/golden/report-lean.json` (`CIVBB_BLESS=1` rewrites it). All 31
  files in corpus/local build with measured reports (summary on #34): e.g. Yumeka draws 23 to 7, 563 shapes to 0,
  7.6 µs CPU and 17.9 µs GPU per avatar; HAOLAN textures 19.0 to 3.3 MB, 33.5 µs GPU. `measure` pins its one
  worker to the fastest core class's highest logical CPU (Windows only, ADR 0080, `client/src/affinity.rs`) and
  reports it as `cpu.pinned` (null: unpinned); two pinned runs agree within 10 % (`#[ignore]`d test in
  `client/tests/measure.rs`, Yumeka 185/180 µs on CPU 11).
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
    HAOLAN 146k to 21k. `Mesh::level(n)` gives a level as a mesh. Runtime switching is #63 (Phase 1). Each level's target is capped at
    the level above's indices (a meshoptimizer precondition); the chain ends at a repeated empty level or a
    non-finite error (hostile input, `tests/lod.rs`). The `compile` fuzz target runs import and every stage but
    `compress` and requires a valid result.
  - `compress` (ADRs 0029, 0035): every texture to KTX2 with `basisu_c_sys` (C++ Basis Universal, bindgen needs
    libclang): the first of UASTC, XUASTC 90, XUASTC 75 no larger than the source blob, else the smallest (JPEG
    sources grow). Mips from `civbb_core::mips` (the renderer uses the same for RGBA8), single-threaded per texture
    (reproducible), at most 4 encoder threads (~3 GB per 4096-texel encode), sides capped at 4096 desktop / 2048
    Quest. Booth avatars 0.80 to 1.00x their source texture bytes; HAOLAN builds in ~90 s.
  - Build log (#128): `import seconds` (stderr) and `stage seconds` per stage. `compress` is nearly all of a build
    (Yumeka 271.25 of 272 s; every other stage under 0.1 s). `--texture-cache <dir>` (ADR 0097,
    `compress_cached`) keys each encoded texture by source hash, format, colour space, cap, the source text of
    `texture.rs` and `core/src/mips.rs` and `texture::ENCODING_DEPENDENCIES` (checked against Cargo.lock);
    packages are byte-identical with or without it. `tools/parity.sh` uses `corpus/local/texture-cache`.
  - `collapse` (ADR 0028): plain bool toggle layers (two constant clips, zero-duration transitions) become one
    layer per group: a `Start` state, then a Write Defaults on direct tree of 1D trees weighted by `civbb/One`.
    Bloated 48 layers to 1, animator update 2.34 to 2.08 µs. Direct and 1D blend weights no longer allocate.

## Parity

- FLIP parity (ADR 0033): `civbb-client parity <reference> <test> --key <k> [--images <dir>]` renders both avatars
  with the reference's rig (640x640, front/left/back/right framed from the bind-pose bounds) in every state
  (`civbb_client::parity::states`: rest, then each state entry set away from its start in turn, 60 frames each so
  states settle, ADR 0055; the stage tests call it too), reads
  pixels back (`Renderer::read_last`) and scores each view with FLIP (`flip-rs`, checked against NVIDIA's
  `flip-evaluator`) over the avatar's pixels: mean, p99, traffic light (green mean <= 0.02 and p99 <= 0.2, red
  mean > 0.1 or p99 > 0.5). `civbb build --report --measure` compares the compiled package with the import before
  any stage and adds `parity` to the report plus `r.parity/*.png`. The rig frames the rest pose's world-space
  vertices of shown meshes (`parity::rest_bounds`, ADR 0036). Untextured corpus/local files score exactly 0;
  textured ones are green 13 of 16 since ADR 0035 (AliciaSolid 0.025, Avocado 0.026, AlphaBlendModeTest 0.045
  accepted yellow). lean and bloated are green in every state (`crates/pipeline/tests/parity.rs`).
- Unity as the reference (ADR 0048): `civbb build --unstaged` packages the import, `civbb-client rig <pkg>` writes
  the hand-off (`parity::Handoff`: rig, light, dt, parameters, states), the exporter renders `<state>-<view>.png`,
  and `civbb-client parity <dir> <pkg>` scores against them (`"reference": "unity"`), stepping through the stored
  hand-off's states, the ones Unity rendered (ADR 0095). After #82, #86/#85, #87 and
  #88 (2026-11-20) rest means (front) run 0.06 (Inuinu) to 0.23 (Cokoro Quest); worst toggled states 0.12 to 0.37
  (Tessakuchan). Bone radii in world units (#90) took HAOLAN to 0.054 front, worst 0.072; `[HDR]` colours as saved and the reference at
  shader time 0 (#91) Cokoro to 0.109, Ostgaroa to 0.134. Fitted mass positions (ADR 0073, run `m96`) undid the
  #52 rest regression: rest means (front) Cokoro 0.110, Tessakuchan 0.102, Yumeka 0.139, Inuinu 0.065. Remaining:
  silhouette edges. Tables on #82, #86, #85, #87.
  `corpus/local/unity-ref/all.sh <name>` rescores all, re-renders those whose hand-off changed, rescores.
  Those numbers score the uncompiled import. Since the health check, parity skips parameters named
  `civbb/...` (`COMPILER_PARAMETER_PREFIX`), so compiled packages score against Unity too; `python
  corpus/scorecard.py` (committed) gives per export the CPU and GPU µs before and after compiling and the compiled
  package's Unity FLIP (ADR 0078, M0.11), measuring pinned to a P-core. Run 2026-11-24 (table in the health check
  report and on #101): compiled against Unity rest means 0.057 (HAOLAN_PC) to 0.194 (Cokoro_Quest), worst states
  up to 0.487, none green; GPU per avatar down where draws or shapes go (Yumeka 59.9 to 33.6 µs), CPU unchanged
  (Yumeka 191 to 186 µs). Rerun with ADR 0080's pinning: Blackbird 22.8 to 21.4 µs (#102 closed, not reproduced);
  its second table puts the compiled CPU next to Unity's and Unity + AvatarOptimizer's at 1 and 40 copies (ADR 0082:
  animator 3.0 to 42 times cheaper compiled).
- Against Unity (#101, ADR 0083): each view's score also splits into `interior` (covered in both images, more than
  1 px from either edge) and `silhouette` (`ViewScore`, report keys added, version kept), and a Unity reference is
  lit with `UNITY_THRESHOLDS` (green mean <= 0.06 and p99 <= 0.25, red mean > 0.12 or p99 > 0.9; the report's
  `thresholds` say which). Rest means after #101, #105, #106, #108: 0.027 (Inuinu) to 0.107 (Yumeka, worst view
  0.1197); green Inuinu, Blackbird, HAOLAN_Quest, Blackbird_Quest, Megumi, the other 8 yellow, none red (table in
  ADR 0084, the gate passed). Pose states (#109), Blackbird's wings (#107) and Cokoro's material toggles (#110) are
  red after rest. Bone-by-bone rest comparison: `Reference.Render -civbbBones <file>` (Unity, our handedness).

## Renderer and viewer

- Toon outlines (ADR 0034): inverted hulls, a second indirect draw per outlined toon submesh (`Shader::Outline`,
  front faces culled), width in metres times the width texture's G channel sampled in the vertex shader, unlit
  outline colour times the outline texture. Material records are 164 words (toon: light model and bounds, matcap blend, tint and mask, rim
  shade, shadow bands, rim ramp, tone correction; ADRs 0050 to 0054, 0057, 0058). Matcaps add, multiply or replace
  (Poiyomi: towards the matcap lit as the colour is), each with an unlit emission weight; a toon has up to two
  (`Toon::matcaps`, ADR 0090). A mesh without UV1 reads UV0 there, as Unity does (`scene.rs`).
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
- Culling (ADR 0077, #99): `civbb_client::cull` bounds each mesh instance by per-joint bind-space spheres moved by
  the frame's skinning matrices (padded for blendshapes) and drops instances outside the view frustum before
  skinning and drawing. The GPU culling pass with hierarchical Z is Phase 2 (#59).
- Viewer (ADR 0024): `civbb-client view [<pkg>... --key <key> | --avatar lean|bloated] [--copies N] [--frames F]
  [--warmup W] [--lod L]`; N copies of each package; packages load on the I/O pool while frames run and spawn when
  done (#134, ADR 0104), the first one frames the camera and the grid; a failed load ends the viewer with its error.
  The title shows triangles per copy (all packages); `--frames` summarises the F frames after a 60-frame warm-up that
  starts once every load is done (ADR 0032).
  glTF/VRM go through `civbb build` first (client may not depend on pipeline). AliciaSolid renders textured and
  toon-shaded. Debug builds are ~30x slower on the CPU (validation).
- Pipelined frames (#133, ADR 0103 point 5): the schedule's `Extract` step (`civbb_client::extract`,
  `Runtime::extract_to_snapshot`) copies each island's origin, skinning matrices (`None` for inactive meshes), shape
  weights, animated materials and submesh slots plus the `Camera` resource into the `Snapshot` resource. The viewer
  swaps it with a free one and hands it to `render_thread::RenderThread` (owns `Renderer` + `Gpu`; culling, material
  packing, draw lists and recording happen there via `gpu::draw`); two snapshots circulate, so simulation is one
  frame ahead at most. Window and event loop stay on main; resizes go through the thread's channel; a render-thread
  panic is re-raised on main. Headless `measure` runs pipelined too: its "viewer CPU per frame" is now the busier
  thread per frame and "systems" includes extract. Parity draws synchronously (extract, then `draw`, same thread).
  Release, reference PC, 100 bloated, vsync: serial before 2.13 ms CPU (systems 1.19 ms); pipelined 1.57 to 1.89 ms
  (simulation 1.57 to 1.82 ms with extract ~0.4 ms of it, render thread 0.69 to 0.80 ms), GPU ~0.7 ms. Since #137
  the runtime's extract is one job per island on the `Workers` pool (`Snapshot::extract_on`; the plain `extract`
  parity uses stays serial, both through `IslandSnapshot::fill`; `System::Extract` counts once per island): extract
  wall 0.35 to 0.12 ms (0.40 ms summed CPU; the gap to ~0.03 ms is #140), simulation 1.69 to 1.52 ms.
- GPU resources by island (#134, ADR 0104, `civbb_client::gpu`): each `IslandSnapshot` carries its key (entity bits)
  and package handle; `Gpu::sync` (start of `draw`) uploads a package on first sight (meshes, textures, shared
  materials, bounds), gives a new key a freed instance of its package or a new one (skinned meshes, own material
  records), and returns instances of keys missing from the snapshot to their package's free list. When the scene
  cannot hold what is new it waits idle, destroys everything and rebuilds with max(2x old, need); the first snapshot
  sizes it exactly (parity allocates as before). Package GPU data is never freed short of a rebuild; no range
  allocator or VRAM budget yet.

## Runtime (`core`, `anim`, `dynamics`, `client`)

- Animated materials (ADRs 0025, 0099): `civbb_anim::material` reads and writes a `MaterialField` as stored;
  `Target::Material(mesh, field, component, srgb)` per renderer, `srgb` slots decoded with `to_linear` when applied. `Model::animated_materials`
  and `Island::materials` give each island its own records for animated renderers; the viewer rewrites changed
  records through `Forward::add`'s upload pass. As recorded (ADR 0067): defaults from the renderer's material 0 (0
  when it lacks the field), components no curve animates are 0 (baked by the importer, ADR 0099), animated colours
  converted from gamma even when `[HDR]` (an `[HDR]` default still round-trips, #93).
- `civbb-core`: shared math, `jobs` (ADR 0105, #140: an own fork-join `Pool`; `broadcast(job)` wakes every pool
  thread with one `notify_all`, runs the job on the caller too and waits only for threads that took it in time; pool
  threads spin 200 µs for the next job before sleeping; `islands(pool, ..)` broadcasts one closure that takes islands
  from a shared iterator; `pool(n, on_spawn)` has n - 1 threads; `default_workers()` cores minus 2; no bevy_tasks or
  rayon in core), per-system `Counters` (a resource; avatar systems, contacts and, in the runtime, extract per island),
  `profile` (#136: `zone(name)` guards, `start()` in the client's `main`, `frame_mark()` after each `Runtime::step`;
  Tracy only with the off-by-default `profile` feature, `cargo build -p civbb-client --release --features profile`;
  zones per `Counters::time` system, per island job (`island`), the `avatars` system, the render thread's `render`,
  the renderer's `wait` and `present`, and each render graph pass by name; the gate never builds the feature),
  `schedule::frame()` (ADR 0103: the `Frame` schedule, `Step` sets in design doc §5's order, chained; `Dt`).
  `civbb-client`: `Model`/`Island` (a component; animator, constraints, IK, dynamics, world poses),
  `frame::step` (plain: islands then the contact pass feeding next frame's parameters, ADR 0020; parity and the
  collapse tests use it) and `frame::Runtime` (bevy_ecs world, one entity per island, the island and
  contact systems in `Avatars`/`CrossAvatar`, `Permits` and `Workers` resources; the bench and viewer use it, #131).
  Packages (#134, ADR 0104): `package::Package` (validated `Document`, `Arc<Model>`, decoded textures kept in RAM,
  process-unique `id`) behind `Handle = Arc<Package>`; `package::spawn(load)` runs a load on bevy's `IoTaskPool`
  (cores / 4 threads, not pinned), `package::read` verifies and decodes a file. `Runtime::spawn_package(&handle, id,
  local, origin)` adds `Spawned(seq)` + `Shows(handle)` + the island; `Runtime::despawn(e)`. Despawn reorders the
  ECS table: extract sorts by `Spawned`; the contact pass needs no order (receiver value = max over senders).
  `frame::workers()` is the shared pool (pool threads kept to the fastest core class, `affinity::keep_to_performance_cores`);
  `Runtime::with_workers` and `frame::step_on` take another. The executor is single-threaded (one system per set so
  far); bevy's `ComputeTaskPool` is a global `OnceLock` and unused. bevy_ecs has
  default features off: the derive macros need it as a direct dependency of each crate that derives.
  `civbb-client bench [--package <pkg> --key <key>]` prints JSON (#23, #57).
- Desktop player (#141, #142; ADRs 0106, 0107): `player.rs`, `Runtime::add_player` (`Input`, `Player`, `Camera`
  resources; a system in `PosesIn` moves the player's island through the `WorldMesh` resource, the viewer's first
  copy). WASD, Shift runs (walk 2, run 4, strafe 2 m/s), Space jumps (3 m/s, gravity 9.81), mouse look, V
  cycles orbit/first/third person (own head hidden in first person), C/Z toggle crouch/prone (`Upright` 0.55/0.25,
  half/quarter speed), Shift+F1..F8 gestures, AFK while unfocused; it sets the island's platform values each frame
  (`Island::set_platform`).
- Character controller (#144, ADR 0110; Jolt deferred to Phase 2 world content): `controller.rs`, `WorldMesh`
  (static world triangles, brute force; `plane()` is `add_player`'s default, `from_document` takes rigid meshes)
  and `Body::step` (vertical capsule r 0.25, h 1.6; overlap resolution in substeps of r/2; walkable ≤ 45° pushes
  straight up; an edge or corner contact below the feet's sphere and within the 0.3 m step of the frame's starting
  feet height is a step edge, lifted onto; taller risers up to 0.3 m by up-across-down; snap down a step when walking
  off, else fall; `Grounded` = walkable within 1 cm). Test world `civbb_corpus::world()` (CC0 rigid meshes, avatar
  role until a world role exists): `civbb-client view --world` spawns it at the origin as an island; the player
  starts at the origin facing +Z towards it, so the orbit camera (at +Z) has it behind.
- Built-in locomotion (#143, #152; ADRs 0108, 0109): `civbb_anim::defaults::apply(&mut Document)` merges generated
  muscle-space clips into a humanoid with muscle frames: fills empty `proxy_*` clips by name (`defaults::proxy`;
  emotes stay empty, #151) and adds `civbb/locomotion` (Base: standing/crouching/prone 2D freeform trees on
  `VelocityX`/`VelocityZ`, falling on `Grounded`, postures at `Upright` 0.7/0.4), `civbb/hand_left`/`_right`
  (Gesture: any-state per gesture value, fist blended by its weight, masked per hand) and `civbb/action` (AFK,
  sitting; raises the Action group) to empty groups, plus the platform entries they read. `defaults::Poser`
  grounds each key: the pose's lowest human bone at the rest pose's lowest. Muscle signs on our rig: front-back
  of spine, chest, neck, head and arms positive backwards, nods positive up, upper leg in-out positive out, upper
  leg front-back 0.6 standing (more negative is forward), stretch 1 straight, finger stretch +1 open. Only the
  viewer's package load applies it (`main.rs`); parity, `measure` and bench show packages as built. All 13 corpus
  exports validate merged; KohzieBoi, Megumi, Ostgaroa and Taro run on the generated layers.
- Full-body IK, 3-point (#145, ADR 0111): `civbb_anim::ik::Solver` (per `Model`, `None` unless hips, spine, head,
  arms and legs are mapped) runs in `Island::step` between constraints and dynamics on `Island::targets`
  (`ik::Targets`: head and hand `Goal`s in avatar space, `look` = view pitch; the default changes nothing, so parity
  is untouched). Head goal: hips follow its height, the spine bends towards it (2 passes), hips take the rest, legs
  keep their animated feet. Limbs: two-bone, bend towards the animated middle joint, else a rest pole (elbows down
  and back, knees forward); unreachable stretches straight. Pose sources `Animation` (tracking control) skip a part.
  The desktop player sets `look` (chest 0.2, neck 0.3, head the rest). Bones are placed by walking parents up to an
  anchor (no pose cache). Bench (`reach` in `bench.rs` sets all three goals and the pitch on every island):
  IK 4.5 µs per avatar on 1 worker (lean and bloated alike), bloated 100 avatars 6.96 to 7.74 ms/frame on 1 worker,
  unchanged at 13 workers (0.81 ms).
- Benchmark on the reference PC (release, bloated, #132): 100 avatars 7.03 ms/frame on 1 worker, 0.756 ms on 16
  (9.3x); 1000 avatars 71.6 ms to 7.06 ms (10.1x). The bench makes one pool per worker count and drives frames from
  a fresh thread (`measure` pins it, 1 worker, ADR 0080). Corpus constraints (#57): lean
  2 twist helpers, bloated 67 (twists and tail bones). The constraint pass places only the nodes constraints read
  and keeps its scratch per island (#66): lean 0.97 to 0.49 µs, bloated 12.7 to 9.45 µs per avatar on one worker.
  `Island::step` allocates nothing after warm-up (#135, `tests/alloc.rs`, a counting global allocator): the
  dynamics solver keeps its world poses, collider placements and grab path in `State`'s scratch. Bloated 100
  avatars, 1 worker: 7.21 to 6.96 ms/frame. Whole frames too since #139 (`frame::step_on` and `Runtime::step`
  without extract, one worker): `contact::step_with` keeps its buffers in a `contact::Scratch` (broadphase grid a
  sorted (cell, sender) list instead of a `HashMap`; receivers placed in the second loop; `touching` lists reused),
  frame.rs keeps the participant and `&mut Island` buffers (`frame::recycle`, the standard in-place collect
  reusing a `Vec`'s allocation for borrows; a thread-local scratch for plain `step`, `Local`s in the systems), and
  `jobs::islands` allocates nothing with threads either (#140; `tests/alloc.rs` counts only the calling thread). Still
  allocating: the extract step's per-frame `Vec`s. Contact pass 100 bloated: 0.042 to 0.024 ms/frame. Viewer (vsync, 100 bloated, 13 workers, #140): simulation
  1.52 to 1.00 ms/frame, island step wall 1.35 to ~0.85 ms, extract 0.126 to ~0.087 ms; render thread 0.69 to
  0.74-0.84 ms (the spin competes). Without the spin only ~5 of 13 threads start a step promptly: Windows wakes
  threads on idle cores late (the owner's plan parks cores down to 4 %, `powercfg /qh scheme_current sub_processor
  CPMINCORES`).
- `civbb-anim`: curve binding, clip sampling, state machines, blend trees, Write Defaults, built-ins, behaviours,
  humanoid muscles and constraints (ADRs 0007 to 0011, 0014, 0018, 0019). Muscles default to 0 (Unity's default
  human pose) and Write Defaults off never holds them; exit times below 1 recur every cycle (ADR 0056), the start time does not reach one and a transition fired at its exit time starts with the overshoot elapsed; the first update advances like any other; a Write Defaults on state without a motion writes nothing (ADR 0065, recorded); instances start entries at `Entry::default` (`Controller::start`; the expression default for lowered Unity content,
  #85) and Write Defaults restores a curve-written entry to `Entry::rest` (the animator default, ADR 0095) and platform
  values standing on the ground (`action::at_rest`, `Instance::set_platform`, ADR 0055). Material slots swap per island
  (`Target::MaterialSlot`, `Island::submesh_materials`, a per-submesh override in `MeshData::draws`; ADR 0059).
  Mirrored states and blend tree children map muscles as Unity records (`muscle::mirror`, exclusive or; ADR 0060). `civbb-dynamics`: PhysBone chains as the SDK
  steps them (ADR 0049, 0068: fixed 1/60 s steps counted in f32, between them the shown direction blended from the
  last step towards the frame's by `r / (1 - pull (1 - r))`, steps against the pose interpolated from the last step's,
  colliders from the last frame's, ADR 0074; versions 1.0/1.1, simplified and advanced
  integration, immobile, limits with their rotation composed `qz qy qx` as the SDK does (#108), segment colliders;
  not the hand and finger colliders that `allowCollision` chains meet, #92) with grab/pose (ADR 0015's, unverified; outputs written to their entries before the next frame's animator update,
  as receivers are), and contacts with the
  cross-avatar grid broadphase (`contact::step`/`step_with`, ADR 0017): sphere, capsule and box shapes, face proximity, the
  component's node apart from the root transform, Proximity from a sender's second frame (ADR 0047). Format v13
  (`rebake` ADR 0046, contact shapes ADR 0047, PhysBone `version` ADR 0049, `Toon::light` ADR 0050).

## Test data

- Test avatars: `civbb-corpus` (`lean()`, `bloated()`, `generate`, `gltf::to_glb`), and locally `corpus/local/khronos/` (23 Khronos samples), `corpus/local/khronos-quantized/` (4 `KHR_mesh_quantization` samples with their originals, #45) `corpus/local/vrm/` (AliciaSolid 0.x, Seed-san and the twist sample 1.0) and `corpus/local/glb/` (Booth FBX conversions: HAOLAN, Taro, Tessakuchan, Yumeka, Little Blackbird, Ostgaroa, Cokoro), `corpus/local/unity-glb/` (13 exporter outputs, listed with their prefabs in its `exports.txt`) and `corpus/local/unity-truth/` (Unity recordings); see `corpus/README.md`. `civbb inspect <file>` prints what an avatar contains.
