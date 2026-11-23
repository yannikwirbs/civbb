# 0067: Material curves as recorded in Unity

Status: accepted
Date: 2026-11-23

Supersedes ADR 0025 point 3's default ("the first submesh material that has the field") and its unverified
consequences; the rest of ADR 0025 stands.

## Context

ADR 0025 bound material curves per renderer, field and component, with four choices left to check against Unity
(#54): the default when a renderer's materials differ, the conversion of animated `[HDR]` colours, the `_MainTex_ST`
V offset, and Write Defaults on material properties. `Goldens.cs` (ADR 0065) now builds renderers with materials and
records what each material's shader receives (`material_first_default`, `material_first_lacks`,
`material_write_defaults`; Unity 6000.3 and 2022.3 agree), and a render probe in the SDK host project (2022.3,
linear) read the values a shader sees. Recorded:

- The animator writes the renderer's property block, shared by every material of the renderer, from the first
  frame, under Write Defaults off too: a second material's differing value is overridden at once (as ours did).
- The **default is material 0's value**; when material 0 lacks the property it is **0**, not the next material's.
- A vector property is written **whole**: animating only `_Color.g` sends `(0, g, 0, 0)`; only `_MainTex_ST.w`
  sends scale 0. Ours kept the material's other components.
- Animated colours reach the shader converted from gamma like plain colours **even when the property is `[HDR]`**
  (5 gives 34.493, `pow(x, 2.2)` above 1; 0.2 gives 0.033105); alpha is not converted. A bound `[HDR]` property's
  default is its saved value converted the same way (saved 0.5 renders 0.214 once bound, 0.5 unbound).
- `_MainTex_ST` reaches the shader as animated, in Unity's V-up units; the exporter flips V and both the importer and
  `civbb_anim::material` map offsets with `1 - scale - offset`, so the convention holds.
- Write Defaults applies as to transforms (ADR 0065): a WD-on empty clip resets to the defaults, a WD-on motionless
  state and WD off keep the last value.

## Decision

1. A material slot's default is the value in the renderer's first submesh material, or 0 when that material lacks
   the field; a curve still binds only when some material of the renderer has the field.
2. When any component of a vector field (colours, `_MainTex_ST`) is bound on a renderer, all four components get
   slots; those no curve animates default to 0 and stay there.
3. Animated colours keep `to_linear` for every colour field. The default of an `[HDR]` field (read from a material
   whose value was taken as saved, ADR 0062) still round-trips to the saved value instead of converting it: that
   needs the record to know which fields are `[HDR]` and stays with #93.

## Consequences

Avatars that animate a single colour or texture-offset component now render as in Unity (other components 0). No
export does today: the corpus animates only scalars (Cokoro's lilToon light limits; KohzieBoi's hue shifts, which bind
to nothing). A material 0 without a base colour texture defaults `_MainTex_ST` to 0 where Unity reads the material's
saved scale and offset; no export animates `_MainTex_ST`.
