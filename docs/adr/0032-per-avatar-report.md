# 0032: The per-avatar report: counts per compiler stage from `civbb build`, times from `civbb-client measure`

Status: accepted
Date: 2026-11-04

## Context

Phase 0 deliverable 5 (design doc §21) is a report per avatar: before and after stats, FLIP parity for every
toggle state (#35) and time per frame. §8's cost model lists what costs CPU (animator layers and states, bones,
chains, contacts, constraints), GPU (skinned vertices, blendshapes, triangles per LOD, pipelines) and memory, and
§9 stage 9 says the cost is measured by running the content on reference hardware, not just counted. Phase 0
passes when compiled avatars cost clearly less CPU than the baselines (#38), so the report has to show where each
stage saved what, and the measured times must be the same numbers `civbb-client bench` and `view --frames` give.

The layering rules forbid the obvious route: `pipeline` may not depend on `client` or `render`, and only the
client can step islands and draw.

## Decision

**Counts come from the compiler.** `civbb build <src> --key <k> --report <r.json>` writes `r.json` and `r.md`.
The stage order moves from `main.rs` into `civbb_pipeline::compile`, which takes a snapshot of [`Stats`] after
import and after each stage (`source`, `prune`, `atlas`, `merge`, `lod`, `collapse`, `compress`) and returns
them with the package. Each snapshot counts, over the whole avatar:

- `nodes`, `bones` (distinct skin joints), `meshes`, `skinned_meshes`, `draws`, `vertices`, `triangles`;
- `lod`: vertices and triangles per LOD level (level 0 is the full mesh; a mesh with fewer levels counts its last,
  as `Mesh::level` draws it), one entry per level of the deepest chain;
- `blend_shapes`, `shape_deltas`, `materials`, `toon_materials`, `textures`, `texture_bytes` (encoded blob bytes);
- `clips`, `curves`, `keys`, `parameters`, `layers`, `states`, `transitions`, `animator_bytes`;
- `expression_params`, `synced_bits`, `chains`, `colliders`, `contacts`, `constraints`.

`draws` counts every submesh, as if every mesh showed. What shows in the default state depends on the animator,
so the measured part reports the draws of the default state from the viewer instead. The report also holds the
signed package size (`package_bytes`).

**Times come from the client, in its own process.** `civbb-client measure <pkg> --key <k> [--avatars N]
[--frames F] [--warmup W]` reads and verifies the signed package (the loader `view` uses), then prints one JSON
[`Measured`] to stdout:

- `cpu`: `bench` on the package's avatar with one worker: N avatars (default 100) for W + F frames (default 60 +
  300), mean and p99 frame time, `ms_per_avatar` and CPU per system and avatar. One worker, because the question
  is what one avatar costs; scaling across workers is the `bench` report's job.
- `gpu`: the viewer's own frame code (the same grid, camera, skinning and forward pass) on an offscreen 1280x720
  target instead of a swapchain (`Renderer::headless`, one image per frame in flight), N copies for W + F frames
  back to back, warm-up discarded: GPU time per frame from the timestamps and divided by N, the viewer's CPU time
  per frame and its avatar systems, draws and skinned meshes of the last frame, and the device name. `null` when
  there is no Vulkan device (CI), with a note on stderr.

`bench` takes the same package (`--package <pkg> --key <k>`, #57), so its numbers can be compared.
`view --frames F` gets the same warm-up (`--warmup W`, default 60) and summarises only the last F frames: its
first frame creates the swapchain (170 ms on the reference PC), which used to add ~0.5 ms to the mean of a
360-frame run.

`civbb build --report r.json --measure` writes the package, runs `civbb-client measure` from the directory of
the running `civbb` executable (cargo puts both binaries there) and stores its output under `timings`. The
pipeline still depends on no client crate; the type both sides share, `civbb_format::cost::Measured`, lives in
`format` next to the cost profile it measures. Without `--measure`, `timings` is `null`.

**Release builds only.** `measure` refuses to run in a debug build (CPU is ~30x slower with validation and
unoptimised crates, LESSONS.md), so a debug number can never reach a report.

**Shapes.** JSON keys are the snake_case field names above; the top level is `report` (the shape's version, 1),
`avatar`, `compiler`, `format_version`, `platform`, `stages` (a list of `{"stage": name, ...counts}` in compiler
order), `package_bytes` and `timings`. New keys may be added within a version; renaming or removing one, or
changing a meaning, bumps `report`. The Markdown is for people: a table with one row per count and one column per
stage (a stage column only shows values that changed, so savings stand out), the package size, and the timings
as two short tables. A golden JSON for the lean corpus avatar (`crates/pipeline/tests/golden/report-lean.json`)
pins the counts; timings are excluded from goldens by construction (`compile` never measures).

The signed header's `CostProfile` stays empty for now: the governor that would read it (#63, Phase 1) does not
exist, and writing measured times into it means signing twice. The report holds everything it would.

## Consequences

- Each stage's effect is visible per avatar without reading its tests; a stage that makes an avatar worse shows
  up as a column.
- The measured CPU number is `bench` with `--threads 1` on the same avatar, and the GPU number is `view --frames`
  without a window, so both can be checked against those commands directly. Done for #34 on AliciaSolid, 100
  copies: CPU per avatar 0.0158 to 0.0166 ms from `measure`, 0.0158 to 0.0159 ms from `bench`; GPU per frame
  0.704 to 0.706 ms headless, 0.703 to 0.719 ms in the window.
- The viewer's CPU per frame reads ~10 % lower headless (1.00 ms, avatar systems 0.28 ms) than in the window at
  vsync (1.10 ms, 0.35 ms). That is pacing, not measurement: headless frames with a 4.5 ms sleep between them read
  1.08 to 1.11 ms (0.37 ms), as the window does; the CPU idles and cools between paced frames. Headless runs flat
  out like `bench`, so the report's CPU numbers stay comparable with each other. Pace it if the governor needs
  paced numbers.
- GPU time per avatar from N copies at a fixed camera is a relative number: pixel cost depends on screen
  coverage, and per-avatar division hides the fixed cost of the clear. Good enough to compare before and after;
  revisit with the governor's real cost model.
- A missing `civbb-client` next to `civbb` makes `--measure` fail with a clear message rather than measure
  nothing.
- FLIP parity per toggle state (#35) adds its own section to the report under the same version rules.
