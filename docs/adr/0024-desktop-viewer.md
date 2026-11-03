# 0024: The desktop viewer loads verified packages, steps then draws each frame, and reports timing in its title

Status: accepted
Date: 2026-11-03

## Context

#27 is Phase 0 deliverable 4 (design doc §21): a small desktop viewer that runs N copies of an avatar with animator
and dynamics and measures CPU and GPU time. ADR 0021 put the window loop in the client; the client already steps
islands (ADR 0020) and `civbb-render` draws scenes (ADR 0023). The architecture rules forbid any crate depending on
a glue crate, so the client cannot use the glTF importer in `civbb-pipeline`. The doc does not say how the viewer
gets avatars, how island state becomes skinning input, or how timing is shown before there is a UI.

## Decision

1. **Input: runtime packages and corpus avatars.** `civbb-client view` takes a signed package with the key that
   signed it (the `civbb` key-file convention), verifies and validates it, or a corpus avatar. glTF and VRM go
   through `civbb build` first; that also exercises the same path real content takes.
2. **Textures decoded in the client** with `image` (PNG and JPEG only, sides up to 8192); a texture that is
   missing, KTX2 or broken draws untextured with a message. The renderer only sees RGBA8 (ADR 0023 point 3).
3. **Island state to skinning.** Skinning matrices are each joint's world placement (grid origin included) times
   its inverse bind; a mesh without a skin uses its node's placement. Blendshape weights are the animator's slot
   values (Unity's 0 to 100 scaled to 0 to 1) where a clip binds the shape, default weights elsewhere
   (`Island::shape_weights`). A mesh whose node is inactive is neither skinned nor drawn; renderer-enabled curves
   and first-person visibility are not applied yet.
4. **One thread, step then draw.** Each frame steps every island (`frame::step`, parallel jobs) and then records
   and submits the frame; the separate render thread of design doc §5 comes with the Phase 1 client.
5. **Timing in the window title, not an overlay.** There is no text rendering yet; the title shows means over half
   a second of CPU time per frame, the avatar systems' share, GPU time (timestamps at the start and end of the
   frame's commands, `Renderer::gpu_ms`) and frame rate. CPU time excludes what `Renderer::frame` spends waiting
   for the frame fence and the swapchain (`Renderer::blocked_ms`), so vsync does not count as work. `--frames N`
   runs N frames, prints the means and exits non-zero on validation errors, for automated runs.
6. **Grid and camera.** Copies stand in a square grid centred on X, rows going back along -Z, spaced by the
   avatar's bind-pose width; an orbit camera (drag to turn, wheel to zoom) starts in front, framing the grid.

## Consequences

- Any avatar the pipeline can import can be viewed, at the cost of a build step.
- Numbers only mean something from release builds: debug builds run validation and unoptimised workspace code
  (4 copies of a 54k-vertex avatar: 45 ms CPU per frame in debug, 1.5 ms in release).
- Revisit the title when the UI crate can draw text, and the threading with the Phase 1 frame pipeline.
