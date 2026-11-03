# civbb-render

Renderer (design doc §6). Engine crate: depends on `civbb-format` (meshes, matrices). Decisions in ADRs 0021 and
0022.

- `Context`: Vulkan 1.3 instance, the Khronos validation layer in debug builds when installed (errors are counted,
  `validation_errors()`), device selection (`pick_device`: Vulkan 1.3 with dynamic rendering and synchronization2,
  a graphics and compute queue that can present, discrete GPU first) and the one queue. `Context::headless` has no
  window.
- `Renderer`: a window's swapchain (FIFO, sRGB, recreated on `resize` or when out of date) and two frames in flight.
  `frame` hands the caller a `Graph` with the swapchain image imported; it records, submits and presents.
- `graph`: passes declare their colour attachments (`Load::Clear`, `Load`, `DontCare`) and optionally record
  commands; `Graph::plan` derives the layout transitions and barriers, `Graph::record` records them with dynamic
  rendering. Passes run in the order added.

- `memory`: `Buffer` (gpu-allocator memory plus device address), `Context::create_buffer`, `one_shot`, `upload`.
- `skin`: compute skinning and sparse blendshapes. `SkinnedMesh::new` uploads a mesh; `Skinning::record` skins a
  frame's meshes from their skinning matrices and blendshape weights, applying only shapes with non-zero weight
  (fixed-point atomics, ADR 0022). Output per vertex: position, normal, tangent.
- `shaders/*.slang`: compiled by `cargo xtask shaders` (needs the Vulkan SDK) into committed `shaders/spv/`;
  `cargo xtask ci` fails when a source changed since.

Windows come in as raw handles (`raw-window-handle`), so the crate has no window loop: that is the client's (#27).
`cargo run -p civbb-render --example clear [-- <frames>]` opens a window and clears it.

Tests: `graph`, `select` and the active list in `skin` need no GPU; `device` and the `skin` reference comparison
run headless on a real device and skip without one (CI).
