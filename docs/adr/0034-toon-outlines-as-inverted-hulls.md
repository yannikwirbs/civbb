# 0034: Toon outlines are inverted hulls drawn as a second indirect draw per outlined submesh

Status: accepted
Date: 2026-11-04

## Context

Most lilToon, Poiyomi and MToon avatars draw outlines, and `format::material::Toon` carries `outline_width`
(metres, 0 for none), `outline_color` and `outline_width_tex` (VRM imports world-space MToon outlines and drops
screen-space ones with a warning). Parity against Unity (ADR 0033, #37) cannot pass without them. The forward pass
(ADR 0023) binds nothing per draw: draws are records and indirect commands sorted into runs of one pipeline.

## Decision

**Inverted hull.** An outlined toon submesh adds a second draw right after its own (`MeshData::draws`), with the
same record, vertices and material but the material's outline key: `Shader::Outline`, blended when the material
is, culling front faces. Its vertex shader (`outline_vertex`) pushes each skinned vertex out along its normalised
skinned normal by the width; only the hull's back faces survive culling, so the mesh shows a rim of them where it
meets the background or other geometry. Its fragment shader returns the outline colour with the base colour's
alpha and cutoff. Outline keys sort after every other opaque key (index 12 to 15), so outlines are drawn after
the meshes they surround and mostly fail the depth test inside them. `PipelineKey::COUNT` goes from 12 to 16.

**Width.** `outline_width` metres in world space after skinning, times the G channel of `outline_width_tex` (MToon
1.0's channel) sampled at mip 0 in the vertex shader, so the bindless texture set is now visible to the vertex
stage. MToon's world mode pushes in object space; the two agree at unit scale, which is what VRM imports are.

**Colour.** `outline_color`, unlit: MToon's fixed-colour mode. Its lighting mix (`outlineLightingMixFactor`, 0.x
`_OutlineColorMode`) is not in the format; add it when Unity parity shows the difference.

**Record.** The material record grows from 72 to 84 words: the outline colour and width (one vec4) and the width
texture's slot come after the six existing slots, so their offsets stay.

## Consequences

- Outlines cost a draw per outlined submesh (the viewer reserves twice the submeshes for draws) and a second
  vertex pass over their vertices, all in the same runs; no new pass or buffer.
- Hard-edged meshes (split normals) open gaps at their creases, as in Unity's inverted-hull shaders.
- MToon 0.x reads its width mask from R and lilToon from its own channels; coloured masks would differ from G. The
  format has no channel per slot; revisit with lilToon/Poiyomi material import or when parity shows it.
- Outlines are drawn identically from the source and the compiled avatar, so source-against-compiled parity
  (ADR 0033) only checks that the compiler keeps them (atlased width textures included).
