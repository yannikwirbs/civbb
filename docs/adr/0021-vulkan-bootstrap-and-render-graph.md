# 0021: Vulkan bootstrap, render graph shape, frames in flight and the shader toolchain

Status: accepted
Date: 2026-11-03

## Context

#24 starts `civbb-render`: an `ash` Vulkan 1.3 bootstrap with a swapchain, validation in debug builds and a render
graph skeleton. Design doc §4 fixes Vulkan 1.3 via `ash` on desktop; §6 asks for a render graph where "passes
declare their resources; the graph handles barriers, aliasing of transient targets and pass culling", compute
skinning, bindless textures and indirect draws. It does not say how the graph is built and run, how many frames
are in flight, how the renderer meets the window, or which shading language and compiler we use. CI runners have
no GPU and no Vulkan SDK.

## Decision

1. **Window boundary.** `render` takes a window as raw handles (`raw-window-handle` 0.6, surfaces via
   `ash-window`) and never runs an event loop; `winit` is only a dev-dependency for the `clear` example. The client
   owns the loop and calls `Renderer::resize` and `Renderer::frame` (#27).
2. **Device.** Required: Vulkan 1.3 with `dynamicRendering` and `synchronization2`, and one queue family with
   graphics and compute (and present, with a window). Preference: discrete, integrated, virtual, CPU, first on a
   tie (`pick_device`). One queue for everything; async compute waits until a profile shows it helps. Features for
   bindless and indirect draws are added by the issues that use them (#26).
3. **Validation.** Debug builds enable `VK_LAYER_KHRONOS_validation` if installed (otherwise a warning, and it
   runs without), with a debug-utils messenger printing warnings and errors to stderr and counting errors. Device
   tests assert the count is zero; tests that need a device skip when the loader or a device is missing.
4. **Render graph shape.** Built every frame (a handful of passes; cheap). Images are imported with an initial and
   a final `Use` (undefined, colour attachment, transfer source, present; the set grows with need). A pass
   declares its colour attachments with a load action and optionally records commands; passes run in the order
   added. `plan` walks the passes, tracks each image's use and emits a synchronization2 image barrier where the
   use changes or after a write (write after write); after the last pass every image goes to its final use. Passes
   with attachments record inside a dynamic rendering scope (no `VkRenderPass` or framebuffer objects); passes
   without (compute) record outside one. Not yet: graph-owned transient images and their aliasing, pass culling,
   reordering, depth attachments and buffer resources. They come with the first pass that needs them (#25 buffers,
   #26 depth and transient targets).
5. **Frames in flight: two.** Each has a command pool (reset per frame), an "acquired" semaphore and a fence. The
   "rendered" semaphore that present waits on is per swapchain image, because a present holds its semaphore until
   that image is acquired again. Two frames bound latency to one extra frame, which matters more in VR than the
   throughput a third would add.
6. **Swapchain.** FIFO (always available, vsync); 8-bit sRGB BGRA or RGBA when offered; minimum image count plus
   one. Recreated on `resize` and on out-of-date or suboptimal results, after waiting for the device to go idle
   (resizes are rare). A zero-sized window skips frames.
7. **Shaders: Slang, compiled offline.** Shaders are written in Slang and compiled to SPIR-V with `slangc` from the
   Vulkan SDK (1.4.363 on the reference PC) by an xtask command added with the first shader (#25). The `.spv`
   files are committed and embedded with `include_bytes!`, each next to a record of its source's hash, which
   `cargo xtask ci` checks so a stale binary fails CI. Builds and CI need no shader compiler. Slang is HLSL-family,
   the language legacy custom shaders arrive in (§10), and is maintained under Khronos; it compiles the features
   §6 needs (descriptor arrays, subgroups, buffer device addresses). `naga` (pure Rust, WGSL or GLSL) was rejected
   for gaps in exactly those features, and `shaderc` in `build.rs` for adding a native C++ build to every compile.

## Consequences

- The renderer runs and is validated on the reference PC; CI checks graph planning and device selection only.
- Committed SPIR-V means a shader change needs the SDK locally; the hash check makes forgetting it fail loudly.
- No memory allocator yet: the bootstrap only uses swapchain images. #25 picks one with its first buffers.
- Revisit frames in flight and the present mode with the XR path (OpenXR owns the swapchain there) and with
  profiling.
