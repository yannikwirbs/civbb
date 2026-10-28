# civbb-dynamics

Bone dynamics (design doc §7 "Bone dynamics"). Engine crate: depends only on `civbb-format` (and later
`civbb-core`).

- `rig`: compiles an avatar's chains into one flat particle list (parents first, per-particle parameters
  evaluated from their graded curves), the layout the solver loops over.
- `solver`: per-avatar `State`: steps every chain against the animated pose and writes the simulated local
  rotations back; grab, release and pose; the `_IsGrabbed`, `_IsPosed`, `_Angle`, `_Stretch` and `_Squish` values.
- `math`: the few vector and quaternion helpers the solver needs (move to `civbb-core` with #22).

The model and its unverified constants are in ADR 0015; colliders and limits in ADR 0016.

Input is an avatar that passed `civbb_format::validate`; indices are not re-checked, parameter ranges are clamped.
