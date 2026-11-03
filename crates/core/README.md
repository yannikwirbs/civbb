# civbb-core

Engine core (design doc §5). Engine crate: depends only on `civbb-format`; the other engine crates may depend on it.

- `math`: vector and quaternion helpers on the format's array types (Unity's Euler order and quaternion
  normalisation), and `Pose` / `world_poses` for world placements of a node hierarchy.
- `jobs`: `islands` runs one job per avatar island in parallel with rayon and sums their counters; `pool` builds a
  worker pool of a given size (ADR 0003).
- `counters`: `System` (the timed systems of a frame, in pipeline order) and `Counters`, per-system time and run
  counts, the release-build counters of design doc §5 "Scheduling rules".

The concrete avatar island (animator, constraints, IK, dynamics, contact pass) needs `anim` and `dynamics`, so it
lives in the glue crate `civbb-client` (ADR 0020).
