# 0042: The exporter reads VRChat components by field name; X_avatar gains muscle frames and the face

Status: accepted
Date: 2026-11-06

## Context

The VRChat SDK host project exists (#37: `corpus/local/unity-export-vrc`, Unity 2022.3.22f1, `com.vrchat.avatars`
3.10.6). With the SDK, the avatar descriptor, PhysBones, colliders, contacts, VRChat constraints and state
behaviours resolve to real components, and the exporter (ADRs 0037, 0038) can read them into the `X_*` extensions
(ADR 0041). ADR 0041 left open how, and deferred visemes and eye look in `X_avatar`. The repo rules forbid copying
VRChat code or SDK assets.

Measured on the 11 avatar packages imported into the SDK project (26 humanoid prefabs, 0 missing scripts): every
avatar has a descriptor; PhysBones on 10 (3 to 57), PhysBone colliders on 3 (4 to 33); VRChat constraints only on
Yumeka (4 `VRCRotationConstraint`); no contacts; `VRCHeadChop` on 3 Cokoro prefabs. Behaviours in the controllers:
parameter drivers, tracking control, layer and playable layer control, temporary pose space and play audio.
Default (SDK) playable layers point at the SDK's own controllers and proxy clips.

Also found: the exporter wrote no muscle frames, so every muscle curve of an exported avatar stayed unbound, and
`AssetDatabase.ImportPackage` in the SDK project is followed by a domain reload (the SDK changes settings and
recompiles) that drops static completion callbacks.

## Decision

1. **Reading.** The exporter finds VRChat components by type full name and reads them through `SerializedObject`
   by serialized field name (names taken from a field dump of SDK 3.10.6). It references no SDK assembly and still
   compiles and runs without the SDK (the components are then missing scripts and nothing is found). A component
   with an unknown field is skipped and counted in the export report, never guessed.
2. **SDK assets are referenced, not copied.** Controllers and clips whose asset path is under
   `Packages/com.vrchat.*` are not exported: a default playable layer leaves its group empty, a proxy clip becomes
   an empty clip that keeps its name. The runtime supplies its own defaults (locomotion, proxy poses) later.
3. **Playable layers.** Without `-civbbLayers`, the animator's groups are the descriptor's base layers in
   VRChat's order, always all five: `Base`, `Additive`, `Gesture`, `Action`, `FX`. A layer that is default, not
   customised or has no controller stays an empty group, so group indices are fixed. `Action` starts at group
   weight 0 (VRChat raises it with Playable Layer Control); the others at 1. Descriptor layer masks and the
   special layers (Sitting, TPose, IKPose) are counted and left out.
4. **Behaviours.** Parameter driver (set, add, random, copy with range), tracking control, animator layer control
   (`LayerWeight` with the playable's first layer plus the layer index), playable layer control (`GroupWeight`)
   and locomotion control map to the format's `Behaviour`s. Operations on parameters the animator lacks, random
   "prevent repeats", temporary pose space and play audio (#51) are counted and left out.
5. **X_avatar** gains two optional fields, both format JSON in glTF space:
   - `muscle_frames`: `[MuscleFrame]` from Unity's `Avatar` (`GetPreRotation`, `GetPostRotation`,
     `GetLimitSign`, as `tools/unity-truth` records them), limits from the human description or Unity's
     defaults, twist shares from the description. Rotations mirror like node rotations; the Y and Z signs negate.
   - `face`: the format `Face` where each `ShapeBind.mesh` is the glTF node carrying the mesh (the importer maps
     it to the format mesh) and eye bones are glTF nodes. Visemes come from the `VisemeBlendShape` lip sync mode
     (other modes are reported, not mapped); blink from the eyelid blendshape mode's first shape; eye look from
     the descriptor's eye bones, with each range map's input and output the angle between looking straight and
     looking in that direction (the left eye's rotations; looking right is the left eye's inner direction).
   `viewpoint` is the descriptor's view position (avatar root space).
6. **Dynamics, contacts, constraints, expressions.** PhysBones become chains (curves sampled at nine depths,
   since VRChat multiplies a value by its curve); colliders are listed once and referenced by index; collision
   off empties a chain's collider list. Contact senders and receivers map one to one; box shapes wait for #55.
   VRChat position, rotation, scale and parent constraints become format constraints with `local_space` and
   `freeze` (ADR 0040), appended after Unity's; aim and look-at ones, unseen in the corpus, are counted and left
   out. "Rebake offsets when unfrozen" is counted (#73). The expression menu tree and parameters map one to one;
   icons are not exported.
7. **Constraint rest values.** Unity's own constraints export the target's local transform as `rest`, not the
   serialized at-rest fields, except look-at. Measured in Unity 6000.3.28f1 and 2022.3.22f1: `Object.Instantiate`
   (how a runtime loads an avatar) resets position, rotation, scale, parent and aim constraints' at-rest values
   to the transform's local values; look-at keeps its own, and `PrefabUtility.InstantiatePrefab` keeps the
   serialized ones. HAOLAN's two sleeve constraints carry identical serialized at-rest values while their targets
   rest differently; with the serialized value the right one was 2 degrees off Unity, with the local transform both
   match within 0.04 degrees. VRChat constraints keep their serialized values until measured.
8. **Import completion.** `Batch.Import` polls until every asset the package lists exists (by path, or by GUID
   when the project already had it) with nothing compiling or importing, and keeps the pending import in
   `SessionState` so a domain reload resumes the watch.

## Consequences

- `crates/pipeline/tests/unity_export.rs` compares exported avatars with Unity: clips sampled on the prefab and
  constraint results, recorded by `tools/unity-truth/Assets/Editor/Exported.cs` into `corpus/local/unity-truth`
  (local only, skipped without it). Muscle, rotation, Euler and blendshape curves of HAOLAN, Tessakuchan and
  Cokoro clips match Unity exactly; the recorder must force the animator to always animate (culling animators
  evaluate nothing in batch mode).
- One exporter serves projects with and without the SDK; a new SDK field shows up as a skipped count in the
  report, not a wrong value.
- Unverified until compared in VRChat or with recordings: eye look range maps (VRChat's rotations are reduced to
  one angle per direction), the Action group's start weight, and PhysBone version 1.1 differences (#38 baselines).
- Unmapped and counted for later: jaw-flap lip sync, eyelid look shapes, head chops, descriptor masks, special
  layers, menu icons, contact box shapes (#55), aim and look-at VRChat constraints, play audio (#51).
