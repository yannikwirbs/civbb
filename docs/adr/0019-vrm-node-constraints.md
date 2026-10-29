# 0019: VRM node constraints become rest-relative constraint kinds (format v7)

Status: accepted
Date: 2026-10-29

## Context

#46 imports `VRMC_node_constraint` (VRM 1.0; github.com/vrm-c/vrm-specification,
`specification/VRMC_node_constraint-1.0`). Its three constraints are defined relative to rest poses, unlike Unity's
(ADR 0018): rotation copies the source's local rotation change from its rest onto the destination's rest
("Local-Local", as Blender's copy rotation); roll keeps that change's twist about an axis of the destination; aim
turns the destination from its rest by the shortest rotation pointing an axis at the source, with no up vector.
Weight slerps from the destination's rest rotation. The spec's formulas are non-normative pseudocode, which this
follows. None of the three equals a Unity constraint with offsets, so mapping them onto Unity kinds would change
how twist bones move.

## Decision

1. Format v7 adds `ConstraintKind::RestRotation`, `RestRoll { axis }` and `RestAim { axis }`: one source (the
   first), the target's rest rotation in `rest`, `weight` slerping from it, `axes` ignored. `civbb-anim` evaluates
   them with the spec's pseudocode in the same sorted pass as Unity kinds; the source's rest rotation comes from the
   hierarchy.
2. The VRM importer maps each node's constraint to one of them: `rollAxis` `X`/`Y`/`Z` and `aimAxis`
   `PositiveX` ... `NegativeZ` become unit vectors, `weight` defaults to 1 and is clamped to 0 to 1. A constraint
   with an unknown type or axis, a missing source or the node itself as source is skipped with a warning.
   Coordinates keep glTF conventions (the importer does not convert them), so the axes need no change.
3. Order is the runtime's dependency sort (ADR 0018), which also covers the spec's "dependencies first"; a cycle,
   which the spec forbids, falls back to list order instead of failing the import.

## Consequences

- Hand-derived tests cover each kind (weight, the roll axis in the target's rest frame, aim under a turned parent);
  the importer test covers mapping and skipping; `VRM1_Constraint_Twist_Sample.vrm` (local corpus) imports all 14
  of its aim and roll constraints without warnings.
- Not compared against a reference implementation (three-vrm, UniVRM): a recorded comparison belongs with parity
  (M0.9). The roll pseudocode assumes the source and destination share a parent frame, as twist bones do.
