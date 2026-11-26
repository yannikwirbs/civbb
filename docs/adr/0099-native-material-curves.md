# 0099: Material curves drive native fields; Unity's unanimated vector components are baked into materials

Status: accepted
Date: 2026-11-26

## Context

ADR 0089 points 1 and 3 (#120): material curves target native material fields with an `srgb` flag, the Unity name
table moves to the importer per fingerprinted family (ADR 0043), and Unity's "vectors written whole" (ADR 0067
point 2: components no curve animates are 0) becomes explicit constant curves. Constant curves do not reproduce it:
Unity holds those components at 0 from the first frame, also where no state animates the field (recorded,
`material_first_default`), while a slot's default (its Write Defaults value) is the material's value. ADR 0089
also leaves the representation of muscle identifiers and Euler orders open.

## Decision

1. **Target.** `MeshProperty::Material { field: MaterialField, component, srgb }`. `MaterialField` lists the
   material fields as stored: base colour (RGBA), emission, shade, rim and outline colours (RGB), metallic,
   roughness, normal scale, cutoff, shade shift and toony, rim power, matcap strength, the base colour texture's
   `scale_offset` (V down, as stored), light min and max. `srgb` (colour fields only) means keys are sRGB-encoded
   and decoded when applied, Unity's `GammaToLinearSpace` (above 1 by the power 2.2); alpha is never encoded.
2. **Override semantics** (ADR 0067 point 1 as the native rule): a curve overrides the field on every submesh of the
   mesh instance whose material has it; the default is the first submesh material's value (sRGB-encoded for an
   `srgb` curve), 0 when that material lacks the field. Components without curves are not touched.
3. **Unity lowering** (`gltf_import/unity_clip/material.rs`, names in `unity_material.rs`): a property name maps to
   a field by the family of the first submesh material whose family reads it (unknown families and non-Unity
   materials read every name); colours get `srgb`; smoothness keys become roughness (`1 - v`, tangents negated);
   a vector name needs a component the field has (`_EmissionColor.a` is dropped and reported). Unity's V-up offset
   becomes the stored `1 - v_scale - v_offset`, combined per clip from that clip's V scale and V offset curves (a
   missing one is its default where another clip animates it, else 0); both keyed at different times keeps the
   scale's curve with the offset's default and reports the drop.
4. **Unanimated components are baked**, replacing ADR 0089 point 3's constant curves: for each mesh and field some
   clip animates, every component no clip animates is written into the materials of that mesh's submeshes that
   have the field, at Unity's value (0; Unity's V offset is kept when its V scale becomes 0). A material another
   mesh also uses is copied first.
5. **Muscles** are named by `HumanoidProperty::Muscle { bone, axis }` (the bone's muscle frame axis, 0 twist),
   following the bone names (#122). **Euler curves** carry an `EulerOrder`; `Zxy` (Unity's) is the only order until
   an importer needs another.

## Consequences

- The runtime binds material fields without names or unit conversion beyond `srgb`; `civbb_anim::material` reads
  and writes stored values only.
- A clip that animates the V scale while another animates the V offset plays the offset with the scale's default,
  where Unity combines the current values. Not seen in the corpus (it animates lilToon light limits only).
- The corpus writer cannot raise material curves back to dialect 1 (names depend on the family, offsets on two
  curves); the generated corpus has none.
- Supersedes ADR 0089 point 3's "vectors written whole to explicit constant curves" and ADR 0025's flat name table.
