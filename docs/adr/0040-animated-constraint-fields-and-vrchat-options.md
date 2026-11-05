# 0040: Clips animate constraint weights, activity and freezing per island; VRChat's local-space and freeze options (format v9)

Status: accepted
Date: 2026-11-05

## Context

ADR 0018 evaluates constraints with their stored weight, active flag and source weights. Avatars animate those
(toggles, blends between sources), and VRChat's constraints add *Solve In Local Space*, *Freeze To World* and
*Rebake Offsets When Unfrozen* (#56). VRChat's public Constraints page (creators.vrchat.com/common-components/
constraints) says: VRChat constraints are "a like-for-like replacement of Unity's constraints system, with a few
additional features"; with Solve In Local Space "the constraint will match the **local** position/rotation/scale
of its sources"; with Freeze To World it "ignores all of its sources and locks its target transform in world
space" until disabled, and Freeze To World can be enabled from an animation clip; Rebake recalculates the offsets
relative to the sources on unfreezing instead of keeping the original ones; Target Transform cannot be animated.
It does not say how a frozen target is placed on the frame it freezes, nor where an unrebaked target goes.

Measured on the Booth packages (script of ADR 0037): Yumeka has 4 VRChat constraints (active, weight 0.5, local
space, freeze and rebake off), HAOLAN PC 4 Unity rotation constraints; no clip animates any constraint property.

## Decision

1. **Format v9.** `Property::Constraint { kind: ConstraintType, field: ConstraintField }` addresses the first
   constraint of that type (position, rotation, scale, parent, aim, look-at) whose target is the node at the curve's
   path, as Unity's bindings address a component by type. Fields: `Weight`, `Active`, `SourceWeight(n)`, `Freeze`.
   `Constraint::options` holds `local_space` and `freeze` (the value at start); serde defaults keep older JSON
   (the Unity recordings) readable. The VRM rest-relative kinds have no type and bind nothing.
2. **Binding** (ADR 0007): `Target::Constraint(index, field)` with the stored value as its default, so Write
   Defaults returns to it. Masks treat it as the target node's. A missing constraint or source index is reported
   unbound. Flags are on at 0.5 or more, like `Active` curves.
3. **Per island.** The constraint pass's `Scratch` now holds each island's animated values (filled from the
   stored ones on first use) and its frozen placements. After the animator writes nodes it writes constraint slots
   (`Instance::write_constraints`); `Constraints::set` takes avatar-order indices and maps them to the sorted
   order.
4. **Solve In Local Space.** Sources are read as their local transforms and the result is the target's local value
   directly, with every formula of ADR 0018 unchanged (the parent placement is the identity).
5. **Freeze To World.** On the first frame an active constraint is frozen, the target's world placement is the one
   the constraint would give it that frame (or its animated one when the constraint has nothing to apply). While
   frozen, the target's local value is that placement in its parent's current space, on the parts and axes the
   constraint constrains; the rest stays animated. Unfreezing, or the constraint going inactive, drops the
   placement; the next freeze captures a new one. Without rebaking, an unfrozen constraint returns to its sources
   with its original offsets, which is what "keeping its original offset" says. Capture frame and per-axis hold are
   **unverified**.
6. **Rebake Offsets When Unfrozen** is not implemented: no corpus avatar uses it, and the page does not define the
   new offsets beyond "relative to its sources". It needs per-island offsets for every kind; #73.
7. The component's own node (VRChat's *Target Transform* may name another node than the component's GameObject)
   is not represented: curves address the constrained node. The exporter writes VRChat constraints on their target;
   a curve on a component whose target differs is reported unbound there (#36).

## Consequences

- Constraint toggles and source blends work per island and return to their stored values under Write Defaults.
- `Scratch` is per-island state now, not only a cache; islands already own one each.
- Unverified: the capture frame, per-axis hold and local-space aim/look-at. Check them against VRChat once the
  exporter runs real avatars; supersede this ADR if any differ.
