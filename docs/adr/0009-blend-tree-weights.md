# 0009: Blend tree weights, time sync and weights that do not sum to 1

Status: accepted
Date: 2026-10-28

## Context

#15 asks for blend tree weights matching Unity. Unity documents what each 2D mode is for and that Freeform
Directional uses polar gradient band interpolation from Rune Skovbo Johansen's 2009 thesis ("Automated
Semi-Procedural Animation for Character Locomotion", §6.3), but not the exact maths.

## Decision

- 1D: children ordered by threshold; linear between the two around the parameter, clamped to the ends.
- 2D freeform cartesian: gradient bands (thesis eq. 6.6, 6.7): `h_i = max(0, min_j (1 - p_ip·p_ij / |p_ij|²))`,
  normalised.
- 2D freeform directional: the same in polar space (eq. 6.14, 6.15): magnitude difference over the pair's mean
  magnitude, signed angle times α = 2. The thesis leaves samples at the origin undefined; ours borrows the
  partner's direction, which gives radial bands (`1 - |p| / |p_j|` around the centre).
- 2D simple directional: the two directions around the parameter's angle (wrapping) get barycentric weights;
  the centre child (at the origin) takes the rest; outside that triangle, or with no centre, the two are
  normalised. One direction alone is projected onto.
- Direct: each child's weight is its parameter, divided by the sum of positive weights when "normalised".
- Children share the state's normalised time (plus their cycle offsets). A tree's length is the weighted average
  of its clips' lengths divided by their time scales. A tree loops if any clip does.
- Children with weight 0 or below play nothing and write nothing.
- When weights do not sum to 1 (direct trees), the remainder goes to the default value:
  `v = d + Σ w_i (v_i - d)` for Write Defaults on. This matches Playables blending a short weight toward defaults,
  and with Write Defaults off it reproduces the community-measured direct tree feedback `x / (1 - S)` (#16).

## Consequences

- Freeform weights follow the published algorithm; the origin rule, the simple directional rule and the
  remainder rule are unverified until Unity recordings (M0.10).
