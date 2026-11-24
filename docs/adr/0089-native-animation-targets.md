# 0089: Curves target native properties by reference, in native units; Unity bindings are resolved by the importer

Status: accepted
Date: 2026-11-24

## Context

#100: a curve's `Binding` is a Unity `/`-separated path plus a Unity property, resolved when an island loads
(`civbb_anim::bind`): Euler rotation as `localEulerAnglesRaw` in degrees (ADR 0007), muscles by Unity clip name
(ADR 0014), blendshape weights 0 to 100, material properties by shader property name mapped through one flat table
of Unity names (`civbb_anim::material`, ADRs 0025, 0067), a `Component { component, property }` catch-all. A native
tool would have to write Unity's names and units to animate anything.

Options:

1. Keep Unity bindings, resolved at load (status quo).
2. Typed references (an index plus a native property) in native units, resolved by the importer; the source format
   writes them as JSON pointers into the document, modelled on Khronos's `KHR_animation_pointer`.
3. Pointers kept as strings into the runtime, resolved at load. Strings and lookups on every client (§7 wants
   bindings resolved once, at compile time).

## Decision

Option 2.

1. **Target kinds**, each an index plus a property, in native units:
   - node: translation (m), rotation (quaternion), `rotation_euler` with its axis order (radians), scale, active;
   - mesh instance: visible, morph target weight (0 to 1), the material of a submesh (a material index, held until
     the next key), a material field overridden for this instance's submeshes that have it;
   - humanoid: a muscle by native identifier (ADR 0085 point 6), the body pose (ADR 0063);
   - state entry value (ADR 0088);
   - component field: constraint fields (ADR 0040) now, chain and contact fields as they gain them (#77).
   Colours are linear unless the curve declares `srgb` (authoring tools animate in display space; ADR 0067's gamma
   conversion is that flag). Booleans are values 0 and 1.
2. **Source form.** A curve's target is a JSON pointer into the native document (`/nodes/3/rotation`,
   `/meshes/2/weights/5`, `/extensions/CIVBB_state/entries/4`), with `KHR_animation_pointer`'s syntax where it
   covers the property; plain glTF animations import as today. Keys stay Hermite with per-key tangents and steps (a
   superset of glTF's samplers, which allow one interpolation per sampler).
3. **The Unity importer resolves** paths to nodes and meshes, property names to kinds, degrees to radians,
   0..100 to 0..1, `localEulerAnglesRaw` to `rotation_euler` in Z, X, Y order, renderer-wide material curves to
   instance overrides, Unity shader property names to native material fields per fingerprinted family (ADR 0043;
   `civbb_anim::material`'s flat table moves there), "vectors written whole" (ADR 0067) to explicit constant curves
   for components no curve animates, muscle clip names to native identifiers. Bindings that resolve to nothing are
   reported and dropped at import (they bind to nothing today); the catch-all goes.
4. **Renumbering stages remap.** Prune, merge and atlas renumber nodes, meshes and materials; one shared remap
   applies to every target, and a target whose element was removed is removed with it.

## Consequences

- The runtime binds indices only; `node_paths` and string matching leave `civbb-anim`.
- Unit conversions at import round keys differently from conversion at evaluation (about 1e-7 relative); the anim
  recordings' tolerances and the parity table must show no change beyond that.
- A native tool animates with glTF-style pointers and documented units, nothing from Unity.
