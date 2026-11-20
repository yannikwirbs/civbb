# 0052: lilToon rim shade, emission gradients and Unity's HDR colour conversion

Status: accepted; HDR conversion (point 3) for `[HDR]` properties and the reference's sample time (point 2) superseded by 0062
Date: 2026-11-19

## Context

Ostgaroa (lilToon Multi, static, no animator) is the cleanest shading comparison against Unity (#82). Three gaps
showed in its side-by-sides:

- Every material enables lilToon's rim shade (lilToon Multi's `AUTO_KEY_VALUE` keyword): after lighting, the colour
  is multiplied towards `_RimShadeColor` by `ramp((1 - |n . v|)^power)`. With black and the default border and blur,
  a surface shows about `|n . v|` of its colour; our spiky bones rendered about twice as bright. Yumeka uses it too.
- Its emission uses lilToon's gradient (`_EmissionUseGrad`): the emission is multiplied by a 1D texture sampled at
  `_EmissionGradSpeed * time`. We ignored it, so an HDR emission colour of 6 to 32 turned the emissive panel white.
- HDR colours: `Mathf.GammaToLinearSpace` raises values above 1 to 2.2 (probed in Unity 2022.3: 32 gives 2048), while
  our `to_linear` continued the sRGB curve (3617). ADR 0025 had left HDR conversion open.

## Decision

1. `Toon::rim_shade: Option<RimShade>` in the format (v15): colour, strength, fresnel power, border and blur as
   lilToon names them. The shader applies it after the light model, before rim light, matcap and emission, as lilToon
   does; the ramp's anti-aliasing width, the mask and the normal strength are not modelled. The material record
   grows to 92 words (rim shade colour and strength; ramp start, end and power).
2. With `_EmissionUseGrad`, the importer multiplies the emission colour by the gradient at `u = 0` (a repeating
   bilinear sample there: the first and last texels averaged). Our runtime has no shader time; Unity's reference
   renders sample a few frames after entering play mode, near the start too, though not exactly.
3. `to_linear` and `to_srgb` follow Unity above 1: `c^2.2` and `l^(1/2.2)`.

## Consequences

- Ostgaroa's rest mean against Unity drops from 0.37 to 0.23 (front), 0.29 to 0.17 (right). Remaining: the
  emissive panel is still brighter and bluer than Unity's (the gradient's sample time differs), and lilToon's
  `_MainTexHSVG` tone correction is not modelled.
- Animated emission (gradients, blink, scroll) stays static; a time input for materials is a renderer feature for
  later.
- Device tests check rim shade on a grey quad; importer tests check the mapping and the gradient sample.
