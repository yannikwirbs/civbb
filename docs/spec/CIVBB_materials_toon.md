# `CIVBB_materials_toon`

On a glTF material. Replaces the material's core PBR model with the toon model (ADRs 0090, 0100): base fields plus
named features, each present or absent. The material's name, alpha mode and double-sidedness stay core glTF.
Schema: [`schema/CIVBB_materials_toon.schema.json`](schema/CIVBB_materials_toon.schema.json).

## Fields

- `order` (default 0): draw order among materials, lowest first. A material without this extension has order 0.
- Base: `base_color` (linear RGBA) and `base_color_tex`; `shade_color` (linear RGB) and `shade_tex`; `shade_shift`
  (-1 to 1, moves the light/shadow boundary) and `shade_toony` (0 soft to 1 hard).
- Features, each optional: `light_clamp` (light plus ambient clamped per channel; absent adds the ambient on top),
  `normal` (tangent-space normal map), `emission`, `matcaps` (blended in order, at most 2), `rim` (Fresnel rim
  light), `rim_shade` (darkening towards grazing angles), `shade` (strength, a second shadow and a border colour),
  `color_adjust` (hue, saturation, value and gamma of the base colour texture), `outline` (an inverted-hull pass).
- A texture slot is `{texture, uv_set, scale_offset}`: `texture` a glTF texture index, `uv_set` 0 or 1,
  `scale_offset` `[scale_u, scale_v, offset_u, offset_v]`. Colour textures are sRGB, data textures (normal, masks,
  widths) linear.

## Rules

Every texture index resolves to a readable glTF texture; `uv_set` is 0 or 1; at most 2 matcaps.
