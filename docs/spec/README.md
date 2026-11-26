# CIVBB content format: the `CIVBB_*` glTF extensions

Normative prose for the native content format (ADR 0086). A source document is a glTF 2.0 file (`.gltf` or `.glb`)
whose core carries nodes, meshes, skins, PBR materials, textures and animations, and whose `CIVBB_*` extensions carry
the rest. The shapes are defined by the generated JSON Schemas in [`schema/`](schema/) (2020-12, field descriptions
included); this text defines meaning and the rules a schema cannot state. Where the two disagree, the Rust types in
`crates/format` decide and both documents are fixed.

The `CIVBB_` prefix is provisional until the platform's final name (#126).

## Extensions

| Extension | On | Holds | Spec |
| --- | --- | --- | --- |
| `CIVBB_avatar` | root | the avatar role: humanoid, viewpoint, face, expressions, meta | [CIVBB_avatar.md](CIVBB_avatar.md) |
| `CIVBB_state` | root | the state table | [CIVBB_state.md](CIVBB_state.md) |
| `CIVBB_behaviour` | root | the layered state machine and its clips | [CIVBB_behaviour.md](CIVBB_behaviour.md) |
| `CIVBB_constraints` | root | transform constraints | [CIVBB_constraints.md](CIVBB_constraints.md) |
| `CIVBB_dynamics` | root | simulated bone chains and their colliders | [CIVBB_dynamics.md](CIVBB_dynamics.md) |
| `CIVBB_contacts` | root | contact senders and receivers | [CIVBB_contacts.md](CIVBB_contacts.md) |
| `CIVBB_node` | node | activity, first-person visibility | [CIVBB_node.md](CIVBB_node.md) |
| `CIVBB_materials_toon` | material | the toon material model and draw order | [CIVBB_materials_toon.md](CIVBB_materials_toon.md) |

## Rules for every extension

1. **Version.** Every extension object has `"version"`, currently `1`. A reader refuses an object without one or with
   a version newer than it reads, naming the extension (ADR 0087).
2. **Unknown fields fail.** A field the extension's version does not define fails the import, at any depth.
3. **One source kind.** A file carries `CIVBB_*`, VRM (`VRMC_*`) or Unity dialect 1 (`X_*`) extensions, never two
   kinds (ADR 0101 point 4).
4. **Absent means default.** Optional fields are omitted rather than `null`; each field's default is stated in its
   schema description or here. The one `null` with a meaning is a key tangent (`CIVBB_behaviour`).
5. **Indices are glTF's** (ADR 0101 point 3). Nodes are glTF node indices in the scene. A mesh reference (a shape
   bind, a mesh curve target) is the index of the glTF node carrying the mesh. Textures are glTF texture indices.
   Material slot keys are glTF material indices. Clips index `CIVBB_behaviour.clips` after the file's glTF
   animations. State entries, constraints, colliders, layers, groups and states index their own lists. An index that
   does not resolve fails the import.
6. **Units and space.** Metres, seconds, weights 0 to 1, linear colour unless a field says sRGB, angles in the unit
   the field's description names; glTF's coordinate system (right-handed, +Y up). Quaternions are `[x, y, z, w]`.
   Every number is finite unless a field says otherwise.
7. **Tagged values.** A value of several kinds is an object whose `"type"` names the kind in snake_case, with the
   kind's fields beside it (`{"type": "sphere", "radius": 0.1}`); a kind without fields is still an object
   (`{"type": "scene_up"}`). A plain choice is a snake_case string (`"first_person_only"`). Humanoid bone names are
   VRM 1.0's camelCase names (`leftUpperArm`).

## Validation and caps

After parsing, the compiler validates the whole document (`civbb_format::validate`) and fails with every problem
found, each with a path (`constraints[3]: source node index 40 out of range (12)`). Beyond index resolution:

- Strings are at most 1,024 bytes.
- Names of state entries are unique; humanoid bones, visemes and muscle frames are mapped at most once.
- Content never writes a platform value (`platform/*` entries): no driver, curve, menu control, contact receiver or
  chain output may target one.
- Counts are capped. The defaults (`Caps::default`; a platform tier may lower them):

| Cap | Default | Cap | Default |
| --- | --- | --- | --- |
| nodes | 8,192 | state entries | 2,048 |
| meshes | 512 | synced bits | 1,024 |
| vertices | 4,000,000 | layers | 512 |
| indices | 12,000,000 | states (all layers) | 16,384 |
| blend shapes | 16,384 | transitions (all layers) | 65,536 |
| shape deltas | 40,000,000 | clips | 16,384 |
| materials | 512 | curves (all clips) | 500,000 |
| textures | 512 | keys (all clips) | 10,000,000 |
| constraints | 4,096 | blend tree depth | 16 |
| chains | 1,024 | menu depth | 16 |
| colliders | 512 | menu controls | 4,096 |
| contacts (senders + receivers) | 1,024 | points per graded curve | 64 |

## Changing the format

The schemas are generated from the Rust types: `cargo xtask schema` rewrites [`schema/`](schema/), and the gate fails
while they are stale. A change to a type's shape bumps `FORMAT_VERSION`; a change to what an extension means or
accepts also bumps that extension's `version` and adds a migration (ADR 0087).
