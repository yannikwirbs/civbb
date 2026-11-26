# 0102: Native JSON shapes, and Unity dialect 1 read through a JSON rewrite instead of copied types

Status: accepted
Date: 2026-11-26

## Context

ADR 0086 point 3 gives the native types JSON shapes for other readers: snake_case, internally tagged enums, named
fields instead of tuples, omitted `None`s, unknown fields rejected. Unity dialect 1 (ADR 0085) is the serde JSON of
the same types at format v25: externally tagged, Rust variant names, tuples. ADR 0085 point 2 says a native change
copies the old type into the importer. Here every enum and several structs change at once: copying would duplicate
most of `civbb_format::animator`, `constraint`, `dynamics` and `face` (and their validation-relevant invariants) in
the importer, frozen and untested apart from the dialect fixtures.

## Decision

1. **Dialect 1 is rewritten as JSON, then parsed with the native types.** `gltf_import/unity_shapes.rs` rewrites each
   `X_*` position that holds a native type (face visemes and eye driver; animator layers, motions, transitions,
   behaviours and parameter kinds; clip constraint bindings; constraints and their world up; dynamics; contacts) into
   the native shape before parsing. Fields dialect 1 carries but the native type lacks (the animator's
   `parameters`, lowered to the state table by `unity_state`) are removed after use. This supersedes ADR 0085
   point 2 for shape-only changes; a change of meaning still forks the type. The frozen fixtures and their goldens
   (`tests/unity_dialect.rs`) still pin what the dialect imports to.
2. **Shapes.** Newtype and tuple variants become named fields: `Motion::Clip { clip }`, `TransitionDest::State
   { state }`, `Target::Humanoid { property }`, `Target::Entry { entry }`, node `{ component }`, mesh `Weight
   { shape }`, `MaterialSlot { submesh }`, `HumanoidProperty::Body { component }`, `ConstraintField::SourceWeight
   { source }`, `WorldUp::Vector { up }`, `ObjectUp { node }`, `ObjectRotationUp { node, up }`. Tuples become
   `BoneNode { bone, node }`, `VisemeBinds { viseme, binds }`, `Remap { from, to }`, `PartSource { part, source }`.
   Names whose snake_case would be ambiguous are spelled out: blend trees `simple_1d`, `simple_directional_2d`,
   `freeform_directional_2d`, `freeform_cartesian_2d`; PhysBone versions `1.0`, `1.1`.
3. **Exceptions.** `HumanBone` stays camelCase: the names are VRM 1.0's (ADR 0085 point 6). A key's `null` tangent
   still reads as a step (+infinity, ADR 0041), as JSON has no infinity. Package metadata (`package.rs`,
   `cost.rs`) is not part of the authored format and keeps its shape.
4. **Recordings follow the format.** Unity ground truth that embeds format JSON (`constraints.json`,
   `vrc_constraints.json`, `vrc_contacts.json`, the animator golden cases) is converted to the new shapes and its
   recorders (`tools/unity-truth`) write them.

FORMAT_VERSION 36.

## Consequences

- Third-party writers get one consistent JSON style; the generated schema (ADR 0086 point 2) describes it.
- The dialect importer holds one small rewriter instead of a frozen copy of the model; it must be extended when a
  native type's shape changes again, and the dialect goldens catch it when it is not.
- Runtime packages are CBOR of the same types, so packages built at v35 are rebuilt (ADR 0087).
