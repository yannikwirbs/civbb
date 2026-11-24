# 0086: A written spec and a JSON Schema generated from the format types, checked in

Status: accepted
Date: 2026-11-24

## Context

#100 goal 4: a written specification and a machine-readable schema that do not depend on our Rust code, and
registered extension names. Today (ADR 0041) the schema is whatever `serde_json` writes for the Rust types: Rust
field names, externally tagged enums, tuples as arrays, `null` for absent options. ADR 0085 makes the native model
the format third parties write (the Blender add-on, converters), so its JSON has to be designed for them.

Options:

1. The Rust types are the schema (status quo). Nothing to maintain; no one else can read the format.
2. A hand-written JSON Schema as the source of truth, the Rust types tested against it. Two definitions that drift.
3. A JSON Schema generated from the Rust types (`schemars`), checked in and compared in the gate, plus a prose spec.
4. An interface language (protobuf, FlatBuffers, KDL schema) generating the code. Fights glTF's JSON.

## Decision

Option 3.

1. **Spec.** `docs/spec/` holds the normative prose, one file per extension: meaning, units, coordinate space,
   evaluation rules and the cross-reference and cap rules `civbb_format::validate` enforces. The schema describes
   shape only.
2. **Schema.** `docs/spec/schema/<extension>.schema.json` (JSON Schema 2020-12) is generated from the Rust types by
   `cargo xtask schema`; the gate fails when the checked-in files differ, like the code map check.
3. **JSON shapes for other readers.** snake_case names; enums as plain strings for unit variants and internally
   tagged objects otherwise (`{"type": "sphere", "radius": 0.1}`); named fields instead of tuples; absent optional
   fields omitted, not `null`; every default stated. Units: metres, radians, seconds, linear colour unless a field
   says sRGB, weights 0 to 1, glTF's coordinate system.
4. **Extension names.** `CIVBB_` prefix (as VRM uses `VRMC_`), provisional until the final name (design doc D9):
   registering a prefix in Khronos's glTF registry is the owner's call (needs-user issue). Components attached to
   one node (constraints, contacts, colliders, chains) are node-level extensions; document-level parts (avatar role,
   state table, behaviour and clips, menu) are root-level; toon materials are `CIVBB_materials_toon` on glTF
   materials, PBR uses core glTF and `KHR_materials_*`.
5. **Bounds** as ADR 0041 point 4: parsed within the input cap, allocations in proportion to the JSON, `validate`
   after. Unknown fields in a native extension fail the import (an extension version says what exists, ADR 0087).

## Consequences

- One definition, the Rust types, still; the checked-in schema makes every shape change visible in review.
- New dependency `schemars` (build-time use in xtask and the format crate's derive), boring and widely used.
- The serde attributes on the format types change (renames, tagging); the runtime package's CBOR follows them.
- The spec is written once the model settles (ADR 0085 point 7), not per intermediate step.
