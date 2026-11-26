# `CIVBB_constraints`

Root-level. Transform constraints (ADRs 0018, 0040, 0046), one list whose order clip curves index (ADR 0101 point 1).
Schema: [`schema/CIVBB_constraints.schema.json`](schema/CIVBB_constraints.schema.json).

## Fields

`constraints`, each:

- `kind`, one of:
  - `position` (`offset`, metres), `rotation` (`offset`, Euler degrees), `scale` (`offset`, a factor): copy the
    weighted sources' value, plus or times the offset;
  - `parent` (`rotation_axes`): follow the sources as if parented, with per-source offsets;
  - `aim` (`offset`, `aim_axis`, `up_axis`, `world_up`): point the local `aim_axis` at the weighted source position;
    `world_up` is `scene_up`, `vector` (`up`), `object_up` (`node`), `object_rotation_up` (`node`, `up`) or `none`;
  - `look_at` (`offset`, `roll` in degrees, optional `world_up_node`): point local Z at the weighted source position;
  - `rest_rotation`, `rest_roll` (`axis`), `rest_aim` (`axis`): VRM's rotation, roll and aim constraints, driven by
    the first source relative to the rests; `weight` blends from `rest`, `axes` are ignored.
- `target`: the constrained node. `sources`: `{node, weight, offset}`, `offset` a transform.
- `weight` (0 to 1) and `active` (enabled at start); `axes`: the constrained x, y, z axes of the kind's channel
  (position, Euler rotation or scale; a parent constraint's position axes), other axes keep the animated value.
- `rest`: the local values `weight` blends from.
- `options`: `local_space` (match the sources' local values), `freeze` (hold the target in world space at start),
  `rebake` (on unfreezing, bake offsets that keep the target in place).

## Evaluation

The runtime orders constraints by dependency when it loads them; list order breaks ties. Clips animate a constraint's
`weight`, `active`, a source's weight and `freeze` (`CIVBB_behaviour` curve target `constraint`).

## Rules

The target, every source node and an aim or look-at up node exist; transforms, weights, offsets and axes are finite.
At most 4,096 constraints.
