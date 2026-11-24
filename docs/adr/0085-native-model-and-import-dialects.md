# 0085: The content model is the platform's own; Unity, VRChat and VRM are import dialects the pipeline lowers

Status: accepted
Date: 2026-11-24

## Context

#100: the format (`crates/format`, the source package's `X_*` extensions, design doc §9) is Unity's and VRChat's data
model written as Rust types. ADR 0041 made every `X_*` extension the serde JSON of a format type, so the Unity
exporter's output, the native source format and the runtime model are one and the same. Survey of the code
(2026-11-24, format v25):

- Shared state is the animator's parameter table: conditions, blend inputs and drivers index it; contact receivers
  write it by name, PhysBones by name prefix (`_IsGrabbed`); `Expressions` declares sync in VRChat's bit budget.
- Curves target a Unity path plus a Unity property (`localEulerAnglesRaw` degrees, muscle clip names, blendshape
  0 to 100, shader property names resolved at runtime by `civbb_anim::material`, a `Component` catch-all).
- Mecanim is the behaviour model, Write Defaults included; materials are three closed structs whose toon fields
  carry lilToon's and Poiyomi's definitions; humanoid bones are `HumanBodyBones`; worlds are one `ContentKind`.
- `FORMAT_VERSION` (25) is written into packages and never checked on read; no migration exists.

What Phase 1 consumes: M1.2's ECS loads the model into entities; M1.3's IK and locomotion read and write built-in
values; M1.5's state sync and protocol carry synced values (§12: IK targets plus synced parameters, quantised);
OSC (§17) and the plugin capability `avatar.params.local` (§11) read and write them; Phase 2's WIT host API (§11)
addresses entities, values and events. Nothing is published, so the model can change at no compatibility cost.

Options:

1. Keep the model and write a spec of it. Cheapest; makes Unity's semantics the platform's permanent ones.
2. A native model on its own terms, into which Unity, VRChat and VRM content is lowered by the importers; the
   runtime's evaluation stays as recorded, its inputs change shape.
3. Replace the runtime semantics too (an animation graph instead of state machines, scripts instead of state
   behaviours). Most freedom, but discards the evaluation the Unity and SDK recordings verify, and delays Phase 1.

## Decision

Option 2.

1. **Two layers.** The native model (`civbb_format`, written as `CIVBB_*` extensions, ADR 0086) is what native
   tools write and what the compiler compiles. Import dialects are lowered into it by `civbb-pipeline`: VRM
   (`VRMC_*`, as now) and Unity, which is the exporter's `X_*` extensions, frozen as **Unity dialect 1** (their shape
   at format v25; names kept, since they are never written natively). Dialect semantics live only in importer code:
   Unity paths and property names, Unity units, shader property names, render queues, VRChat parameter names, its
   bit budget and the PhysBone parameter-prefix convention.
2. **Fork on divergence.** Dialect parsing reuses a native Rust type until that type changes; the change then copies
   the old type into the importer. Frozen dialect fixtures (generated content covering every variant) must import
   to a golden native model and never change, so a native change cannot silently change what the dialect reads.
3. **Kept sources stay as uploaded** (native or dialect). Recompiling re-lowers them with the current importer, so a
   better lowering improves old content too.
4. **Behaviour stays a layered state machine, specified as ours.** Layers in weighted groups, override and additive
   blending, masks, states, transitions with exit time and interruption, 1D, 2D and direct blend trees: a general
   hierarchical state machine, which the runtime evaluates exactly as recorded. What changes: conditions and blend
   inputs read state entries (ADR 0088); curves target native properties (ADR 0089); VRChat's state behaviours
   become native actions (set, add, random and copy a state entry; layer and group weight; pose source, for
   tracking control; locomotion); Write Defaults becomes each state's `unwritten` rule, `hold` or `rest`: what the
   state does to targets its layer animates and its motion does not. Lowering Write Defaults into explicit constant
   curves was rejected: it costs a curve per layer target per state, and blend tree children differ subtly (ADRs
   0010, 0065).
5. **One document model for avatars and worlds.** A document is a node tree, components that reference nodes
   (meshes, materials, chains, colliders, contacts, constraints, the state table, behaviour; scripts later) and a
   role: `avatar` (humanoid, viewpoint, face, menu) now, `world` in Phase 2 with the same components.
6. **Humanoid.** Native bone names are VRM 1.0's humanoid bone set (open, documented, what Blender tools already
   use); the Unity importer maps `HumanBodyBones` by table. The muscle space stays the runtime's humanoid pose model
   (muscle frames carried per avatar, ADRs 0064, 0072), with native muscle identifiers and its definition in our
   spec. Baking muscle clips into bone rotations at compile time is left for later: blend trees blend in muscle
   space, so it would change results.
7. **Order.** State (ADR 0088) and bindings (ADR 0089) before M1.2; the document role and humanoid names next;
   materials (ADR 0090), then the spec, schema and versioning (ADRs 0086, 0087) last in M1.1, before any native
   writer exists. Every step keeps the Unity and SDK recordings and ADR 0084's parity table as they are.

Supersedes ADR 0041 points 1 and 5 for the native format: `X_*` become Unity dialect 1, read as ADR 0041 describes;
the native format gets its own extensions (ADR 0086) and versions (ADR 0087).

## Consequences

- The Unity exporter and the corpus exports stay as they are; the Rust importer grows a lowering per part.
- References become indices: every pipeline stage that renumbers nodes, meshes or materials (prune, merge, atlas)
  must remap them, where Unity paths survived renumbering for free.
- The recordings test behaviour, not layout, so they keep proving that lowered content behaves identically.
- Revisit the behaviour model when Phase 2 scripting needs animation graphs: a graph would be a new component
  version (ADR 0087), with state machines as one node kind.
