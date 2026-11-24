# 0083: Parity thresholds against Unity's renders, from the translation floor

Status: accepted
Date: 2026-11-24

## Context

ADR 0033's traffic light (green mean ≤ 0.02 and p99 ≤ 0.2, red mean > 0.1 or p99 > 0.5) was set for the compiled
avatar against its own import in our renderer, where the only differences are what the compiler stages do. ADR 0078
asks #101 for thresholds a Unity reference (ADR 0048) must meet, decided from data. Against Unity a translated
renderer keeps differences no mapping removes: Unity's texture import, compression and mip filtering against our
KTX2 path, terms Unity computes per vertex and we per pixel (the Mobile shaders' matcap UV), and the rasterisation
of 1-pixel features and edges.

To measure that floor, #101 first removed the causes that were shared or large, each from the shader's source:

- `civbb-client parity` splits each view's FLIP into interior pixels (covered in both images, more than 1 px from
  either avatar's edge) and silhouette pixels (`ViewScore::interior`, `silhouette`). At rest the silhouette is 2 to 11 %
  of the covered pixels and the interior carries nearly all of the error: the rest-state gaps were shading.
- VRChat Mobile Toon Lit and MatCap Lit have no Lambert term (`albedo × (light + ambient)`); we shaded them.
- lilToon's outline colour is `_OutlineColor × _OutlineTex` (usually the main texture); we drew the colour alone
  (format v23, `Toon::outline_tex`).
- UTS2's first shade ramp was mapped with the wrong shift, and its second shade not at all.
- Poiyomi 7.3 lights through `_ToonRamp` (white by default) in every mode but Wrapped: without a ramp texture it is
  unshaded.
- TSF (Little Blackbird) is an unlit matcap times the texture; it was an unknown shader.
- VRChat Toon Standard's emission is always on and scaled by `_EmissionStrength` (1.7 on Cokoro_Quest, which got
  slightly worse with it: its shading is still unmapped, #105).

Rest mean against Unity, before and after (compiled packages, four views averaged):

| export | before | after | worst view mean / p99 | light (below) |
| --- | ---: | ---: | ---: | --- |
| Inuinu | 0.119 | 0.027 | 0.035 / 0.080 | green |
| Blackbird | 0.108 | 0.040 | 0.041 / 0.110 | green |
| HAOLAN_Quest | 0.150 | 0.046 | 0.048 / 0.203 | green |
| Blackbird_Quest | 0.086 | 0.052 | 0.054 / 0.132 | green |
| Megumi | 0.160 | 0.054 | 0.054 / 0.216 | green |
| HAOLAN_PC | 0.057 | 0.057 | 0.060 / 0.304 | yellow |
| Taro | 0.165 | 0.059 | 0.065 / 0.384 | yellow |
| Cokoro | 0.132 | 0.070 | 0.084 / 0.816 | yellow |
| Tessakuchan | 0.109 | 0.075 | 0.082 / 0.445 | yellow |
| Ostgaroa | 0.099 | 0.099 | 0.107 / 0.576 | yellow |
| Yumeka | 0.146 | 0.108 | 0.121 / 0.586 | red |
| KohzieBoi | 0.140 | 0.140 | 0.158 / 0.973 | red |
| Cokoro_Quest | 0.194 | 0.206 | 0.247 / 0.830 | red |

The first five were inspected side by side at 3x: what remains is texture filtering on thin high-contrast features
(arm bands, eyelashes) and single pixels along hair and outline edges. Their worst view is mean 0.054, p99 0.216.
Two leads were measured and ruled out: compiling costs 0.001 to 0.003 against Unity (the uncompiled import scores
Megumi 0.052, HAOLAN_Quest 0.043, Inuinu 0.026), and Unity's import max size (2048, so 4096 textures render
downsized in 8 of 13 exports) moves Megumi by 0.003 and Taro by nothing at this image size.

## Decision

Against a Unity reference, `parity` lights each view with `UNITY_THRESHOLDS` (and records them in the report's
`thresholds`):

- **Green: mean ≤ 0.06 and p99 ≤ 0.25.** The floor above (0.054 / 0.216) with 10 to 15 % headroom: visually
  identical up to texture filtering and edge pixels.
- **Red: mean > 0.12 or p99 > 0.9.** Twice the green mean is a different look over the avatar; a p99 above 0.9
  means errors near 1 over at least 1 % of the pixels, which is a misplaced or missing part (pose states sit at 0.97
  to 0.99, KohzieBoi's missing red matcap at 0.97).
- Yellow between: one material or feature off, or small geometry offsets (PhysBone rest, a face shape).

ADR 0033's thresholds stay for the compiled avatar against its import, the compiler's own proof.

What #101 needs: every export's rest state not red, and every state that is red itemised with its cause and an
issue. Today three rest states are red, each with a cause and an issue in M0.11: Cokoro_Quest (VRChat Toon
Standard's ramp and albedo-coloured shadow, #105), KohzieBoi (Poiyomi's second matcap, replace blend and UV1 masks,
#106), Yumeka (silhouette offsets at tail, hair and skirt hem, likely PhysBone rest, #108). Red states after rest:

| cause | states | issue |
| --- | --- | --- |
| pose differs (mask IoU 0.79 to 0.97); later states inherit it, because states stack | Cokoro and Cokoro_Quest Velocity*, AngularY, Seated, AFK; Yumeka from VelocityX on; Tessakuchan Upright=0; HAOLAN_PC and HAOLAN_Quest InStation=1 | #109 |
| Blackbird's wings in another pose | WingsClosed=0, Expression=1, Texture=1 | #107 |
| material toggles | Cokoro 07_coat, 07_shirt, 07_botoms, 07_mat, 07_monitor, 07_emission | #110 |
| unmapped shading, as at rest | every Cokoro_Quest and KohzieBoi state | #105, #106 |

## Consequences

- Phase 0's visual half is not decided yet: it waits on #105, #106 and #108 (M0.11). The follow-up ADR that closes
  the gate (ADR 0078) comes after them.
- The thresholds are for this rig (one light, ambient 0.3, 640 px, ADR 0048). A different rig, MSAA (#60) or a
  change of texture path moves the floor: re-measure the floor exports before reusing the numbers.
- p99 punishes small geometry offsets more than shading: a state can be yellow by mean and red by p99 from a
  part a few pixels off. The interior/silhouette split and the mask IoU (`--images`) tell the two apart.
- Each shader family's mapping is now argued from its source; the Unity families still unmapped in the corpus are
  only the features in #105 and #106.
