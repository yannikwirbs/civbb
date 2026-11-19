# 0053: Matcap tint and mask; Poiyomi's matcap weights; lilToon's emission main strength

Status: accepted
Date: 2026-11-19

## Context

After ADR 0051 the toon matcap was a texture, a strength and a blend. Unity reference renders (#82) showed two more
gaps:

- KohzieBoi (Poiyomi 7.3) rendered grey and glossy where Unity shows black. Poiyomi blends the matcap (texture times
  `_MatcapColor` times `_MatcapIntensity`, masked by `_MatcapMask`) with separate weights: `_MatcapReplace` lerps
  towards it, `_MatcapMultiply` multiplies, `_MatcapAdd` and `_MatcapEmissionStrength` add. We added it at
  `_MatcapIntensity`; KohzieBoi replaces at 0.02 and adds nothing.
- lilToon tints its matcap with `_MatCapColor` (alpha scales the blend) and masks it with `_MatCapBlendMask` (Cokoro,
  Yumeka); we used neither. Cokoro's emission sets `_EmissionMainStrength` 1, which multiplies the emission by the
  albedo (orange in Unity, yellow in ours).

## Decision

1. `Toon::matcap_color: Vec3` tints the matcap texture and `Toon::matcap_mask_tex` scales the strength per texel by
   its red channel (format v16). The material record grows to 104 words (a tint vector and a mask slot, after the
   rim shade); the atlas stage treats the mask as a UV-sampled slot.
2. lilToon: tint from `_MatCapColor`, strength `_MatCapBlend` times its alpha, mask `_MatCapBlendMask`.
3. Poiyomi: tint `_MatcapColor` times `_MatcapIntensity`; strength the larger of add (plus matcap emission) and
   multiply, which picks the blend; replace has no counterpart and is dropped; the mask unless inverted.
4. lilToon's `_EmissionMainStrength` lerps the emission colour towards itself times the base colour; the main
   texture's share is left out (the emission map often is the main texture already).

## Consequences

- KohzieBoi's rest mean drops from 0.63 to 0.28 (front); what remains is the arms' pose and its red emission.
- Device tests check the tint and the mask; importer tests the lilToon and Poiyomi mappings.
