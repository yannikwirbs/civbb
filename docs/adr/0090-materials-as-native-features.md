# 0090: Materials are native models with named features; shader families are lowered into them by table

Status: accepted
Date: 2026-11-24

## Context

#100: materials are a closed set (`Pbr`, `Toon`, `Unlit`). `Toon` carries lilToon's and Poiyomi's definitions under
their names (`tone` is `_MainTexHSVG`, `light_min`/`light_max` are `_LightMinLimit`/`_LightMaxLimit`, `rim_shade`,
`rim_ramp`, `shade_bands`, a first matcap in flat fields and a `second_matcap`), and `render_queue` is Unity's
render queue. Design doc §6 fixes the models (PBR, Toon, Unlit, Translated) and §10 the family tables (ADR 0043).
The render path is verified by parity (ADRs 0083, 0084); Yumeka's rest worst view (0.1197) has no margin.

Options:

1. Keep the structs, rename the fields. Leaves features as fixed singletons (one or two matcaps, one rim).
2. Each model has base fields plus optional named features, each with its own equation in the spec; repeatable
   features are lists; families lower into them by table.
3. Node-graph materials (MaterialX-like) compiled to SPIR-V. Most freedom; costing, Quest variants and validation
   are a project of their own.
4. Creator shader code only. That is §10's translated path, already planned for migrated custom shaders.

## Decision

Option 2 now; option 3 later as a separate model if creators need it; option 4 stays §10's path.

1. **Shape.** `Material = {name, model, alpha, double_sided, order}`; `model` is `pbr`, `toon` or `unlit` with base
   fields, and optional features named for what they compute: `normal`, `emission`, `matcaps` (a list, capped by
   the shader), `rim`, `rim_shade`, `shade` (the shadow beyond one colour), `color_adjust` (hue, saturation, value,
   gamma), `light_clamp` (min, max), `outline`. No source shader's names in the format or its spec.
2. **Draw order.** `alpha` picks the pass; `order` offsets within it (lowered as Unity's queue minus the pass's base:
   2000 opaque, 2450 mask, 3000 blend), so the renderer's sort key is the same number as today.
3. **Units.** Colours state their encoding in the spec (linear, as glTF, for native content), angles radians, widths
   metres.
4. **Lowering.** ADR 0043's fingerprint and family tables stay in the importer and fill the native features.
5. **Verification.** A restructuring only: the material records the renderer uploads must stay byte-identical for
   the generated corpus and the local Unity exports, and the parity table must not move.

## Consequences

- Repeatable features lift today's caps (more than two matcaps) once the shader loops over them; a new feature is an
  additive change (ADR 0087).
- Materials change last in M1.1: nothing in M1.2 to M1.5 reads them.
- Revisit graph materials with the Blender add-on's needs (Phase 3).
