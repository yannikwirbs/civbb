# 0041: The source package's X_* extensions carry the format model as JSON, with glTF node indices

Status: accepted
Date: 2026-11-06

## Context

The source package (design doc §9) is glTF plus `X_avatar`, `X_animator`, `X_dynamics`, `X_contacts`,
`X_constraints`, `X_expressions` and `X_toon_material`. The Unity exporter writes them (#36, ADRs 0037, 0038) and
the glTF importer must read them as untrusted input (#71). Each needs a schema, defined once and shared by both
sides. The format crate already models these parts with Unity's semantics and serde derives: animator
(parameters, layer groups, state machines, blend trees, behaviours), clips with Hermite keys and Unity-style
binding paths, constraints, dynamics, contacts and expressions; `civbb_format::validate` checks every index and
cap of an avatar. The format addresses nodes by hierarchy index, which the importer assigns (parents first), not
by glTF node index.

Measured on the five Booth packages imported into Unity 6000.3.28f1 without the VRChat SDK: no prefab's root
`Animator` has a controller (the playable-layer controllers are referenced only by the VRChat avatar descriptor,
a missing script without the SDK, #37); 16 controllers sit in the packages as assets (1 to 26 layers, up to 62
states); HAOLAN PC has two Unity `RotationConstraint`s, the others none.

## Decision

1. Every `X_*` extension but `X_avatar` is the serde JSON of the format type it fills, as `serde_json` writes it
   (field names as in Rust, enums externally tagged, `Option` as `null` or the value, tuples as arrays), placed in
   the glTF root's `extensions`:
   - `X_animator`: `{"animator": Animator, "clips": [Clip]}`. `Motion::Clip` indexes this `clips` list; the
     importer appends it after the glTF animations' clips and shifts the indices.
   - `X_constraints`: `[Constraint]`, `X_dynamics`: `Dynamics`, `X_contacts`: `Contacts`, `X_expressions`:
     `Expressions`.
   - Every `NodeIndex` field in them is a glTF node index; the importer maps it to its hierarchy index and fails
     the import when one is out of range or outside the scene.
   - Values are in glTF space (ADR 0038's X mirror and ADR 0039's curve mirroring happen in the exporter).
2. `X_avatar` is its own small schema, because its Unity and VRM sources are maps rather than format types:
   `{"humanoid": {"<HumanBodyBones name>": <glTF node>}, "viewpoint": [x, y, z]?}`. The viewpoint is in avatar
   space; without one it is the head's world position plus 0.06 m up, as VRM's default. Visemes and eye look join
   it with the avatar descriptor (#37).
3. A node's `extras.active: false` marks it inactive at rest (the exporter writes it for inactive GameObjects).
4. Bounds: the extensions are parsed with the rest of the glTF JSON, whose size the input limit already caps;
   deserialisation allocates at most in proportion to that JSON; then the import's `validate` call checks every
   cap and index as for VRM input. A malformed extension fails the import with its name in the error rather than
   being skipped: unlike VRM, the exporter is ours, so a mismatch is a bug to see.
5. The format version (`civbb_format::FORMAT_VERSION`) versions these schemas too. A format change that renames or
   removes a field supersedes this ADR's schema for that version; the importer then keeps reading the older shape
   (kept sources must recompile, design doc §9).
6. Until the exporter reads the avatar descriptor (#37), `Batch.Export -civbbLayers Base=<controller>;FX=<...>`
   names the controllers that fill the layer groups, in that order; it also serves Unity avatars without
   VRChat components.

## Consequences

- One schema definition, the Rust types, and no second model to keep in step; the round trip of the generated
  corpus through `to_glb` and `import` tests the whole path. The C# writer must emit serde's exact shapes, which
  the exporter's runs on the Booth corpus check by importing the result.
- The source format follows the format model's versions; old sources need migrations when the model changes
  (point 5). Revisit if the model starts carrying compiled-only data a source should not hold.
- Face data (`Face`) indexes format meshes, which the importer creates per glTF node; it waits for `X_avatar`'s
  visemes and eye look, which will address meshes by glTF node.
