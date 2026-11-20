# 0058: lilToon's tone correction of the main texture

Status: accepted
Date: 2026-11-20

## Context

lilToon corrects the main texture's tone (`_MainTexHSVG`: hue shift, saturation, value, gamma) right after sampling
it, before `_Color` and before the shadow takes the albedo (`lilToneCorrection` in `lil_common_functions.hlsl`).
Every lilToon variant applies it; the multi shader only with the `EFFECT_HUE_VARIATION` keyword
(`lil_replace_keywords.hlsl`). In the corpus only Ostgaroa (lilToon Multi) uses it: three materials, value 0.99 to
1.43, gamma 0.91. #87 listed it; its spikes rendered darker than Unity's.

## Decision

1. Format v19: `Toon::tone: Option<[f32; 4]>`, the saved vector as is (a vector property, not linearised). The importer
   sets it when it differs from `(0, 1, 1, 1)`, on the multi shader only with the keyword. The colour adjust mask is
   not modelled.
2. The material record grows to 132 words. The shader ports `lilToneCorrection` and applies it to the base texture
   sample, and to the shadow texture sample when it is the same texture (lilToon's shadow starts from the corrected
   albedo); identity skips it.

## Consequences

- Ostgaroa's rest FLIP against Unity: 0.24 / 0.18 / 0.19 / 0.17 to 0.22 / 0.15 / 0.15 / 0.14. Its dragon heads stay
  darker and its emissive panel brighter than Unity's: both carry an HDR emission times lilToon's emission gradient,
  which our runtime samples at time 0 (ADR 0052) and Unity's reference about a second into play mode.
- A device test checks the value scale on a grey quad; an importer test checks the keyword rule.
