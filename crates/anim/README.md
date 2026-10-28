# civbb-anim

The avatar animation runtime (design doc §7 "Animation"). Engine crate: depends only on `civbb-format` (and later
`civbb-core`).

- `curve`: evaluates one curve of Hermite keys exactly as Unity's `AnimationCurve` does.
- `bind`: compile-time binding. Every animated property of an avatar gets a slot in one flat `f32` buffer with its
  default value; clips become lists of slot curves. No string paths at runtime (ADR 0007).
- `clip`: samples a bound clip into the slot buffer.

Input is an avatar that passed `civbb_format::validate`; indices are not re-checked.
