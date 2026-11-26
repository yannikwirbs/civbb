# `CIVBB_avatar`

Root-level. Its presence makes the document an avatar (ADR 0085 point 5); without it the document has no role.
Schema: [`schema/CIVBB_avatar.schema.json`](schema/CIVBB_avatar.schema.json).

## Fields

- `humanoid` (optional): `bones`, a list of `{bone, node}` mapping VRM 1.0 humanoid bones (camelCase) to glTF nodes;
  `muscle_frames`, `human_scale` and `center_of_mass` describe the rig in muscle space (ADRs 0042, 0063). Absent for
  a non-humanoid avatar.
- `viewpoint`: the eye position in the avatar's root space, metres.
- `face` (optional): `visemes` (`{viseme, binds}` per viseme), `blink` binds and `eyes` (look maps and a driver,
  bones or shapes). A bind is `{mesh, shape, weight}`: `mesh` the glTF node carrying the mesh, `shape` its morph
  target index.
- `expressions` (optional): the expression menu; controls name state entries and may carry an icon (a glTF texture
  index).
- `meta` (optional): authors, version, licence, references, contact.

## Rules

- Every humanoid bone is mapped at most once and to an existing node. `human_scale` is finite and 0 or more.
- A muscle frame names a mapped bone, at most once per bone; `pre` and `post` are unit quaternions; `sign` components
  are 1 or -1; `min` is at most 0 and `max` at least 0 per axis; `twist` is in 0 to 1 on a limb bone with a twist
  child and exactly 1 elsewhere.
- Each viseme is mapped at most once. Every bind's node carries a mesh and its `shape` exists on that mesh. Eye bones
  are existing nodes.
- Menu controls write state entries (`parameter`, `sub_parameters`), which may not be platform values; menus nest
  at most 16 deep and hold at most 4,096 controls in total; an icon is an existing texture.
- `meta`: at most 256 authors and references together; every string at most 1,024 bytes.
