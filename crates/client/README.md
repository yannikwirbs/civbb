# civbb-client

The client (design doc §5). Glue crate: wires `anim`, `dynamics`, `core` and `render` together; nothing depends on
it.

- `island`: `Model`, an avatar's systems compiled once and shared, and `Island`, one avatar's running state.
  `Island::step` runs design doc §5 step 5 in order: last frame's contact values into their animator parameters,
  the animator (pose written to node transforms and activity), constraints, an IK stub, bone dynamics, then world
  placements and activity, each timed into `civbb_core::Counters`.
- `frame`: `step` runs every island as its own rayon job, then the cross-avatar contact pass (step 6) once.
- `bench` and `main.rs`: `civbb-client bench [--avatars N] [--threads T,T..] [--frames F] [--warmup W]
  [--avatar lean|bloated]` steps N corpus avatars at each worker count and prints JSON: wall ms per frame (mean,
  p50, p99), ms per avatar, speedup over the first run, and CPU ms per avatar by system. Avatars stand on a 1 m grid,
  sway so their chains move, and flip one parameter every 30 frames. Run it with
  `cargo run --release -p civbb-client -- bench`.
- `view` and `main.rs`: `civbb-client view [<package.civpkg> --key <key-file> | --avatar lean|bloated]
  [--copies N] [--frames F]` opens a window with N copies of a verified runtime package (make one from glTF or VRM
  with `civbb build`) or a corpus avatar in a grid, animator and dynamics running, skinned and drawn by
  `civbb-render`. Drag to orbit, scroll to zoom. The title shows CPU time per frame (without waiting for vsync and
  the GPU), the avatar systems' share and GPU time; `--frames` exits after F frames, prints those numbers and fails
  on validation errors. PNG and JPEG textures are decoded with `image`; KTX2 is checked, then transcoded to BC7 by the pure-Rust
  `basisu` crate (ADR 0029).

Decisions are in ADRs 0020 and 0024.
