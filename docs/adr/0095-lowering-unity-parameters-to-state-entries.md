# 0095: Lowering Unity parameters to state entries; an entry's rest value

Status: accepted
Date: 2026-11-26

## Context

ADR 0088 replaces the animator's parameter table and `Expressions::parameters` with one state table (#115). A Unity
export declares a parameter up to twice, in the animator and in the expression parameters, and the two declarations
disagree in ways the runtime already reproduces:

- **Two defaults.** VRChat starts an instance at the expression parameter's default (#85), but Write Defaults restores
  a curve-animated parameter to the animator's declared default. They differ on the corpus (Yumeka 15 parameters,
  Cokoro 3, Blackbird 1). `Controller::start` and `bind.rs` each read one of them.
- **Two types.** An expression parameter may declare a network type other than the animator parameter's (none do in
  the corpus).
- **Names that resolve to nothing.** A menu control, puppet axis or contact receiver may name a parameter neither
  table declares (none do in the corpus); VRChat then writes nothing.

## Decision

1. An entry has `default` (the value an instance starts at) and an optional `rest`: the value Write Defaults restores
   where an animation curve writes the entry, when it is not `default`. Native content leaves `rest` out.
2. The Unity importer lowers dialect 1 into entries: the animator's parameters first, in their order (every animator
   index stays valid), then the expression parameters the animator lacks. A parameter in both takes the animator's
   type, the expression parameter's default, `sync` (owner when synced), `saved`, and `external: true`; its animator
   default becomes `rest` when the two differ. Animator-only parameters are local and not external.
3. Names that resolve to no entry (empty, or unknown) lower to `None`: the control, axis or receiver writes nothing.
   Entry names are unique; validation refuses duplicates.
4. Until #118 sets the sync encoding per entry, the synced budget counts an entry's type: bool and trigger 1 bit, int
   and float 8. This equals VRChat's count unless the two declarations disagree between bool and another type.
5. The parity run against Unity steps through the states of the stored hand-off (the ones Unity rendered), not the
   states recomputed from the entries, so entries the animator never declared add no states to old references.

## Consequences

- Evaluation is unchanged: animator goldens, contact recordings and the Unity dialect golden (parameters moved into
  `state` only) pin it. `FORMAT_VERSION` 26.
- `rest` is a Unity-ism in the native model. Revisit with #121 (actions and the unwritten rule): if native content
  never needs a rest value apart from the start value, the importer can bake it into the clips instead.
- The corpus's dialect writer (`civbb_corpus::gltf`) raises entries back to dialect 1, so round trips stay tested.
