# 0051: Toon matcaps add or multiply

Status: accepted
Date: 2026-11-19

## Context

The toon fragment shader added the matcap times its strength (MToon's `_SphereAdd`). VRChat's MatCap Lit shader
(`VRChat-Mobile-MatCapLit.shader`, SDK 3.10.6) multiplies instead: `albedo * vertex colour * matcap * (light +
ambient)`, with no Lambert term. Megumi (static, every material MatCap Lit) rendered white where Unity shows a
greyed white and black hair: the dark hair texture plus a mostly white matcap saturated (#82). lilToon has the same
choice per material (`_MatCapBlendMode`: normal 0, add 1, screen 2, multiply 3); the 13 exports use only add.

## Decision

1. `Toon::matcap_blend: MatcapBlend` in the format (v14): `Add` (the matcap times the strength adds to the lit
   colour) or `Multiply` (`lerp(colour, colour * matcap, strength)`). The flag rides in the material record's
   `rim.w`.
2. The importer maps VRChat MatCap Lit and lilToon's multiply mode to `Multiply`, everything else to `Add`. lilToon's
   normal and screen modes have no counterpart and stay `Add`, as before.
3. Our matcap frame (MToon's, computed in glTF space) already equals MatCap Lit's in Unity space: the exporter's x
   mirror flips MToon's horizontal axis onto VRChat's `cross(view, up)`.

## Consequences

- Megumi's rest mean against Unity drops from 0.40 to 0.08 (front); a device test checks both modes on a grey quad.
- Vertex colours, which the mobile shaders also multiply in, are still ignored; the 13 exports' are white.
