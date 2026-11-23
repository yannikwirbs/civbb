# 0072: Unity's muscle frames follow the skeleton the Avatar was built from; thumbs need no extra rule

Status: accepted
Date: 2026-11-23

Corrects ADR 0064's account of thumbs and finger spread; its decision stands.

## Context

ADR 0064 derived muscle frames from the rest pose and found thumbs 10 to 28 degrees off on the T-posed corpus
exports (fingers up to 8), guessing that the thumb's pre rotation depends on the hand's geometry (#95).
`MuscleFrames.cs` now also records the generated humanoid with the thumb root moved three ways, the thumb turned and
twisted, fingers fanned and moved, and the hand rolled: ADR 0064's rules match Unity within 0.05 degrees on all of
them. A probe then compared each export's `Avatar.humanDescription.skeleton` (the pose `AvatarBuilder` used) with the
prefab's rest: they differ exactly where the derived frames did (thumbs 9.7 to 31.8 degrees, fingers up to 7.9, HAOLAN's
neck 2.4, Cokoro's eyes 25), because the importer builds the avatar from an adjusted pose (such as Enforce T-Pose).

## Decision

1. No hand-geometry rule: the derivation stays as ADR 0064 has it.
2. The local corpus check derives from the build skeleton (`tools/unity-truth/Assets/Editor/BuildSkeleton.cs` writes
   it to `corpus/local/unity-truth/<name>.skeleton.json`) and holds every bone of every export within 0.15 degrees
   (Blackbird's shoulders 1.75, unexplained), thumbs and eyes included.
3. Exports keep carrying Unity's own frames, so the build skeleton is not exported.

## Consequences

glTF and VRM avatars get Unity's frames for the pose they rest in; a source whose rest thumbs are not the pose a Unity
import would build from (an FBX converted to glTF after Enforce T-Pose) gets frames for its rest instead, as ADR 0064
point 3 already says for the whole skeleton.
