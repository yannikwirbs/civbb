# 0038: The Unity exporter is a UPM package in this repo with its own GLB writer, run in batch mode

Status: accepted
Date: 2026-11-05

## Context

ADR 0037 makes the Unity exporter (#36, design doc §10) the only way VRChat components reach the compiler. The
design doc's repo table (§21) gives it its own repo, `civbb-unity-exporter`, for "Unity's own package tooling and
release cadence". This project is built by AI sessions that work from one `STATUS.md`, one code map and one issue
queue, and no CI runner can run Unity (it needs a licence). The exporter's output must be what our glTF importer
accepts: nodes, skinned meshes with submeshes, four influences, morph targets with `extras.targetNames`, PBR
materials with embedded PNG or JPEG images (`crates/pipeline/src/gltf_import.rs`).

## Decision

1. The exporter lives at `tools/unity-exporter/` as a UPM package (`civbb.unity-exporter`, MIT OR Apache-2.0,
   editor-only assembly). Creators install it by git URL with `?path=tools/unity-exporter`, which gives it its own
   tags and release cadence without a second repo. Split it out only if that stops being true. This deviates
   from §21's separate repo.
2. Sessions test it in batch mode on a host project in `corpus/local/unity-export/` (gitignored, created from
   the README), never on a project the human has open:
   `Civbb.Exporter.Batch.Import` checks a `.unitypackage` and imports it, `Civbb.Exporter.Batch.Export` writes one
   prefab as GLB (`-civbbAvatar`, without it lists the humanoid prefabs).
3. The import step reads the package's tar.gz entry names and refuses packages with `.cs`, `.dll` or `.asmdef`
   files (ADR 0037: importing must run no package code). It makes imported models readable so the exporter can
   read their meshes.
4. Own GLB writer (about 500 lines of C#), no glTF package: we need a small, exact subset, the importer defines
   what it accepts, and materials, animators and components go into `X_*` extensions we write ourselves anyway.
   - Unity's left-handed space maps to glTF by mirroring X: positions and normals `(-x, y, z)`, rotations
     `(x, -y, -z, w)`, tangents `(-x, y, z, -w)`, matrices `S M S`, triangle winding reversed. Both face +Z
     (ADR 0006). UV V becomes `1 - v`.
   - The prefab root is exported at the origin; every transform below it is a node with its local TRS.
   - One glTF mesh per renderer, one primitive per submesh with that renderer's material. Inverse bind matrices
     are Unity's bind poses (they already include the renderer's transform at bind time). Four influences
     (`Mesh.boneWeights`).
   - A blendshape exports its last frame (weight 100) as position and normal deltas; the renderer's weights are
     the mesh's default weights. Multi-frame shapes are counted in the report.
   - Materials read the saved properties (`m_SavedProperties`), which survive a missing shader: `_MainTex` as base
     colour texture (source PNG or JPEG bytes as they are), `_Color` as base colour factor in linear space, metallic
     0, roughness 1, the Unity shader name in `extras.unityShader`. Shader mapping comes with `X_toon_material`.
   - The humanoid map from the root `Animator`'s avatar goes into `X_avatar.humanoid` as `HumanBodyBones` names
     to node indices. Its full schema, and the Rust side, are #71.

## Consequences

- One queue and one status for exporter and compiler; the exporter is versioned by tags in this repo.
- The exporter's output is checked by importing it with `civbb` next to the Blender conversion of the same
  avatar (counts per mesh, triangles, shapes, joints). Unity's FBX import is the truth: differences point at the
  Blender path, which the exporter replaces for Booth avatars.
- Nothing in `cargo xtask ci` runs Unity; exporter tests are local batch runs recorded in issue comments, as with
  `tools/unity-truth` (ADR 0013).
- The Booth projects were made in Unity 2019.4 and 2022.x; we import them into the pinned 6000.3. Revisit when
  VRChat's own Unity version can be installed (#37).
