# 0046: VRChat constraint options follow the SDK's own runtime: freeze capture, rebaking, scale offsets (format v10)

Status: accepted
Date: 2026-11-19

## Context

ADR 0040 guessed Freeze To World's capture frame and per-axis hold and left Rebake Offsets When Unfrozen out (#73):
VRChat's Constraints page does not define them. The VRChat SDK (3.10.6, in the exporter host project) runs its
constraints in editor play mode (#75), so its runtime can be recorded like Unity's (ADR 0013).
`tools/unity-truth/Assets/Editor/VrcConstraints.cs` builds generated cases there, drives the SDK components by
reflection (`FreezeToWorld`, `IsActive`, `GlobalWeight`, source weights, then `ApplyConfigurationChanges`) and
records ten frames per case: every frame's input pose, field changes, output pose and the component's offsets. The
sources and the target's parent move every frame; the target is either animated every frame or left alone. 73 cases
(`crates/anim/tests/unity/vrc_constraints.json`): every kind plain and in local space, freezing per kind with and
without rebaking, per-axis, weights, freeze at start, freeze while inactive, deactivation while frozen, refreezing,
weight changes while frozen, and scale offsets against source weights.

What the recordings show where ADR 0040 differs or is silent:

- On the frame a constraint freezes, the target stays as animated; that world placement is what it holds. ADR 0040
  captured the placement the constraint would have given it.
- Going inactive keeps the frozen placement: reactivated while still frozen, the target returns to it. Only
  unfreezing drops it. Freezing while inactive captures on the first active frame.
- Per-axis hold, weights and local space (every kind, aim and look-at included) behave as ADR 0040 implemented them.
- Rebaking happens on the unfreeze frame: the SDK rewrites the component's offsets so the constraint gives the target
  its current local values, and the target stays as animated that frame.
- Scale offsets are raised to a power that depends on the source weights (below); the old cases had no scale offset
  with weights other than 1.

## Decision

1. **Freeze capture.** The first frame a constraint is active and frozen, the target's animated world placement is
   captured and the constraint leaves the target as animated. Later frozen frames hold it on the parts and axes the
   constraint constrains (ADR 0040's hold, unchanged). Deactivating keeps the capture; unfreezing drops it.
2. **Rebake (format v10).** `ConstraintOptions::rebake` (serde default false; the exporter writes it). On the frame an
   active constraint with a capture is unfrozen, the island's offsets are replaced by baked ones, then it solves as
   usual:
   - position: the current local position minus the sources' weighted mean in the parent's space;
   - rotation, aim, look-at: the Euler offset turning the constrained rotation into the current local one;
   - parent: per source, the rotation offset from that source to the target, and one translation (the target minus
     the sources' weighted mean position) turned into each source's frame;
   - scale: current local scale times the parent's scale, divided by the **product** of the weighted sources' scales
     (not their blend; with several sources or non-unit weights the target jumps when unfrozen, as in the SDK).
   Offsets are per-island state (`constraint::Scratch`); the stored ones stay as compiled.
3. **Scale offsets.** The SDK applies a scale offset as `offset^(n / total)`, n the number of sources with weight
   and total their weight sum, plus 1 per zero-weight source when only one source has weight. For the usual single
   source of weight 1 that is the offset itself. The fit is exact on every recorded scale case; the SDK's reason for it is
   unknown.
4. Points 5 and 6 of ADR 0040 (capture frame, rebake not implemented) are superseded; the rest of ADR 0040 stands.

## Consequences

- `crates/anim/tests/vrc_constraint.rs` replays every recorded frame on one island; all 73 cases match within 2e-4
  (scale within 1%, as for Unity's constraints). Re-record with the recorder after an SDK update.
- Not recorded, so unverified: unfreezing while inactive (no bake), zero-weight sources in a scale rebake (excluded
  from the product), `SmoothingFactor` (not in the format; the recordings keep the SDK's default).
- The scale offset rule mirrors an SDK quirk; if VRChat changes it, the recording shows it.
