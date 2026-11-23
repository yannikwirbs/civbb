# 0073: The centre of mass with Unity's per-bone mass positions, fitted

Status: accepted
Date: 2026-11-23

Refines ADR 0063's mass model (masses at segment midpoints).

## Context

ADR 0063 placed each bone's own mass (Unity's `HumanTrait.GetBoneDefaultHierarchyMass` minus its children's) at
the midpoint to its next bone, which left 5 mm on the generated skeletons of `BodyPose.cs` (7 mm without upper chest)
and 2 to 4 mm on the corpus hips (#96); Unity's rest centre sat 2.5 mm off-axis on a symmetric skeleton. Reading
`Avatar.GetAxisLength` and the post frames by reflection and fitting a per-bone offset along each post frame (scaled
by axis length) over the plain and rolled skeletons left 0.03 mm, with clear values: hips, head, hands, feet and
toes at the joint; legs, spine, chest, neck, shoulders, lower arms and the right upper arm halfway; the upper chest
about 7/12 of the way; and the **left** upper arm 1/8 of the way, which alone accounts for the off-axis rest centre
(2 x 0.27 x 0.375 / 82.5 = 2.5 mm). Our format carries neither axis lengths nor that offset table, and in terms of
the next bone's position the same fractions leave 0.5 mm.

## Decision

1. Each bone's mass sits at its joint plus a fraction of the way to its next bone: 1/2 by default, 0 for hips,
   head, hands, feet and toes, 7/12 for the upper chest, 1/8 for the left upper arm.
2. A missing toe's mass stays at its foot (Unity's hierarchy mass keeps it). A missing upper chest's mass sits 5/6
   of the way from chest to neck (fitted; the chest's own halfway along the same segment).

## Consequences

Generated skeletons within 0.52 mm (1.25 mm without upper chest, with or without toes); corpus hips on recorded
clips within 1 mm (were 2 to 4), now checked at 1.5 mm in `unity_export.rs`. The rest anchor (ADR 0063) stays.
Unity's exact model needs its axis lengths, which no format field carries; the remaining 0.5 to 1.3 mm is that.
