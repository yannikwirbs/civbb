# 0057: lilToon's shadow ramp, shadow bands, rim ramp and emission main strength

Status: accepted
Date: 2026-11-20

## Context

After ADRs 0055 and 0056 the poses match Unity, and the material residuals (#87) were the largest FLIP error against
the Unity references. Yumeka scored 0.53 at rest (front): its clothes rendered washed out. Reading lilToon 2.4.1's
`lil_common_frag.hlsl` against our mapping showed four gaps, all in lilToon, the most used family in the corpus:

- The first shadow ramps half-Lambert `(n . l + 1) / 2` from `_ShadowBorder - _ShadowBlur / 2` to
  `_ShadowBorder + _ShadowBlur / 2` (both clamped to 0 to 1). The importer put MToon's shift at the ramp's start
  in `n . l` terms, which lit most of each shadowed surface (HAOLAN's border 0.5 started lighting at `n . l` -0.5
  instead of 0).
- The rim (`_UseRim`: HAOLAN, Tessakuchan, Yumeka) ramps the fresnel term by `_RimBorder` and `_RimBlur`, fades it
  in shadow by `_RimShadowMask` and tints it by the albedo by `_RimMainStrength`. We added the bare fresnel term:
  Yumeka's clothes (`_RimFresnelPower` 0.2, border 0.997) got their rim colour over the whole surface.
- The shadow (HAOLAN, Yumeka) mixes in by `_ShadowStrength` after its colour is built: a second shadow colour below
  its own ramp (`_Shadow2nd*`), the albedo multiplied in by `_ShadowMainStrength`, a clamp to the lit colour, and
  `_ShadowBorderColor` towards the lit colour over a ramp `_ShadowBorderRange` wider than the first.
- `_EmissionMainStrength` multiplies the emission by the albedo, main texture included (Cokoro's emission map is its
  main texture, so Unity squares it to orange); we multiplied by `_Color` only.

## Decision

1. The importer maps lilToon's first shadow ramp `[lo, hi]` exactly into MToon's terms: shift `1 - lo - hi`, toony
   `1 - (hi - lo)`.
2. Format v18 adds three toon fields: `rim_ramp: Option<RimRamp>` (border, blur, shadow mask, main strength),
   `shade_bands: Option<ShadeBands>` (strength, main strength, second colour, strength, border and blur, border
   colour and range; `shade_color` is then the full shadow colour, without the strength), and
   `emissive_main_strength`. `None` and 0 leave the other families' shading as it was (their scores did not move).
3. The material record grows to 128 words. The shader builds the shadow as lilToon does (second band, main strength,
   clamp, border colour, strength) before the existing lit-to-shadow blend, ramps and masks the rim with the first
   shadow's blend factor, and multiplies the emission towards the albedo.
4. Not modelled: the rim's light direction terms and its multiplication by the light colour (1 under the reference
   rig's clamped light), the third shadow, shadow and rim masks and textures, normal strengths, anti-aliased ramps,
   `_EmissionFluorescence`.

## Consequences

- Rest FLIP against Unity (front / left / back / right): Yumeka 0.53 / 0.57 / 0.56 / 0.53 to 0.14 / 0.14 / 0.15 /
  0.15; Tessakuchan 0.21 / 0.20 / 0.17 / 0.20 to 0.10 / 0.12 / 0.09 / 0.12; HAOLAN 0.17 / 0.22 / 0.27 / 0.18 to
  0.14 / 0.11 / 0.11 / 0.11; Cokoro 0.20 / 0.22 / 0.18 / 0.22 to 0.17 / 0.20 / 0.16 / 0.20. Every other export
  scores exactly as before.
- Device tests check the bands (second shadow, main strength, strength, the clamp) and the rim ramp with emission
  main strength on grey quads; the shadow border colour is checked only by the scores.
- Animating `_ShadowColor` now writes the full shadow colour without `_Color`, closer than before (which also lost
  the strength).
