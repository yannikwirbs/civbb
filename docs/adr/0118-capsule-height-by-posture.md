# 0118: The character capsule's height follows the posture; getting up only where it fits

Status: accepted
Date: 2026-11-28

## Context

ADR 0110 point 2 keeps the capsule 1.6 m high whatever the posture ("crouching and prone do not shrink the capsule
yet"), so a crouching or prone player cannot pass under anything lower than standing height (#154). The design doc
(§7, §18) asks for crouch and prone but gives no heights. The capsule stays vertical (ADR 0110), so a lying body
cannot be long and flat.

## Decision

1. `Body` carries a `height` before scale; `Posture::height` sets it: 1.6 m standing, 1.0 m crouching, prone twice
   the radius (0.5 m, a ball). Radius, step and jump are unchanged by posture; height, radius and step scale with
   `Body::scale` as before (ADR 0114).
2. Lowering the posture always succeeds. Raising it (prone to crouching or standing, crouching to standing) happens
   only when the taller capsule at the same feet overlaps no world triangle by more than the 1 cm ground skin
   (`WorldMesh::fits`); otherwise the posture stays and the toggle is dropped, not queued.

This supersedes the last sentence of ADR 0110 point 2.

## Consequences

- A prone player crawls under anything above 0.5 m, a crouching one under 1.0 m; standing up beneath them does
  nothing until the player moves out.
- The prone ball fits through gaps a lying body would not (narrow tunnels 0.5 m wide); a horizontal capsule along
  the facing comes with Jolt or when worlds need it.
- A blocked getting-up asks again only on the next toggle; holding it or trying an intermediate posture is not done.
