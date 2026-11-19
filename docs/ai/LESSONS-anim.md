# Lessons: animation and dynamics

Read before working on `civbb-anim`, `civbb-dynamics`, constraints, muscles or their tests. Add a line whenever something costs more than one attempt.

- serde_json `Number` equality is type-sensitive: integer `0` != float `0.0`. Compare JSON numbers via `as_f64()`.
- Unity muscle space is not "angle-axis swing then twist": it is half-angle tangents (`zyroll`). Check reconstructions found online against recordings before trusting them.
- Comparing rotations by `acos(|dot|)` in f32 is noisy near 0 (0.05 degrees); compare components with a sign flip instead.
- Discrete collision tunnels when a particle moves further than a collider per step; test contacts with gentle forces, and remember single-pass collide-then-length can settle through a surface (iterate them).
- To identify an unknown blend in recordings, fit the output as a linear combination of the inputs (least squares, all sign choices): a near-zero residual proves linearity and gives the effective weights (how Unity's sequential nlerp of constraint sources showed up).
- Boundary tests in f32: `0.15 + 0.05` is not exactly `0.2`. Test "touching counts" with binary-exact values (0.25 + 0.25 at 0.5) and keep other cases clear of the boundary.
- Corpus toggles have Write Defaults off: a value a clip once wrote persists after switching state. Tests that
  compare islands must start from a toggle that defaults off (`parameters` with `default == 0.0`).
- Measure a compile-time optimisation in our runtime before shipping it: collapsing toggle layers into a blend tree
  (a big win in Unity) made our animator 4x slower until blend tree weights stopped allocating. Time it with a
  throwaway release test (`Instance::update` in a loop) and delete the test afterwards.
- `serde_json` writes non-finite floats as `null` and refuses `null` for an `f32`: Unity's step tangents (infinity)
  need `Key`'s `null`-as-infinity reader (ADR 0041). Other format floats must be finite.
- Fitting the SDK's PhysBones (#75): a throwaway numpy model of the chain in Unity space against
  `vrc_physbones.json` (max bone distance over all frames per case) finds the model fast; try one force at a time
  on the cases that isolate it (`pull_high` has no spring or gravity). Frame 0 of the recording already holds one
  step.
