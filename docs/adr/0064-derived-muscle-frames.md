# 0064: Muscle frames derived from the rest pose for glTF and VRM humanoids

Status: accepted
Date: 2026-11-21

## Context

Muscle curves turn bones through per-bone muscle frames (ADR 0014). The Unity exporter copies them from Unity's
`Avatar`; glTF and VRM imports had none, so humanoid clips could not play on those avatars (#53). Unity's
`AvatarBuilder` derives them from the skeleton. `tools/unity-truth/Assets/Editor/MuscleFrames.cs` records its frames on
the generated humanoid reshaped one way at a time (rolled bone axes, sloped shoulders, A-pose, arms forward, bent
elbows, curved spine, legs apart, bent hands, curled fingers, turned feet, tilted hips, turned head), and the 13 corpus
exports hold Unity's frames for real skeletons:

- The post frame (muscle space in the bone's own space) has X along the bone: towards the next mapped human bone
  (chest to neck without an upper chest), continuing the parent's direction for hands and distal finger joints, and
  fixed avatar axes for head (up), feet (down), toes, eyes and jaw (forward). Its roll puts Y or Z on the projection of
  a fixed avatar axis per bone (sideways for spine and legs, forward for arms and fingers, up for lower arms and thumbs,
  vertical for shoulders and hands). Within 0.03 degrees on every variant.
- The pre frame (muscle space in the parent node's space; the hips' in avatar space) is the post frame in avatar
  space turned by a fixed avatar-space rotation per bone: constant within 2e-5 degrees across all variants. In our
  axes it is the identity for spine, head, shoulders, hands and feet, 30 degrees for upper legs, 80 for lower legs and lower arms, 48.6 for upper arms, 20 to
  37 for fingers, 10 for the jaw; the right side mirrors the left.
- Unity's limit signs are a fixed table per bone; default limits are the muscle table's; twist shares default to 0.5.
- On the 10 corpus exports whose rest is Unity's skeleton, the rules give Unity's frames for spine, head, arms, legs,
  hands and feet exactly (Blackbird's shoulders 1.8 degrees, HAOLAN's neck and head 2.4), finger pre frames within
  0.4 to 8 degrees (the spread follows the hand), thumbs 10 to 28 degrees off: their pre rotation varies per avatar
  (19 to 43 degrees), so it depends on the hand's geometry, which the variants never changed (#95). Cokoro's eyes
  differ by 25 degrees in the post frame only.

## Decision

1. `civbb_anim::frames::derive` computes every mapped bone's frame from the rest pose with those rules and a table of
   the per-bone rotations and signs read from the T-pose recording. `tests/muscle_frames.rs` checks it against all
   recorded variants (within 0.0002 degrees); `unity_export.rs` checks the corpus locally (body and limbs within 2.5
   degrees, fingers 8, thumbs and eyes reported).
2. The glTF importer derives frames for a humanoid that has none (glTF, VRM). Exports keep Unity's own.
3. The rest pose is taken as the skeleton Unity would have built from: VRM requires a T-pose rest; glTF has no rule.
   VRM 0.x per-bone limits and twist settings are not mapped: no local VRM sets non-default values.

## Consequences

Humanoid clips play on glTF and VRM avatars as Unity would play them on the same skeleton, thumbs approximately until
#95. An avatar whose Unity `Avatar` was built from another pose than its rest (KohzieBoi, Megumi and Taro in the
corpus) gets different frames from Unity's; exports carry Unity's, so only a glTF or VRM conversion of such an avatar
would differ.
