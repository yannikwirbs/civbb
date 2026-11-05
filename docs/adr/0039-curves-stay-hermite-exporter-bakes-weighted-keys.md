# 0039: Curves stay Hermite; the exporter bakes weighted tangents, splits equal key times and mirrors curve values

Status: accepted
Date: 2026-11-05

## Context

`animator::Key` is a plain Hermite key (ADR 0007), and validation rejects equal key times. Unity keys can also
carry weighted (Bezier) tangents (`weightedMode` 1 to 3 with `inWeight`/`outWeight`), YAML can hold two keys at
one time, and Unity's values are left-handed (#50). Measured on the five Booth avatar packages (counts only, the
script of ADR 0037): 167 clips, 16,239 curves, 28,693 keys; **no** weighted key (`weightedMode` 0 everywhere),
**no** equal key times and no gap under 0.1 ms.

## Decision

1. The format keeps Hermite keys; no weights, no duplicate times. A feature none of the corpus uses does not get
   a runtime path.
2. The exporter (#36) converts each `AnimationCurve` before writing it:
   - A segment with a weighted tangent on either side is baked: split at its midpoint into Hermite keys whose
     values are Unity's own `AnimationCurve.Evaluate` and whose tangents are its central differences, recursively
     until the Hermite result is within 1e-4 of the value range of `Evaluate` at the quarter points, at most 64
     keys per segment. The report counts baked segments.
   - Two keys at the same time (a jump): the earlier one gets an infinite out tangent (a step, ADR 0007) and the
     later one moves 1e-4 s later, so the curve holds the first value up to the jump. The report counts them.
   - Values follow the X mirror of ADR 0038, tangents with them: `m_LocalPosition.x`, `localEulerAnglesRaw.y`
     and `.z`, and `m_LocalRotation.y` and `.z` are negated. Scale, blendshape, material, active, parameter and
     muscle curves are unchanged (muscle values are defined per side by Unity's muscle space, ADR 0014).

## Consequences

- The runtime sampler and validation stay as they are; ADR 0007's note on weighted tangents is settled.
- A creator's weighted curve costs extra keys but keeps Unity's shape within the tolerance; if a future corpus
  shows many weighted curves, measure key counts before considering weights in the format.
- Checked against Unity once the exporter runs (#72): a baked curve sampled densely against `Evaluate`.
