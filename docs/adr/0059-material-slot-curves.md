# 0059: Material-slot curves

Status: accepted
Date: 2026-11-20

## Context

Some clips swap a renderer's materials with object-reference curves on `m_Materials.Array.data[n]` instead of
animating properties (#88). Little Blackbird's mesh carries two wing sets and its wing and texture toggles put a
hidden material (Particles/Additive) on one set and a toon material on the other (16 curves; 12 on the Quest
version); inuinu switches its body between PC and Quest skins (4 curves). The exporter dropped object-reference
curves, so these toggles did nothing in our runtime and were the worst parity states (Blackbird 0.80, Blackbird Quest
0.83, inuinu 0.82 mean).

## Decision

1. The exporter writes each material-slot curve as `Property::MaterialSlot(n)` whose keys hold glTF material indices,
   held until the next key (a null out tangent, ADR 0041). A material only a curve uses is written like any other.
   Other object-reference curves stay skipped and counted.
2. Format v20 adds `Property::MaterialSlot(u8)`: slot `n` is submesh `n` of the renderer at the path; key values
   index `Avatar::materials`. `civbb-anim` binds it to `Target::MaterialSlot(mesh, submesh)` with the submesh's own
   material as the default, and leaves curves unbound when the slot does not exist or a key is not a material index.
   Blends between states round to the nearest index.
3. `Island::submesh_materials` gives each submesh's current material for meshes with swaps;
   `MeshData::draws` takes them as a per-submesh override, so a swap may change the pipeline (toon to additive).
4. Compiler stages keep slot numbers and the swapped-in materials: merge leaves such renderers alone (no submesh
   coalescing), atlasing skips their materials and the swapped-in ones, and compaction keeps materials a slot curve
   uses and renumbers its keys.
5. Not modelled: a swapped-in material does not get the renderer's animated material properties (no corpus renderer
   has both).

## Consequences

- Worst-state FLIP against Unity: Blackbird 0.80 to 0.35, Blackbird Quest 0.83 to 0.16, inuinu 0.82 to 0.12; rest
  states unchanged. Blackbird's remaining worst states show the right wing set in a different arm pose.
- Tests: binding and its rejections (`anim/tests/bind.rs`), a per-island swap (`client/tests/frame.rs`), atlasing
  keeping and renumbering a swapped-in material (`pipeline/tests/texture.rs`).
