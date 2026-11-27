# civbb-corpus

One job: generate synthetic avatars for tests and benchmarks (design doc §21, "deliberately bloated avatars" and "control" corpus slots).

Output is a complete `civbb_format::Avatar` (hierarchy, skinned meshes, sparse blendshapes, toggle animator, expressions, nine-tail dynamics, contacts, rotation constraints), deterministic for a given spec, and free of third-party assets. Use it as a dev-dependency.

`world()` generates the test world the desktop player walks in (ADR 0110): ground, stairs and a walkable and a too
steep ramp up to a platform, and a wall, as rigid meshes.
