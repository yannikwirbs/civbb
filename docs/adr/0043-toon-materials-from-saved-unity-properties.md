# 0043: X_toon_material carries Unity's saved material; the importer fingerprints the shader and maps it

Status: accepted
Date: 2026-11-06

## Context

Design doc §10 ("Shaders"): known shader families are fingerprinted by shader name, GUID and source markers, and
versioned tables map their material properties to the native Toon or PBR model. The source package names
`X_toon_material` for this (§9); ADR 0041 left its shape open. Until now the exporter wrote each material's
`_MainTex` and `_Color` as glTF PBR and the shader name in `extras.unityShader`.

Measured on the 13 exports of the 11 local avatar packages (54 materials): 27 materials show
`Hidden/InternalErrorShader` because their shader (lilToon) is not in the exporter's project; Unity still keeps
their saved properties and the material file still names the shader's GUID. Their GUIDs match lilToon's own
`.shader.meta` files (`lts_o` 15 materials, `ltsmulti` 12, `lts_trans_o` 3, `lts_trans` 2, `lts` 2,
`lts_cutout_o` 1 across the projects' materials). Materials keep the properties of shaders they used before:
HAOLAN's lilToon materials still carry UTS2's `_utsVersion`, Little Blackbird's TSF materials Poiyomi's markers.
lilToon materials save every property (`_OutlineWidth` too) whatever their variant, and most leave the render
queue to the shader (-1). The rest: locked Poiyomi (Taro), unlocked Poiyomi without its shader (KohzieBoi), UTS2
without its shader (inuinu), VRChat Mobile Toon Lit, Toon Standard, MatCap Lit and Particles, and TSF.

## Decision

1. **Shape.** Each glTF material carries `extensions.X_toon_material`: `{shader, shader_guid, render_queue,
   keywords, floats, ints, colors, textures}`, the material as Unity saved it. Colours are as stored (sRGB-encoded,
   HDR above 1 allowed), textures are `{index: glTF texture or null, scale_offset: [sx, sy, ox, oy]}` with Unity's
   V-up tiling, `render_queue` -1 means the shader's own. The exporter embeds every PNG or JPEG texture a property
   references and reads `shader_guid` from the material file's `m_Shader` reference when the shader is missing.
   The glTF PBR fields stay as a fallback for other readers.
2. **Fingerprint, in the importer**, first match wins: the shader GUID in lilToon's table (GUIDs and file names
   from lilToon's MIT-licensed `.shader.meta` files; the file name gives cutout, transparent and outline); then a
   known shader name (VRChat Mobile, Poiyomi, lilToon, UTS2, MToon, Standard, Unlit); then, for missing shaders
   only, marker properties (`_lilToonVersion`, Poiyomi's `shader_master_label`/`_ShaderOptimizerEnabled`,
   `_utsVersion`, `_MToonVersion`). A GUID or name wins over markers because of leftover properties. Anything else
   becomes a plain toon with a warning.
3. **Mapping tables** per family, in `gltf_import/unity_material.rs`; features only where the family enables them
   (lilToon's `_UseShadow`, `_UseRim`, `_UseMatCap`, `_UseEmission`, `_UseBumpMap`; Poiyomi's `_MatcapEnable`,
   `_EnableRimLighting`, `_EnableEmission`; UTS2's `_RimLight`, `_MatCap`, `_OUTLINE_*` keywords; Standard's
   `_EMISSION` keyword). Shadow ramps convert to MToon's shift and toony: lilToon's border and blur give
   `shift = border - blur / 2`, `toony = 1 - blur`; UTS2's step and feather `shift = step - feather`,
   `toony = 1 - feather`. lilToon's shadowed albedo is the albedo times the shadow colour by strength. Outline
   widths: lilToon and MToon centimetres, UTS2 millimetres. Families without a documented shadow model (Poiyomi,
   VRChat Mobile, unknown) shade a quarter darker. Transparency: a set render queue decides (>= 2501 blend, >= 2450
   cutout); otherwise the lilToon variant, lilToon multi's `_TransparentMode`, or `_Mode`/`_BlendMode`. lilToon
   with an unknown variant gets no outline (its saved `_OutlineWidth` does not say whether the shader draws one).
4. Standard's metallic-gloss map (metal in R, smoothness in A) has no glTF counterpart and is dropped with a
   warning; VRChat Mobile particles become alpha-blended unlit.

## Consequences

- Shaders missing from the exporter's project still map, and a new family or version is a table change in Rust,
  tested on hand-written saved properties (`tests/unity_material.rs`).
- The mappings are approximations until the exporter renders Unity reference images (design doc §10) and parity
  scores them; ramps, Poiyomi's lighting modes, second shadows, rim widths and matcap blend modes are not modelled.
- Source packages grow by the saved properties (up to about 2,800 floats per TSF material) and every referenced
  texture; the runtime package only keeps textures a mapping uses.
- Revisit lilToon's table when lilToon adds shader files; GUIDs of Poiyomi and UTS2 are not used yet (their
  names and markers suffice on the corpus).
