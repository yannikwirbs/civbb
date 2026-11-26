# `CIVBB_dynamics`

Root-level. Simulated bone chains and the colliders they collide with (ADRs 0016, 0049), mirroring the documented
PhysBone parameters. Schema: [`schema/CIVBB_dynamics.schema.json`](schema/CIVBB_dynamics.schema.json).

## Fields

- `chains`, each:
  - `root`: every descendant not under a node in `ignore` is simulated; `endpoint`: a virtual tip bone offset added
    to each leaf, in leaf space.
  - Solver: `multi_child` (how a root with several children behaves), `version` (`"1.0"` pulls with a force,
    `"1.1"` bends the rest direction towards gravity), `integration` (simplified or advanced).
  - Graded parameters, each `{value, curve}`: a base value scaled along the chain by an optional curve over
    normalised depth (0 at the root, 1 at the tip): `pull`, `spring`, `stiffness`, `gravity` (-1 to 1),
    `gravity_falloff`, `immobile` (with `immobile_type`), `radius`, `max_stretch` and `max_squish` (fractions of bone
    length).
  - `limit`: `none`, `angle` or `hinge` (`max_angle`, `rotation`) or `polar` (`max_pitch`, `max_yaw`, `rotation`).
  - `colliders`: indices into `colliders`. `grab` and `pose` allow grabbing and posing.
  - `outputs`: optional state entries the chain writes after each frame (ADR 0088): `is_grabbed`, `is_posed`,
    `angle` (tip angle from rest, 0 to 1 for 0 to 180 degrees), `stretch`, `squish`.
- `colliders`, each: `node`, `shape` (`sphere` with `radius`, `capsule` with `radius` and `height`, or `plane`),
  `position` and `rotation` relative to the node, `inside` (keep bones inside instead of outside).

## Rules

Roots, ignored nodes and collider nodes exist; collider indices resolve; every number is finite; a graded curve has at
most 64 points; outputs are existing entries and not platform values. At most 1,024 chains and 512 colliders.
