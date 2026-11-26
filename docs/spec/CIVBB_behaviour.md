# `CIVBB_behaviour`

Root-level. The document's behaviour: a layered state machine (ADR 0085 point 4) and the clips it plays. Schema:
[`schema/CIVBB_behaviour.schema.json`](schema/CIVBB_behaviour.schema.json).

## Fields

- `animator` (optional; absent when nothing plays):
  - `groups`: layer groups, each weighted as a whole (`name`, initial `weight`).
  - `layers`: in evaluation order. A layer has a `group`, a `weight`, `blending` (`override` or `additive`), an
    optional `mask` (body parts and/or nodes) and a `state_machine`: `states`, `default_state`, `entry_transitions`
    and `any_state_transitions`, flattened (sub-state machines are inlined by the writer).
  - A state plays an optional `motion` (`{"type": "clip", "clip": n}` or a blend tree: `simple_1d`,
    `simple_directional_2d`, `freeform_directional_2d`, `freeform_cartesian_2d` or `direct`, children nesting further
    motions), has `transitions`, optional speed, cycle offset, time and mirror parameters, `actions` run on entry and
    an `unwritten` rule: `hold` keeps what the state does not write at its last value, `rest` returns it to its rest
    value (a state without a motion writes nothing, ADR 0065).
  - A transition goes to `{"type": "state", "state": n}` in the same layer or `{"type": "exit"}` (leave through the
    exit, then run the entry transitions again), when all its `conditions` hold (an entry compared by `mode` with a
    `threshold`) and its exit time, if any, has passed; it blends over `duration` and may be interrupted per
    `interruption` (ADR 0066).
  - Actions: `entries` (set, add, random or copy state entries, optionally only for the owner), `pose_source` (body
    parts from tracking or animation), `layer_weight` and `group_weight` (blend a weight to a goal over a duration),
    `locomotion` (enable or disable).
- `clips`: each with a `name`, `length` in seconds, optional root-motion settings and `curves`. A curve has a
  `target` and Hermite `keys` (`time`, `value`, `in_tangent`, `out_tangent`). A `null` tangent is infinite: the
  segment holds its value (a step). Targets and their units (ADRs 0089, 0099):
  - `node`: translation, rotation (quaternion component), Euler rotation, scale or activity of a node;
  - `mesh`: a morph target weight, visibility, a material field or a material slot of the mesh on a node;
  - `humanoid`: a muscle (`bone`, `axis`) or a body position and rotation component;
  - `entry`: a state entry;
  - `constraint`: a field of a constraint (weight, active, a source's weight, freeze).

## Rules

- Clip indices count the file's glTF animations first: clip `n` of a motion is glTF animation `n`, or
  `clips[n - animations]`.
- Every layer names an existing group; every state machine has a state and its `default_state` exists; transition
  destinations, condition, blend, direct, speed, cycle offset, time and mirror parameters, and driver entries
  resolve; drivers do not write platform values. Layer and group actions name existing layers and groups. Mask
  nodes exist.
- Blend trees nest at most 16 deep.
- Clips: `length` is finite and 0 or more. Keys have finite times and values in strictly ascending time. A curve's
  target resolves (node, mesh carried by a node, morph target on that mesh, entry, constraint) and its component
  index fits the property (3 for translation, Euler rotation and scale, 4 for rotation, 3 muscle axes, 7 body
  components, a material field's components; `srgb` only on colour fields). Entry curves do not write platform values.
- Caps: 512 layers, 16,384 states and 65,536 transitions over all layers, 16,384 clips, 500,000 curves and
  10,000,000 keys over all clips.
