# civbb-render

Renderer (design doc §6). Engine crate: depends on no internal crate yet (only `core` and `format` are allowed).
Decisions in ADR 0021.

- `Context`: Vulkan 1.3 instance, the Khronos validation layer in debug builds when installed (errors are counted,
  `validation_errors()`), device selection (`pick_device`: Vulkan 1.3 with dynamic rendering and synchronization2,
  a graphics and compute queue that can present, discrete GPU first) and the one queue. `Context::headless` has no
  window.
- `Renderer`: a window's swapchain (FIFO, sRGB, recreated on `resize` or when out of date) and two frames in flight.
  `frame` hands the caller a `Graph` with the swapchain image imported; it records, submits and presents.
- `graph`: passes declare their colour attachments (`Load::Clear`, `Load`, `DontCare`) and optionally record
  commands; `Graph::plan` derives the layout transitions and barriers, `Graph::record` records them with dynamic
  rendering. Passes run in the order added.

Windows come in as raw handles (`raw-window-handle`), so the crate has no window loop: that is the client's (#27).
`cargo run -p civbb-render --example clear [-- <frames>]` opens a window and clears it.

Tests: `graph` and `select` need no GPU; `device` renders headless on a real device and skips without one (CI).
