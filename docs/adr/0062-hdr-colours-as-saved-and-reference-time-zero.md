# 0062: `[HDR]` colours as saved, and reference renders at shader time 0

Status: accepted
Date: 2026-11-21

## Context

Ostgaroa's emissive panel rendered far brighter than Unity's (#91). Probes in the host project (Unity 2022.3, linear
colour space), with the reference's shader time pinned to 0 so that the gradient sample matched ours:

- With the emission colour set to 1 our panel matched Unity's (FLIP equal to rendering without emission); with the
  saved 5.99 Unity added about 6 times as much light, we 51 times. Texture compression (DXT1), mip generation and the
  gradient's precision changed Unity's panel by at most one level.
- A blit of one value through a probe shader: a property declared `[HDR]` reaches the shader exactly as saved (0.5,
  5.99, 32), a plain colour is converted (0.5 to 0.214, 5.99 to 51.4, 32 to 2048). ADR 0052 point 3 had probed only
  a plain colour.
- `Shader.GetPropertyFlags` lists the `[HDR]` colours: lilToon `_EmissionColor`, `_Emission2ndColor`; UTS2
  `_Emissive_Color`, `_ColorShift`, `_ViewShift`; Poiyomi 7.3 declares `_EmissionColor` `[HDR]` in its source.
  Unity's Standard and VRChat's mobile shaders have none.
- Our runtime has no shader time; the importer samples lilToon's emission gradient at its start (ADR 0052). Unity's
  reference renders ran at play mode's running time (one second per state), so the gradient showed another colour.
  Only Ostgaroa uses a time-driven material effect in the corpus.

## Decision

1. The importer takes `[HDR]` colours as saved (`Props::hdr_rgb`): lilToon's and Poiyomi's `_EmissionColor` and
   UTS2's `_Emissive_Color`, the ones it maps. Plain colours keep ADR 0052's conversion.
2. `Reference.Render` pins shader time to 0 (`_Time`, `_SinTime`, `_CosTime` from the camera's command buffers), so
   the reference shows the frame our static materials model. A time input for materials is #94's question, not the
   reference's.

## Consequences

- Rest FLIP (front, side): Cokoro 0.169 to 0.109, 0.200 to 0.140; Ostgaroa 0.219 to 0.134, 0.150 to 0.116 (worst
  0.165 to 0.123). The other exports render the same at time 0.
- The runtime still converts animated colours as sRGB whatever the property's flag; no export animates an `[HDR]`
  colour (#93).
- Shader families added later need their `[HDR]` colours listed; `GetPropertyFlags` in the host project answers it.
