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
  on the cases that isolate it (`pull_high` has no spring or gravity). The SDK takes two steps on a chain's first
  frame (built in edit mode, the first was even at an unpinned frame time: build cases in play mode).
- To identify a step's force terms when the parent is pinned (bone 1), regress the pre-length-step point on candidate
  vectors with the condition `cross(P - parent, recorded direction) = 0` (linear in the coefficients): it gave
  stiffness as "carried with the parent" (residual 2e-6) at once. It is degenerate near equilibrium; use frames
  with motion (sway).
- Teacher forcing (one model step from the recorded previous frame, spring 0 cases) isolates a contact rule: the
  chord a bone moves equals the collider depth at its closest point, so the SDK pushes the end by that depth.
- Python `from a import x` followed by `from b import *` silently replaces `x` with `b.x`: a scratch model ran a
  different driver for an hour. Keep one driver in one module.
- Frame-rate questions: record the same cases at other frame times (`-civbbDt`). 30 Hz equal to every second 60 Hz
  frame bit for bit proved fixed 1/60 s steps; compare with exact equality, not tolerances.
- VRChat's engine sets built-ins every frame: a runtime that leaves them at the creator's defaults starts
  `Grounded` at 0 and plays the falling states (`Builtin::at_rest`, ADR 0055).
- Hand-derived animator rules (#54) have been wrong twice against Unity probes (ADR 0056: Write Defaults off on
  muscles, exit times on non-looping and empty states). Probe a rule before relying on it for parity.
- Compare collider and bone radii in world space before blaming a rule: HAOLAN's inside capsule sits under a node
  scaled 100 inside the 0.01 armature (0.3 m) while its chains' bones are 0.0126 m; the issue's "bone radius above
  the capsule's" guess was 25x off. A Python walk of the GLB's node scales answers it in one call.
- Cases inserted mid-list in `VrcPhysBones.cs` move later cases on the grid: bounds within a few µm of their limit
  can trip on float noise after a re-record (`simplified_p10_s100` 0.0007 to 0.000702).
- Every PhysBone length or radius in the SDK is in some node's units: check which scale each field takes (record a
  case with only that node scaled) before trusting a replay that passes. HAOLAN's 1.26 bone radius was used as
  metres under a 0.01 armature, which put every tip on the far side of its inside capsule; the `scaled` case had
  scaled chain and collider together, which hid it.
- Before fitting a Unity quantity freely, search its internals by reflection for the table it uses (a probe listing
  members named `*Mass*` found `HumanTrait.GetBoneDefaultHierarchyMass`): free least-squares weights over rigidly
  linked bones are degenerate (hips, spine and upper legs fitted -224, 75, 75) and do not generalise. Then fit the
  structure (where each mass sits, which points span a frame) against cases that move one muscle group at a time.
- A rotation blend whose axis is right but whose angle grows faster than linear in the weight is Unity's
  `normalize(w * q.xyz, q.w)` (loop pose, ADR 0063), not slerp or nlerp: compare `2 atan(w tan(a/2))`.
- Before modelling a missing-bone case, check which bones the corpus actually lacks (13 exports: UpperChest in 9,
  toes in 4, never neck or shoulders) and record exactly those skeletons.
