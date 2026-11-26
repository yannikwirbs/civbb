# 0101: CIVBB_* extensions: document parts at the root, glTF indices, node flags and toon materials local

Status: accepted
Date: 2026-11-26

## Context

ADR 0086 point 4 names the native extensions: node-level ones for components attached to one node (constraints,
contacts, colliders, chains), root-level ones for document parts, `CIVBB_materials_toon` on materials. Writing the
importer and the corpus writer (#124) settles what that point leaves open:

- Components are referenced by index: curves target a constraint by its index, chains list colliders by index.
  Spread over nodes, those lists need a defined global order (node order, then order within the node) that every
  writer must produce and every reader rebuild, and a writer whose document is in another order must renumber.
  VRM draws the same line: `VRMC_node_constraint` is node-level (nothing references a constraint), while
  `VRMC_springBone` keeps springs and colliders at the root because springs reference colliders.
- A writer knows glTF indices, not ours: the importer renumbers nodes (scene traversal, a synthetic root) and makes
  one format mesh per node carrying a mesh.
- Some fields have no glTF home: a node's activity, a mesh's first-person visibility, a material's draw order.

## Decision

1. **Root-level parts.** Each is the format type's JSON (ADR 0086 point 3) plus `"version"` (ADR 0087):
   `CIVBB_avatar` (the avatar role: humanoid, viewpoint, face, expressions, meta; its presence is the role),
   `CIVBB_state` (state table), `CIVBB_behaviour` (`animator`, `clips`), `CIVBB_constraints` (`constraints`),
   `CIVBB_dynamics` (`chains`, `colliders`), `CIVBB_contacts` (`senders`, `receivers`). Constraints, contacts,
   colliders and chains stay root lists with node fields, superseding ADR 0086 point 4's node-level placement.
2. **Local extensions.** `CIVBB_node` on a node: `active` (default true) and, on a node carrying a mesh,
   `first_person` (default `auto`). `CIVBB_materials_toon` on a material: the toon model plus `order` (default 0);
   name, alpha and double-sidedness stay core glTF. Other models have order 0.
3. **Indices are glTF's.** Nodes are glTF node indices; a mesh reference (a shape bind, a mesh curve target) is the
   glTF node carrying the mesh; textures (toon slots, menu icons) are glTF texture indices; material slot keys are
   glTF material indices. Clips index `CIVBB_behaviour.clips` after any glTF animations; entries, constraints and
   colliders index their own lists. The importer maps every reference and fails on one that does not resolve.
4. **One source kind.** A file carries VRM, Unity dialect 1 (`X_*`) or `CIVBB_*` extensions, never two.
5. **Versions.** Each extension object has `"version": 1`; the importer refuses a missing or newer one by name.
   Migrations and frozen fixtures follow with ADR 0087 (#125).

## Consequences

- One global order for every component kind, the one the document already has; no renumbering in writers.
- The corpus writer (`civbb_corpus::gltf::to_source_glb`) writes `CIVBB_*` instead of dialect 1, so the corpus
  round trip tests the native path. Unity dialect 1 keeps its frozen fixtures (`tests/unity_dialect.rs`) and the
  real exports; nothing in the repository writes it any more.
- A glTF editor that drops unknown extensions keeps nodes but loses components, as with VRM's spring bones; the
  Blender add-on reads and writes the extensions itself.
