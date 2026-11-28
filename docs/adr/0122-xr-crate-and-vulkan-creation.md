# 0122: The `civbb-xr` crate, the OpenXR loader, and who creates the Vulkan device

Status: accepted
Date: 2026-11-28

## Context

M1.4 puts the client in a headset (design doc §4: OpenXR via the `openxr` crate; §5: render submit presents through
OpenXR; §18: any OpenXR runtime). #162 needs the crate, the loader, the session and its frame loop on the
renderer's Vulkan device. Three things the design doc leaves open:

- `openxr` reaches the Khronos loader either at run time (`loaded`, the default: `openxr_loader.dll` /
  `libopenxr_loader.so.1` through the system's library search) or linked, built from the bundled loader source
  with CMake and a C++ compiler (`static`). This PC has no CMake.
- XR_KHR_vulkan_enable2 has the runtime create the Vulkan instance and device from our create infos (it adds the
  extensions it needs) and decide the physical device. `civbb_render::Context::new` creates both itself.
- Layering (`cargo xtask arch`): `civbb-xr` is an engine crate and must not depend on `civbb-render`.

## Decision

1. **Loader: `loaded`.** `civbb-xr` loads the Khronos loader at run time: from the path in `CIVBB_OPENXR_LOADER`
   if set, else by the platform name through the system search (beside the executable on Windows). No CMake or C++
   in the build, no change to CI. A missing loader is `Error::NoLoader`, the desktop viewer keeps working. Releases
   ship the Khronos loader (Apache-2.0) beside the executable; packaging is filed with the release work. Static
   linking stays open for when CMake is part of the toolchain; Android (Phase 3) uses the platform's loader anyway.
2. **Creation seam in `civbb-render`.** A `Create` trait (create the instance from an info, name the physical
   device or leave it to selection, create the device from an info); `Direct` is the plain path and
   `Context::new` uses it, so the desktop path is unchanged. `Context::create(validation, window, creator)` builds
   the instance and device create infos (extensions, features 1.0 to 1.3 with `multiview`) in the one place both
   paths share, checks a required physical device like any other, and owns and destroys what was created.
3. **`civbb-xr` speaks raw handles.** It wraps `openxr` (instance with `XR_KHR_vulkan_enable2`, the HMD system, the
   primary stereo view configuration, blend mode, the session state machine, the frame loop, swapchains) and
   takes and returns Vulkan handles as the `openxr` platform types, so it needs neither `ash` nor `civbb-render`.
   The client implements `Create` with it; the client owns the order of teardown (session and swapchains before
   the device).
4. **Session.** Stage space, else local. One colour swapchain with two array layers for multiview (ADR 0120), else
   one per eye. sRGB colour formats first (the renderer's forward pass writes linear into sRGB targets). Depth comes
   with #163 when the forward pass renders into the swapchain (submitted through XR_KHR_composition_layer_depth
   where the runtime has it).
5. **Tests.** The runtime test skips without a loader, without a runtime and without a headset; with the loader not
   on the search path the gate never starts a runtime (SteamVR launches itself on instance creation).

## Consequences

- Development on this PC needs `CIVBB_OPENXR_LOADER` (SteamVR ships a loader in `bin/win64`) or the Khronos loader
  beside the executable.
- `Context` gains one trait and one constructor; nothing else in the renderer moves.
- A runtime that requires a physical device without the renderer's features fails with `Error::NoDevice` at
  context creation, not later.
