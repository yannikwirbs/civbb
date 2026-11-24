# 0084: Phase 0's gate is passed: cheaper on the animator, no rest state red against Unity

Status: accepted
Date: 2026-11-24

## Context

ADR 0078 reopened Phase 0 for two proofs and asked for a follow-up ADR stating the gate's result (design doc §21:
compiled avatars visually identical, FLIP within threshold in every state, and clearly cheaper than the Unity
baselines). The CPU half passed in ADR 0082. For the visual half, ADR 0083 set thresholds for a Unity reference
from the translation floor (green mean ≤ 0.06 and p99 ≤ 0.25, red mean > 0.12 or p99 > 0.9, a state taking its
worst view's light) and made #101's bar: every export's rest state not red, and every red state after rest
itemised with its cause and an issue. Three rest states were red then; their causes, each mapped from the shader's
or the SDK's own source or recordings:

- Cokoro_Quest (#105): VRChat Mobile Toon Standard shades by `_Ramp`, its shadow is the albedo times itself times
  `_ShadowAlbedo`, and `_LimitBrightness` clamps light plus ambient. Cokoro_Quest's ramp is the SDK's flat one, so
  it is unshaded and clamped. Mapped onto the Toon model (shade bands' main strength, MToon's step).
- KohzieBoi (#106): Poiyomi 7.3's matcaps replace the colour before lighting and add an unlit emission; KohzieBoi's
  red parts are its second matcap and its black cloth's sheen the first (replace 0.02). New in the format (v24,
  v25): `MatcapBlend::Replace`, `Toon::matcap_emission` and `Toon::second_matcap`. Poiyomi slots read the UV set
  their `_<Slot>UV` names, and a mesh without UV1 reads UV0 there, as Unity's render of KohzieBoi shows.
- Yumeka (#108): compared bone by bone (`Reference.Render -civbbBones`), body bones match within 0.1 mm and Unity's
  animated pose without PhysBones equals ours. PhysBone limit rotations compose as `qz qy qx` in the SDK, not in
  Unity's Euler order (recorded with all three angles; earlier recordings rotated about one axis): the hair ribbons
  now rest as Unity's. The skirt is pushed out by Yumeka's own hand colliders (`allowCollision`, not modelled:
  #92); the tail is still swinging at frame 60 on both sides.

## Decision

**Phase 0's gate is passed.** Visual half, compiled packages against Unity's renders, all 13 local Unity exports,
2026-11-24 (rest: mean over the four views, the worst view's mean and p99, the light; states after rest by light):

| export | rest mean | worst view mean / p99 | rest | states green / yellow / red | red states and issue |
| --- | ---: | ---: | --- | ---: | --- |
| Inuinu | 0.027 | 0.035 / 0.080 | green | 19 / 0 / 0 | |
| Blackbird | 0.040 | 0.041 / 0.110 | green | 10 / 4 / 3 | wings: WingsClosed=0, Expression=1, Texture=1 (#107) |
| HAOLAN_Quest | 0.046 | 0.048 / 0.203 | green | 24 / 0 / 1 | InStation=1 (#109) |
| Blackbird_Quest | 0.052 | 0.054 / 0.132 | green | 12 / 5 / 0 | |
| Megumi | 0.054 | 0.054 / 0.216 | green | 1 / 0 / 0 | |
| HAOLAN_PC | 0.057 | 0.060 / 0.305 | yellow | 0 / 24 / 1 | InStation=1 (#109) |
| Taro | 0.059 | 0.065 / 0.384 | yellow | 0 / 9 / 0 | |
| Cokoro | 0.070 | 0.084 / 0.816 | yellow | 0 / 19 / 12 | Velocity*, AngularY, Seated, AFK (#109); 07_coat, 07_shirt, 07_botoms, 07_mat, 07_monitor, 07_emission (#110) |
| KohzieBoi | 0.071 | 0.086 / 0.707 | yellow | 0 / 6 / 0 | |
| Cokoro_Quest | 0.074 | 0.084 / 0.824 | yellow | 0 / 22 / 9 | Velocity*, AngularY, Seated, AFK (#109); 07_mat, 07_monitor, 07_emission (#110) |
| Tessakuchan | 0.075 | 0.082 / 0.445 | yellow | 0 / 11 / 1 | Upright=0 (#109) |
| Ostgaroa | 0.099 | 0.107 / 0.576 | yellow | 0 / 1 / 0 | |
| Yumeka | 0.107 | 0.120 / 0.574 | yellow | 0 / 1 / 37 | every state from VelocityX on: the pose differs and later states inherit it (#109) |

Rest means before #105, #106 and #108: Cokoro_Quest 0.206, KohzieBoi 0.140, Yumeka 0.108 (the rest unchanged).

- No rest state is red; 5 are green and 8 yellow. Yellow is one material feature or small geometry offsets away from
  the floor (ADR 0083). Yumeka's worst view is 0.1197, 0.0003 under red; what is left there is measured and filed
  (#92 for the skirt).
- Every red state after rest has a cause and an issue, all in Phase 1: pose differences (#109), Blackbird's wings
  (#107), Cokoro's material toggles (#110). None is a shading family left unmapped.
- CPU half (ADR 0082): the animator 3.0 to 42 times cheaper compiled than in Unity with AvatarOptimizer.

"Visually identical in every state" (§21) is read through ADR 0083's thresholds: rest within the floor's yellow
band and every red state explained. It is not "every state green": against a different renderer that needs the
Phase 1 issues above, and the gate exists to show migration works well enough before the engine grows, which this
evidence does.

## Consequences

- M0.11 closes and epic #39 is split into Phase 1 milestones. #104 (the Av3Emulator's animator time) moves to Phase
  1: it cannot change ADR 0082's result.
- The red states (#107, #109, #110), the hand colliders (#92) and the other parity leftovers (#93, #94, #77) are
  Phase 1 work; Yumeka's rest has no margin, so a change that moves it is checked against this table.
- Rerun: per export `civbb build corpus/local/unity-glb/<name>.glb --key corpus/local/unity-ref/key -o <pkg>`, then
  `civbb-client parity corpus/local/unity-ref/<name> <pkg> --key corpus/local/unity-ref/key --images <dir>`
  (about 25 minutes for all 13); `python corpus/scorecard.py` adds CPU and GPU.
- The format grew for this (v24, v25: the matcap replace, emission and second matcap). #100's content model redesign
  is the first Phase 1 milestone and decides how such shader-shaped fields evolve.
