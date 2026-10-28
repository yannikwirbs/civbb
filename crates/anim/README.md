# civbb-anim

The avatar animation runtime (design doc §7 "Animation"). Engine crate: depends only on `civbb-format` (and later
`civbb-core`).

- `curve`: evaluates one curve of Hermite keys exactly as Unity's `AnimationCurve` does.
- `bind`: compile-time binding. Every animated property of an avatar gets a slot in one flat `f32` buffer with its
  default value; clips become lists of slot curves. No string paths at runtime (ADR 0007).
- `clip`: samples a bound clip into the slot buffer.
- `motion`: what a state plays: blend tree weights (1D, 2D simple directional, freeform cartesian and polar
  gradient bands, direct; ADR 0009), synced times and lengths.
- `machine`: per-layer Mecanim state machines (entry, any-state, exit, exit time, interruption, triggers; ADR 0008).
- `behaviour`: legacy built-in parameters and state behaviours (drivers, tracking, layer and locomotion control;
  ADR 0011).
- `mix` and `controller`: a `Controller` (compiled, shared) and per-avatar `Instance`s that update parameters,
  layers and the pose, with Write Defaults on, off and mixed (ADR 0010).

Conformance: `tests/curve.rs` (key semantics), `tests/machine.rs` (state machine), `tests/blend.rs` (weights),
`tests/behaviour.rs` (built-ins and behaviours),
`tests/golden/*.json` run by `tests/golden.rs` (frame-by-frame cases citing their sources).

Input is an avatar that passed `civbb_format::validate`; indices are not re-checked.
