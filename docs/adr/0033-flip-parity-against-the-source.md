# 0033: FLIP parity compares the compiled avatar with its own import in our renderer, per state and view

Status: accepted; rig bounds superseded by 0036
Date: 2026-11-04

## Context

Design doc §10 "Parity scoring" renders the same camera rig, poses and states as Unity reference images and
compares each pair with NVIDIA FLIP, scored as a traffic light per state: green publishes, yellow publishes with a
note, red needs a fix or the fallback. §9 stage 10 and §21 Phase 0 deliverable 5 put FLIP parity for every toggle
state into the per-avatar report (ADR 0032), and Phase 0 passes only if compiled avatars stay visually identical.

Unity reference renders need Unity projects and baselines the human sets up (#37, #38, M0.10). The stage tests
prove each compiler stage behaviour-identical on the CPU (triangles in world space, animated material records), but
they cannot see what only the GPU shows: texture atlasing and compression, mips, draw order, blending.

## Decision

**What parity means now.** The reference is our renderer drawing the import before any compiler stage; the test is
the compiled package. Both are packaged, signed and loaded the same way, stepped through the same states with the
same seed, drawn by the same code (`view::draw`) with the same rig, settings and light. Any pixel that differs is a
compiler stage's doing. `civbb build --report r.json --measure` packages the import unstaged next to the output
(`<out>.source.civpkg`, deleted afterwards), runs `civbb-client parity <source> <compiled> --key <k> --images
r.parity` and stores its JSON under the report's new `parity` key (null without `--measure` or without a Vulkan
device). The pipeline still depends on no client crate.

**How Unity slots in.** Unity's renders of the source (§10 export path, #37, #38) become a second reference for the
same compiled renders: a new report key `unity_parity` of the same shape with `reference: "unity"`, added within
report v1. Source-against-compiled keeps proving the compiler; Unity-against-source then isolates shader and
runtime translation. The rig is data in the report so the Unity renderer can reproduce it exactly.

**Rig.** 640x640 pixels, vertical field of view 0.6 rad, four views (`front` from +Z, `left` from +X, `back`,
`right`) level with the centre of the avatar's bind-pose bounds, each as far away as needed for the bounds to fill
90 % of the image across the view (height, or width across the view) measured from their near side. Square because
T-posed avatars are about as wide as tall. Background linear (0.08, 0.09, 0.11), the viewer's; light as
`View::look_at` sets it (one directional light towards (-0.3, 0.8, 0.5) normalised, colour 1, ambient 0.3). Avatar
space as ADR 0006 defines it (+Y up, facing +Z). The rig comes from the reference and is used for both sides.

**States.** Rest, then each parameter of the reference's animator set away from its default (0 to 1, anything else
to 0) on top of the state before, three 60 Hz frames each with animator, constraints and dynamics running:
`civbb_client::parity::states`, which the stage tests now call too, so both prove the same states. A parameter the
test avatar lacks is skipped and shows as a difference; the stage tests assert none is lost.

**Pixels.** `Renderer::read_last` copies the last headless frame back: a one-shot copy submitted after the frame on
the same queue (the graph's final `TransferSrc` barrier orders the frame's writes before it), then a host barrier,
into a `GpuToCpu` buffer. Texels are `HEADLESS_FORMAT` (BGRA8 sRGB).

**FLIP.** `flip-rs` 0.1.2: a pure-Rust port of NVIDIA FLIP 1.7 (LDR-FLIP), BSD-3-Clause like FLIP itself,
`forbid(unsafe_code)`, rayon only. `nv-flip` (gfx-rs bindings to the C++, used by wgpu's tests) was the first
choice but cannot link next to `basisu_c_sys`: both compile tinyexr with external symbols, so every binary with
both (all pipeline test binaries) fails with LNK2005. A port of our own would put BSD-3 code in our MIT/Apache tree.
`flip-rs` is young (October 2026, one author), so tests pin it against NVIDIA's own evaluator (`flip-evaluator`
1.7): an image against itself is 0, and a known perturbation's mean and four pixels match to 1e-6 and 1e-5. Before
it was dropped, `nv-flip` gave the same means on AliciaSolid. Default 67.02 pixels per degree (0.7 m from a 0.7 m
wide 3840-pixel display).

**Score.** Per view, over the pixels the avatar covers in either image (not within 1 of the background per channel):
mean, 99th percentile, max and the pixel count. A whole-image mean depends on how much background the framing
leaves; FLIP's spread past an edge onto background pixels does not count.

**Traffic light.** Per view: green when mean <= 0.02 and p99 <= 0.2, red when mean > 0.1 or p99 > 0.5, yellow
between; a state takes its worst view, the avatar its worst state. Our renderer on both sides has no noise of its
own (untextured avatars score exactly 0), so the bounds only have to separate invisible from visible compiler
changes. Measured on the 32 files in corpus/local (#35): the 16 untextured ones score exactly 0 in every view; the
textured ones 0.006 to 0.09 worst mean, p99 0.06 to 0.51. On AliciaSolid stage by stage the error comes from
`atlas` (about 0.015: a shared atlas's mip levels differ under minification) and `compress` (ETC1S, about 0.04
more); `prune`, `merge`, `lod` and `collapse` add nothing. AlphaBlendModeTest is red (0.12 / 0.39: ETC1S turns its
yellow and blue stripes green), TextureTransformTest too (p99 0.51), most textured avatars yellow. Revisit the
bounds when Unity references exist: Unity against us has shader noise that source against compiled does not.

**Report.** `parity`: `reference`, `rig` (`width`, `height`, `fov_y`, `background`, `views` with `name`, `eye`,
`target`), `pixels_per_degree`, `thresholds`, `states` (each `name`, `light`, `views` with `mean`, `p99`, `max`,
`pixels`, in rig order) and the overall `light`; the types live in `civbb_format::cost` next to `Measured`. The
Markdown gets a section with one row per state, mean / p99 per view and the light. `r.parity/NNN-<view>.png` holds
reference, test and the FLIP error map (magma) side by side for the rest state and every state that is not green.

## Consequences

- Every compiler stage is now checked visually as well as behaviourally, on every local avatar, and the lean corpus
  avatar is tested green in every state on a device (CI skips without one). A wrong colour is red.
- The first result is a finding: ETC1S compression is visible on textured avatars (yellow, AlphaBlendModeTest red).
  Compression quality is #68; parity is the gate it has to pass.
- States are cumulative, so later states show several toggles at once, as in the stage tests. Expressions are
  covered only where they are animator parameters (VRM presets); visemes, eye look and gestures are not states yet.
- Parity runs only with `--measure`; it adds two renders and a FLIP per view and state to a build.
- The bind-pose bounds frame the rest pose; a state that moves the avatar far (a big prop) may leave the frame.
