# 0088: One state table per document, read and written by reference; events as a separate channel

Status: accepted
Date: 2026-11-24

## Context

#100: the only shared state is the animator's parameter table, addressed by name. Contact receivers write it by
name (`client/src/island.rs` looks the name up at load), PhysBone chains by name prefix (`_IsGrabbed`, `_Angle`),
the menu and network sync are declared in `Expressions` with VRChat's types and bit costs (bool 1, int 8, float 8),
and the 30 built-ins (`civbb_anim::behaviour::Builtin`) are recognised by VRChat's names. Phase 1 adds readers and
writers: state sync (§12: synced parameters, quantised, 30 Hz), OSC (§17), plugins (`avatar.params.local`, §11),
IK and locomotion; Phase 2 adds scripts (§11: entities, values, events). Design doc §7 already sets the sync
budgets: 256 bits for legacy avatars, 1,024 for native ones.

Options:

1. Keep the animator parameters as the state and add a sync table beside them (status quo).
2. A state table per document: typed entries declared by the content, referenced by every reader and writer;
   events as a separate typed channel.
3. Every component field addressable as state (a reflection system, as Unity's). Unbounded wire and permission
   surface.
4. A runtime key-value blackboard with string keys. Flexible; strings at runtime and nothing resolved at compile.

## Decision

Option 2.

1. **The state table** (`CIVBB_state`, root-level): ordered entries `{name, type, default, sync, saved, external}`.
   Types: `bool`; `int` (i32, optional `range`); `float` (f32, optional `range`); `trigger`, a bool a transition
   that tests it resets (state machines need latches). Names are the creator's, unique, `/` allowed for
   namespaces; compiled to indices. Names are looked up only at the edges (OSC, plugin API, debugger), through the
   compiled name table.
2. **One mechanism for every reader and writer**, each by entry reference: transition conditions, blend inputs,
   speed, cycle-offset and time inputs, actions (ADR 0085), menu controls, contact receivers, chain outputs (an
   explicit entry per output: grabbed, posed, angle, stretch, squish, instead of the prefix convention), curves
   (ADR 0089), OSC, plugins, scripts and network sync. The animator has no parameter table of its own.
3. **Platform values** are entries whose `source` is the platform, read-only to content, under native names in a
   `platform/` namespace with SI units (`platform/velocity`, `platform/grounded`, `platform/upright`,
   `platform/gesture_left`, `platform/viseme`, `platform/is_local`, `platform/tracking_type` and the rest of
   `Builtin`'s meanings). The Unity importer maps VRChat's built-in names onto them; OSC keeps VRChat's addresses
   for existing tools (§17) through a fixed table at its edge, and creator entries keep their own names.
4. **Sync** per entry: `sync: none | owner` (the owning client sends it; the server validates, §12). The wire
   encoding follows the type: bool 1 bit; int with a range ceil(log2(span + 1)) bits; float with a range `bits`
   (default 8) quantised uniformly over it; triggers are not synced. Budgets count these bits: 256 for lowered
   legacy content, 1,024 by default for native content (§7), the network governor deciding the rest (§12).
   VRChat's types lower exactly: Int to int 0..255, Float to float -1..1 in 8 bits, Bool to bool. `saved` persists
   the value per user across sessions; `external` lets inputs from outside the content write it (menu, OSC,
   plugins), true for every lowered VRChat expression parameter, false by default natively.
5. **Events** are a second channel, not state: declared `{name, payload: none | bool | int | float, sync}`, emitted
   by components (contact enter and exit, grab and release, menu press, state enter) and scripts, delivered once to
   subscribers in the next frame phase, sent reliable and ordered per entity when synced (§12). Phase 1 lowers
   VRChat's one-frame pulses (On Enter receivers) to state as recorded; events are implemented with scripting in
   Phase 2, and `CIVBB_state` reserves their declarations.

## Consequences

- M1.5's protocol carries entry indices and the type's encoding, not VRChat's parameter layout or bit costs.
- Every system shares one resolution step at compile time; a typo in a name fails the import, not a runtime lookup.
- The animator goldens and contact recordings must stay green through the change: it moves data, not evaluation.
- Revisit vector and quaternion entries when scripts or IK need them (a new type, additive).
