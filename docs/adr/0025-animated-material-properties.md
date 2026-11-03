# 0025: Material curves bind per renderer to native fields; animated renderers get per-island material records

Status: accepted
Date: 2026-11-03

## Context

#49: clips animate material properties by Unity shader property name (`_Color.r`, `_EmissionColor.g`,
`_MainTex_ST.z`, `_Glossiness`), and ADR 0007 point 5 left those curves unbound. Design doc §10 "Shaders" maps known
shader families' properties into the native PBR and Toon models with versioned per-family tables, but packages carry
only the native model, not the source family, and the Unity exporter that would fingerprint families is M0.10. The
doc does not say what an animated material property is bound to, which units curves use, what the default is, or
whether an animated material is shared. In Unity the animator writes a renderer's material property block: the value
applies to that renderer only, to every one of its materials whose shader has the property. ADR 0023 keeps all
material records in one buffer, one record per avatar material, shared by every copy of an avatar.

## Decision

1. **One flat name table in `civbb-anim`.** `material::parse` maps the property names the common families share to
   a native field (`Field`): base colour (`_Color`, `_BaseColor`), emission (`_EmissionColor`, `_Emissive_Color`),
   shade, rim and outline colours (MToon, lilToon, Poiyomi and UTS2 names), metallic, smoothness (`_Glossiness`,
   `_Smoothness`), normal scale, alpha cutoff, MToon shade shift, toony and rim power, lilToon matcap blend and
   `_MainTex_ST`. Vector fields need a component suffix (`rgba` or `xyzw`), scalars none. Unknown names, outline
   width (its unit differs per family) and properties the native models lack stay unbound with a reason. Per-family
   tables replace this one when the exporter records the family.
2. **Curves keep Unity's units; conversion happens when a value is applied.** Colour RGB is sRGB-encoded (Unity
   converts colour properties to linear in a linear-space project, and format colours are linear like glTF's),
   alpha is linear, smoothness is `1 - roughness`, and `_MainTex_ST`'s V offset counts from the bottom (format UVs
   have V down, so ours is `1 - scale_v - offset_v`). `material::get` reads a field in these units and
   `material::set` writes one component, leaving the others untouched, so unbound components never round-trip.
   `_MainTex_ST` moves the base colour texture only.
3. **A slot per renderer, field and component.** `Target::Material(mesh, field, component)` binds on the first
   mesh at the curve's path, as `RendererEnabled` does. Its default is the field's value in the renderer's first
   submesh material that has it, which stands in for Unity reading the default from the renderer's first material.
   The value is applied to every material of the renderer whose model has the field. Avatar masks treat it like
   the renderer's other curves (by the renderer's node path). This replaces the material part of ADR 0007 point 5.
4. **Animated renderers get their own records per island.** `Model` lists the renderers with material slots and
   the avatar materials each uses (once per material). Every island gets its own material record for each, so one
   avatar's animation never shows on another copy or another renderer; unanimated materials stay shared.
   `Island::materials` returns those materials after the step with the animator's values set.
5. **Records are rewritten in the frame, only when they change.** `Forward::add` takes `(material index, record)`
   pairs and writes them in its upload pass with `vkCmdUpdateBuffer`; the material buffer becomes a graph buffer
   there (transfer write, then fragment read), so the next frame's writes wait for this frame's draws. The viewer
   packs each island's animated materials every frame and passes only records that differ from the last ones
   written. Changing the alpha mode or shading model is not animatable, so a material's pipeline never changes.

## Consequences

- Toggles that recolour or light up a renderer work per avatar copy at no cost to unanimated content; material
  capacity is the avatar's materials plus, per copy, the materials of its animated renderers.
- Not checked against Unity yet, to settle with the recorded animator goldens (#54): defaults from the first
  material when a renderer's materials differ, sRGB conversion of animated HDR colours, the V-offset convention
  and Write Defaults on material properties. Supersede this ADR if any differ.
- Outline width, per-family property meanings (lilToon `_ShadowColor` is not exactly MToon's shade colour) and
  `_ST` of textures other than the base colour wait for the exporter's family fingerprints (M0.10).
