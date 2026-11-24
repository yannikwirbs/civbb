# 0082: Phase 0's gate, CPU half: compiled avatars are clearly cheaper than Unity's animator

Status: accepted
Date: 2026-11-24

## Context

ADR 0078 reopened Phase 0 for two proofs and asked for a follow-up ADR stating the gate's result. This is the CPU
half (#38); the visual half is #101, still open. Measured with ADR 0080 (our worker pinned to a performance core)
and ADR 0081 (Unity in Editor play mode, job workers off, confined to the same core; per avatar = `PlayerLoop` less
the 0-copy frame, divided by the copies). `python corpus/scorecard.py`, 2026-11-24, all 13 local Unity exports,
compiled packages, CPU µs per avatar at 1 / 40 copies:

| export | CIVBB | Unity | Unity + AAO | CIVBB animator | Unity animator | Unity + AAO animator |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Blackbird | 23.1 / 21.8 | 467.1 / 387.1 | 454.5 / 385.3 | 8.2 / 8.1 | 52.4 / 48.2 | 47.8 / 44.9 |
| Blackbird_Quest | 10.5 / 7.2 | 64.1 / 30.6 | 56.0 / 29.3 | 6.3 / 4.9 | 41.2 / 27.1 | 38.0 / 25.8 |
| Cokoro | 43.8 / 40.7 | 473.8 / 394.0 | 404.4 / 346.0 | 27.1 / 26.5 | 111.6 / 109.8 | 79.4 / 84.3 |
| Cokoro_Quest | 46.4 / 39.9 | 457.5 / 368.2 | 416.9 / 325.1 | 30.3 / 26.0 | 110.0 / 105.5 | 80.7 / 78.7 |
| HAOLAN_PC | 83.5 / 82.0 | 1997.2 / 1884.2 | 1684.1 / 1575.1 | 15.5 / 14.2 | 81.5 / 75.8 | 79.9 / 77.8 |
| HAOLAN_Quest | 35.7 / 34.9 | 561.8 / 500.9 | 500.7 / 450.2 | 13.5 / 12.9 | 72.9 / 74.4 | 67.5 / 76.3 |
| Inuinu | 29.7 / 28.0 | 403.5 / 341.9 | 399.5 / 334.6 | 14.2 / 13.7 | 60.0 / 52.5 | 46.5 / 43.4 |
| KohzieBoi | 5.4 / 4.8 | 95.6 / 20.9 | 89.8 / 17.2 | 2.3 / 2.3 | 26.9 / 14.4 | 28.0 / 12.9 |
| Megumi | 4.8 / 3.9 | -1.5 / 1.6 | 9.1 / 1.7 | 0.1 / 0.1 | 0.0 / 0.0 | 0.0 / 0.0 |
| Ostgaroa | 43.1 / 43.9 | 1370.3 / 1297.2 | 1332.5 / 1254.6 | 0.1 / 0.1 | 0.1 / 0.0 | 0.0 / 0.0 |
| Taro | 25.0 / 23.1 | 592.2 / 528.8 | 589.0 / 496.4 | 0.7 / 0.6 | 25.7 / 23.2 | 30.1 / 25.5 |
| Tessakuchan | 17.3 / 17.7 | 319.5 / 255.9 | 289.4 / 240.3 | 6.8 / 8.0 | 49.7 / 44.1 | 53.1 / 49.0 |
| Yumeka | 186.8 / 191.2 | 4351.3 / 4200.3 | 4260.3 / 3843.4 | 17.1 / 18.5 | 167.7 / 141.0 | 170.3 / 124.0 |

AAO is Anatawa12's AvatarOptimizer 1.9.23 (Trace and Optimize through NDMF 1.15.1). It cut shapes (Yumeka 563 to
113), transforms (HAOLAN_PC 227 to 152, Megumi 331 to 63), skinned meshes and some animator layers (Cokoro 19 to 14);
it removed one PhysBone component in HAOLAN_Quest and in Taro and none elsewhere.

## Decision

The CPU half of Phase 0's gate is **passed on the animator, the part that can be compared**:

- Every export with an animator (11 of 13) runs its animator 3.0 to 42 times cheaper compiled than in Unity with
  AvatarOptimizer at 40 copies (Cokoro_Quest 26.0 against 78.7 µs is the closest; Yumeka 18.5 against 124.0), and
  more at 1 copy. Mecanim is engine code, the same in the Editor and the client (ADR 0081), so this is the
  like-for-like comparison.
- Totals are 4 to 30 times lower (Megumi aside), but Unity's totals are mostly the SDK's PhysBones and contacts, which run as
  managed jobs in Unity and not in the client (ADR 0081). They are not used to claim anything about PhysBone cost.
- Megumi (no animator, no PhysBones) costs 3.9 µs per avatar compiled against Unity's 1.6: our fixed per-avatar
  work (the descriptor's colliders as contact senders, ADR 0061, and the world step) has no counterpart in this
  Unity setup. Below 5 µs it does not matter to the gate.

Phase 0's gate as a whole stays not passed until #101 (visual parity against Unity) is decided. #102 (Blackbird's
compiled package costing 33 % more) does not reproduce on this main: source 22.2 / 22.3 µs against compiled 21.5 /
22.5 µs on CPU 11, 23.9 against 23.7 µs on CPU 2; closed.

## Consequences

- #38 closes. The emulator-against-our-setup check is #104; it cannot change this result unless Unity's animator
  were more than three times cheaper under the emulator.
- PhysBone cost against VRChat stays unproven either way; Phase 1's 40-avatar frame budget (design doc) is measured
  on our runtime alone and does not depend on it.
- d4rk's optimizer is not measured (ADR 0081): AvatarOptimizer barely moved Unity's CPU (it targets draws and
  shapes), and the gap to our animator is too large for another optimiser of the same kind to close.
