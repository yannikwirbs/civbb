# 0123: XR frame pacing across the two threads, the play space's place, and what the first stereo frames leave out

Status: accepted
Date: 2026-11-28

## Context

#163 puts the viewer's frames into the headset (design doc §5 steps 1, 8 and 10; §6 multiview). ADR 0103 point 5
pipelines the viewer: the simulation thread steps and extracts frame N+1 while the render thread draws frame N.
OpenXR's frame loop is `xrWaitFrame` (blocks until the runtime wants a frame, gives its predicted display time),
`xrBeginFrame`, drawing, `xrEndFrame`; the `openxr` crate splits the session into a `FrameWaiter` and a
`FrameStream` so the two may live on different threads. ADR 0122 point 4 planned a depth swapchain with #163 and
kept a one-swapchain-per-eye fallback. Until the VR player (#165) the world has no notion of where the play space
is.

## Decision

1. **`xrWaitFrame` paces the simulation thread; the render thread begins and ends.** `civbb_xr::Session` (events,
   `wait`, session begin and end) stays on the simulation thread and `civbb_xr::Frames` (begin, locate the views,
   acquire, release, end) moves to the render thread. The simulation thread waits, steps the frame and hands the
   snapshot over with the frame's `FrameState`; the render thread begins that frame, locates the eyes for its
   predicted display time just before drawing, draws, releases and ends. The runtime blocks the next wait until the
   last frame was begun, so the two threads stay one frame apart as on the desktop, and the predicted display time
   is known where #164 samples poses. Every frame waited for is begun and ended, also when the runtime says not to
   render it or tracking is lost (then with no layers). When the runtime stops the session, the simulation thread
   lets the render thread finish its frame before `xrEndSession`.
2. **The render thread draws through a `Present` seam**: the renderer's own target (window, offscreen) or the
   headset. `civbb_render::Renderer::external` and `frame_into` draw into images owned elsewhere (imported with
   their layers, left in the layout the owner expects), so frames in flight, timestamps and transients are shared
   with the desktop path.
3. **Culling: a mesh is drawn when either eye's frustum sees it**, the union of the two, tested per eye; level of
   detail by the distance from the eyes' midpoint. The screen-space overlay is not drawn in the headset (the menu
   moves into the world with #170).
4. **The play space stands on the ground below the camera, facing along its look** (`xr::stage`), until #165 makes
   the player place it; play-space metres are world metres (no eye-height scale yet).
5. **One colour swapchain with two layers, no fallback.** Multiview needs a layered image; the per-eye fallback of
   ADR 0122 point 4 is removed rather than kept unused. A runtime without array swapchains fails with
   `Error::Unsupported`; a copy from an own layered image is the fix if one turns up.
6. **Depth submission is #175**, not #163: the forward pass's depth stays transient. No desktop mirror window yet.
7. **Pacing report**: frames waited for, frames the runtime did not want rendered, and display periods missed (the
   gap between consecutive predicted display times, rounded to periods, minus one), past the viewer's warm-up.

## Consequences

- `view --xr` takes every viewer option (`--world`, `--copies`, packages); its timings are the desktop viewer's.
- Without depth, the runtime reprojects from colour alone (SteamVR's motion smoothing works less well) until #175.
- Late latching (#167) can move the view location later still; it already happens on the render thread right before
  recording.
- Supersedes ADR 0122 point 4's per-eye swapchains and its "depth comes with #163".
