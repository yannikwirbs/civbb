# civbb-xr

OpenXR for the client (design doc §4, §5, §18; ADR 0122). Engine crate: depends on nothing internal (and `openxr`,
which loads the Khronos loader at run time: `CIVBB_OPENXR_LOADER`, else the system search).

- `Xr`: instance with XR_KHR_vulkan_enable2, the head-mounted system, its primary stereo views and blend mode, the
  Vulkan version range it accepts, and the runtime's creation of the Vulkan instance and device. Handles are the
  `openxr` platform types; the client glues them to `civbb_render::Create`.
- `Session`: the session on a Vulkan device, its state machine (`poll`), stage or local space, one colour
  swapchain with a layer per eye (or one per eye), and the frame loop (`wait`, `begin`, `views`, `acquire`,
  `release`, `end`).

Input, poses for trackers, hand and face tracking and haptics come in later M1.4 issues.
