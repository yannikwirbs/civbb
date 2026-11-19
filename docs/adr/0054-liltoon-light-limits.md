# 0054: lilToon's light limits bound the clamped toon light

Status: accepted
Date: 2026-11-19

## Context

ADR 0050 clamped light plus ambient to 1 for the Unity toon families and left lilToon's `_LightMinLimit` and
`_LightMaxLimit` out: with the parity rig's light they made no difference. Cokoro (#82) animates both from a
brightness slider (`07_mat`, a motion-time state in its FX layer). At the animator's default 0 the slider's clip sets
the upper limit to 0.1, so Unity renders Cokoro about ten times darker than ours: rest mean 0.72 against 0.37 with
the limits.

## Decision

1. `Toon::light_min` and `Toon::light_max` in the format (v17): the clamped model lights the shade-to-lit blend with
   `clamp(light + ambient, light_min, light_max)` per channel. lilToon maps its limits (defaults 0.05 and 1); the
   other families use 0 and 1. The bounds ride in a new record vector (108 words).
2. Both are animatable: `_LightMinLimit` and `_LightMaxLimit` bind `Field::LightMin` and `Field::LightMax` (ADR 0025's
   table).
3. lilToon's final `min(colour, albedo * _LightMaxLimit)` after additional lights is not modelled; with one light it
   changes nothing.

## Consequences

- Cokoro's rest mean against Unity drops from 0.72 to 0.37 (front); its emission hue (the emission map squared by
  `_EmissionMainStrength`) and outlines remain.
- VRChat starts animator parameters at their expression-menu defaults (07_mat 0.5, not 0); both our runtime and the
  Unity reference start at the animator defaults, so parity is unaffected, but the runtime differs from VRChat
  (#85).
