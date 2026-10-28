# 0006: Avatars face +Z, faces are shape binds, VRM maps to native types by documented heuristics

Status: accepted
Date: 2026-10-28

## Context

The design doc (§9 "Normalise: units, axes", §9 `X_avatar` "viseme mapping") leaves three things open that the VRM importer (#10) needs:

1. Which way an avatar faces. glTF has no rule; VRM 0.x models face -Z, VRM 1.0 and our corpus face +Z.
2. How visemes and blinking are stored. Format v1 had no field for them.
3. How VRM spring bones and MToon map to our PhysBone-shaped chains and Toon model. The models differ, so no mapping is exact.

## Decision

1. Avatars face +Z in avatar space. Importers normalise: VRM 0.x gets a 180° Y rotation on the root node's rest transform (exact, and skinning follows because joints inherit it).
2. `Avatar::face` holds visemes (the 15 legacy visemes) and blink as lists of `(mesh, blendshape, weight)` binds. This covers both legacy viseme mappings (one shape per viseme) and VRM expressions (several weighted shapes). `FORMAT_VERSION` goes to 2. Eye-look setup and first-person mesh visibility come later (#44).
3. VRM mapping, all in `crates/pipeline/src/vrm.rs`:
   - Spring bones: one chain per VRM 0.x bone-group root or VRM 1.0 spring. `pull = stiffness / 4`, `spring = 1 - dragForce`, `gravity = gravityPower * -gravityDir.y`, all clamped; per-joint VRM 1.0 values become graded curves. Spheres and capsules map directly.
   - MToon: lit, shade, normal, rim, matcap, emission and outline map to the Toon fields with the same meaning. VRM 0.x colours are sRGB and are linearised. Screen-space outlines and UV animation are dropped with a warning.
   - Expressions: visemes and blink go to `face`; every other expression becomes a clip, a float parameter, a child of one Direct blend tree layer, and a menu toggle.

## Consequences

- Every downstream system can assume +Z facing.
- Spring-bone and MToon heuristics are first guesses. The parity check (M0.9) measures them; tune the constants then, in a new ADR if the approach changes.
- Expressions driven through one Direct blend tree cost 8 synced bits each; avatars with many custom expressions may exceed the budget, in which case the rest are local only.
