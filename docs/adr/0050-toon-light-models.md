# 0050: Toon materials name their light model: MToon's additive ambient or the Unity families' clamped light

Status: accepted
Date: 2026-11-19

## Context

The toon fragment shader (ADR 0023) followed MToon: `lerp(shade, lit, s) * light + lit * ambient`. Unity reference
renders (ADR 0048, #79) showed every lilToon, Poiyomi and UTS2 avatar washed out (#81): under the parity rig's light
(1) and ambient (0.3) a lit surface showed 1.3 times its colour. Those families add the ambient (spherical
harmonics) to the main light and clamp the sum before it lights the shade-to-lit blend (lilToon's
`_LightMinLimit`/`_LightMaxLimit`, default 0.05 to 1; Poiyomi and UTS2 combine and saturate likewise), so a lit
surface shows its own colour at most. MToon (VRM) keeps its own model.

## Decision

1. `Toon::light: ToonLight` in the format (v13): `Additive` (MToon) or `Clamped` (lilToon, Poiyomi, UTS2).
2. `Clamped` lights the blend with `saturate(light + ambient)` per channel; rim, matcap and emission add after, as
   before. lilToon's lower limit and each family's exact combination are not modelled: with the rig's light they
   give the same result, and real scenes are not in the parity set yet.
3. The importer marks lilToon, Poiyomi, UTS2 and unknown shaders `Clamped`; Unity's MToon shader, every VRM
   material and VRChat's mobile shaders `Additive`. The mobile shaders were measured: unclamped they score closer to
   Unity (HAOLAN_Quest's Toon Lit rest mean 0.53 clamped against 0.46, Megumi's MatCap 0.45 against 0.40). The flag
   rides in the material record's `shade.w`.

## Consequences

- A device test checks both models on a lit grey quad (0.65 against 0.5).
- The 13 Unity exports are re-scored against their references (#81); remaining gaps are per-family mappings (#82).
- Revisit when world lighting arrives (Phase 1): several lights, light probes and lilToon's limits would need the
  per-family details this ADR leaves out.
