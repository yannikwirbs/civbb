# 0037: VRChat components reach the pipeline through the Unity exporter, never through a unitypackage reader

Status: accepted
Date: 2026-11-05

## Context

The four Booth avatars in `corpus/local/` are imported from FBX converted with Blender (#11), so they carry meshes,
skins, shape keys and textures but none of their VRChat setup: avatar descriptor (view point, visemes, eye look),
PhysBones, contacts, constraints, FX animators, expression menus and parameters. That data sits in the creators'
`.unitypackage` files (#47). Two ways to get it into the compiler:

- (a) the Unity editor plugin of design doc §10 (#36), which reads the project through Unity's serialisation and
  writes the source package of §9 (glTF plus `X_*` extensions);
- (b) a bounded, fuzzable Rust reader of `.unitypackage` (a tar.gz of `<guid>/{pathname,asset,asset.meta}` with
  Unity YAML assets referencing each other by GUID and fileID) inside the trust boundary.

Measured on the seven packages in `corpus/local/booth/` with a read-only script (counts only; nothing committed).
The five avatar packages:

| | HAOLAN PC | HAOLAN Quest | Taro | Tessakuchan | Yumeka |
| --- | ---: | ---: | ---: | ---: | ---: |
| `.unitypackage` / uncompressed (MB) | 28.1 / 33.5 | 22.1 / 25.8 | 16.4 / 20.4 | 3.1 / 7.9 | 69.3 / 82.5 |
| assets (largest, MB) | 73 (7.9 FBX) | 63 (7.3) | 361 (5.1) | 78 (1.0) | 152 (14.8 FBX) |
| Unity YAML assets (MB) | 49 (2.9) | 46 (2.1) | 24 (0.6) | 57 (4.8) | 83 (5.8) |
| PNG textures (MB) | 10 (22.3) | 5 (20.6) | 6 (14.0) | 7 (2.4) | 45 (61.0) |
| controllers / clips | 4 / 29 | 3 / 30 | 1 / 14 | 4 / 30 | 3 / 64 |
| states / transitions / blend trees | 174 / 332 / 150 | 123 / 249 / 113 | 52 / 82 / 37 | 88 / 153 / 1 | 146 / 274 / 113 |
| state behaviours (tracking, layer, driver) | 114 | 76 | 0 | 30 | 28 |
| avatar descriptor | 1 | 1 | 1 | 1 | 1 |
| PhysBones / PhysBone colliders | 36 / 33 | 7 / 0 | 9 / 0 | 4 / 0 | 57 / 9 |
| Dynamic Bones / colliders (scripts not shipped) | 36 / 34 | 0 | 0 | 0 | 0 |
| contacts | 0 | 0 | 0 | 0 | 0 |
| constraints | 4 Unity rotation | 0 | 0 | 0 | 4 VRChat |
| expression menus / parameter lists | 3 / 2 | 3 / 2 | 1 / 1 | 2 / 1 | 5 / 1 |
| humanoid bones in the FBX `.meta` | 49 | 51 | 37 | 51 | 53 |
| GameObjects in YAML (stripped) | 454 | 141 | 88 | 6 (1) | 3 (68) |
| references into the FBX (distinct fileIDs) | 22 (11) | 11 (11) | 7 (7) | 39 (23) | 347 (119) |
| prefab modifications | 26 | 0 | 12 | 225 | 182 |
| `m_Motion` references to clips outside the package (GUIDs) | 1147 (44) | 869 (44) | 277 (15) | 53 (21) | 853 (36) |

The other two packages are a 256-polygon Tessakuchan variant (0.4 MB, 10 YAML assets, 4 prefab instances) and an
outfit prefab (2 KB, one prefab instance with 20 modifications of a prefab it does not ship).

Further facts from the same pass:

- Every Unity asset is text YAML, none binary-serialised; 44 of HAOLAN Quest's start with a UTF-8 BOM. The FBX
  importer settings come in two serialisation versions (`serializedVersion` 19301 and 22200, i.e. Unity 2019.4 and
  2022.x projects).
- VRChat components are MonoBehaviours whose `m_Script` points into SDK assemblies the package does not ship
  (identified here by their serialised field names: avatar descriptor, PhysBone, PhysBone collider, VRChat
  constraint, expression menu and parameters, pipeline manager, four state-behaviour types). Two script GUIDs
  are not identifiable by field names at all.
- All five FBX `.meta` files use `fileIdsGeneration: 2` with an empty `internalIDToNameTable`: the fileIDs of the
  meshes, bones and avatar the prefabs reference are derived by Unity's model importer and listed nowhere in the
  package. Yumeka's prefab is a prefab instance of the FBX itself: its 57 PhysBones sit on 68 stripped
  GameObjects that exist only as fileIDs inside the imported model. Tessakuchan is the same through 13 nested
  prefab instances.
- None of the seven packages contains `.cs` or `.dll` files. Shader sources do ship (Taro: 5 shaders and 305
  `.cginc` files); the other packages reference shaders outside the package.

## Decision

1. Option (a). VRChat components reach the pipeline only as the source package (§9) that the Unity exporter
   (#36) writes. The compiler never parses `.unitypackage` files or Unity YAML; its trust boundary stays the
   glTF importer it already has, extended by the `X_*` extensions (#71).
2. Option (b) is rejected for what the numbers show it would have to rebuild:
   - In three of five avatars the components sit on objects that only Unity's FBX import defines (Yumeka 347
     references to 119 importer fileIDs, Tessakuchan 39, the 256-polygon variant 12). Binding them needs Unity's
     undocumented fileID derivation and Unity's FBX import itself (node names, axis conversion, scale), which our
     Blender conversion does not reproduce. Without it the reader binds HAOLAN PC/Quest and Taro by path and
     drops Yumeka's 57 PhysBones.
   - Prefab overrides (up to 225 per avatar) are `propertyPath` strings into nested prefabs, some of which the
     package does not ship (the outfit prefab).
   - Every avatar's controllers reference 15 to 44 clips outside the package. A Rust reader cannot resolve them;
     a creator's project has them.
   - It would own schemas for about 15 Unity classes in at least two serialisation versions and every VRChat
     component, following SDK releases, plus NDMF, Modular Avatar and VRCFury builds, which only run in Unity.
   - It adds a second untrusted-input parser (gzip, tar, YAML subset, GUID graph) to fuzz and keep bounded.
   What (b) would give now, real component data for two or three avatars without the SDK, does not outweigh a
   reader that cannot be completed.
3. Order of work on the exporter (#36): first what Unity reads without the VRChat SDK, in batch mode on this
   machine, from the Booth packages imported into a gitignored Unity project in `corpus/local/` (the import runs
   no package code since none ships `.cs` or `.dll`; the exporter's import step refuses packages that do):
   hierarchy, skinned meshes, humanoid avatar, materials, animator controllers, clips and Unity constraints. The
   VRChat components' fields come next, read through `SerializedObject` once the SDK is installed in that project
   (#37): Unity only deserialises a MonoBehaviour's fields when its script class loads.
4. Where an avatar has both Dynamic Bones and PhysBones (HAOLAN PC), the exporter exports the PhysBones. Dynamic
   Bone conversion (§10, "High") is later work.

## Consequences

- One path for creators and for the corpus: the same exporter writes the source package and, later, the
  reference images (§10). The compiler's untrusted input stays glTF.
- Real VRChat component data waits for #37 (the human installs the SDK). Until then the compiler keeps testing
  components on the generated corpus and on VRM.
- The exporter needs a C# glTF writer and the `X_*` extension schemas; the Rust importer needs to read those
  extensions (#71). Both sides are tested against the same exported Booth files locally (never committed).
- Revisit if creators turn out to upload unitypackages without a Unity install: then a reader would still need
  Unity's model importer, so the answer would more likely be running the exporter on a worker than parsing YAML.
