# 0100: When a toon feature is present, and what an absent one means

Status: accepted
Date: 2026-11-26

## Context

ADR 0090 makes the toon model base fields plus optional features (`normal`, `emission`, `matcaps`, `rim`,
`rim_shade`, `shade`, `color_adjust`, `light_clamp`, `outline`) and requires the restructuring (#123) to leave the
renderer's material records byte-identical. It does not say when a lowered material has a feature, what curves on
an absent feature's fields do, or how many matcaps a document may hold. Before #123 every field existed on every
toon material, so a curve on `_EmissionColor` wrote emission even where the source shader had emission off.

## Decision

1. **Presence follows the source.** A family table lowers a feature wherever the source shader has it on (lilToon's
   `_UseEmission`, Poiyomi's `_EnableRimLighting`, ...) or always has it (MToon's rim, emission and outline, whose
   outline then has width 0 when its mode is off). A feature on with neutral values stays present: curves can
   animate it, as they can in Unity.
2. **An absent feature computes nothing, and curves on its fields do nothing** (`civbb_anim::material::set` reports
   the field missing). That matches Unity, where a property of a disabled feature has no effect.
3. **Off encodings in the record.** An absent feature packs the values that compute nothing (no textures, black
   colours, power 0 for the rim). A black rim packs power 0 too, whether present or not: the shader skips the rim
   at power 0, and a black rim added nothing before either.
4. **Matcaps** are validated to at most `MATCAPS` (2, what the shader draws). A second matcap without a first gets an
   off first one (no texture, strength 0) so it keeps its slot.
5. **Draw order.** `Material::queue()` is the alpha pass's base plus `order`; `order` is the lowered Unity queue minus
   that base.

## Consequences

- Records are byte-identical to before for the generated corpus, the Unity dialect fixtures, the toon fixture, the
  13 local exports compiled, and the local VRMs (`crates/pipeline/tests/material_records.rs`); parity run s123 equals
  s122 in every state.
- Generated content that animates a feature must declare it (the corpus toons carry a black emission).
- Raising `MATCAPS` needs the shader to loop over matcaps first.
