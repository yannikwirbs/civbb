# 0068: PhysBone version 1.0 and gravity falloff fitted to the SDK's stored state

Status: accepted
Date: 2026-11-23

Supersedes ADR 0049 point 4's version 1.0 gravity and its falloff reference; the rest of ADR 0049 stands.

## Context

ADR 0049 fitted version 1.0 only at equilibrium (`tan = gravity / pull`); its transients were 1 to 16 cm off, and
advanced integration differed from 1.1 even without gravity (#83). Gravity falloff left 1 mm on version 1.1 (#84) and
1.3 cm on 1.0. Fitting from positions alone was ill-conditioned: one direction per bone per frame, and any term along
the bone leaves the equilibrium unchanged. The SDK keeps per-bone state that reflection can read (the `ApiProbe`
pattern of LESSONS-unity.md): `endPoint`, `prevEndPoint`, `prevVector` (the last offset from the parent) and
`prevVelocity`, which holds the step's displacement before the length constraint in simplified integration and
after it in advanced. A scratch copy of `VrcPhysBones.cs` logged that state per frame for the version 1.0 cases;
regressions against it fit with residuals near 1e-6. Only recorded values are used.

## Decision

`d` is the bone's last offset from its parent (normalised), `len` its length, `r` the step fraction.

1. **Version 1.0, simplified.** Gravity acts across the bone only: `G = g len r (down - (down . d) d)`. It is damped
   like the pull and kept in the velocity: `x = pos + keep u + (1 - keep)(pull (target - pos) + G)`, `u = x - pos`.
2. **Version 1.0, advanced.** After the pull, `x += g len r down` (straight down); stiffness then acts as
   `k / (1 + k)`.
3. **Falloff (both versions).** The cosine compares the bone's last offset turned with its parent bone's turn in
   this step (from the parent's last offset to its new one; the chain root's children unturned) with the animated
   rest offset.

## Consequences

Every version 1.0 case replays within 0.25 mm at 60 Hz (were 1 to 16 cm) and at 30 Hz; falloff within 0.05 mm
(versions 1.0 and 1.1). 90 Hz keeps the partial output step's residuals (#84). VRM springs map to version 1.0
simplified, so their gravity now acts across the bone: same rest angle, different swing; VRM's own gravity is a
plain force, which no recording here pins. Four of 96 PhysBones in the local corpus are version 1.0, all in an
unexported prefab, so the exports' parity does not change.
