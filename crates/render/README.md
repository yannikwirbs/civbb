# civbb-render

Renderer (design doc §6). Engine crate: depends on `civbb-format` (meshes, materials, matrices). Decisions in ADRs
0021, 0022 and 0023.

- `Context`: Vulkan 1.3 instance, the Khronos validation layer in debug builds when installed (errors are counted,
  `validation_errors()`), device selection (`pick_device`: Vulkan 1.3 with dynamic rendering, synchronization2,
  buffer device address, descriptor indexing and multi-draw indirect, a graphics and compute queue that can
  present, discrete GPU first) and the one queue. `Context::headless` has no window.
- `Renderer`: a window's swapchain (FIFO, sRGB, recreated on `resize` or when out of date), two frames in flight
  and the graph's transient images. `frame` hands the caller a `Graph` with the swapchain image imported; it
  records, submits and presents. `color_format` is the format pipelines draw in.
- `graph`: passes declare colour and depth attachments (`Load::Clear`, `Load`, `DontCare`) and buffer accesses
  (`Access`), and optionally record commands. Images are imported or transient (`Graph::transient`, taken from a
  `Transients` pool when recorded and reused across frames); buffers are imported with how earlier work touched
  them. `Graph::plan` derives the layout transitions and barriers, `Graph::record` records them with dynamic
  rendering. Passes run in the order added.
- `memory`: `Buffer` (gpu-allocator memory plus device address), `Image`, `Context::create_buffer`,
  `create_image`, `one_shot`, `upload`.
- `scene`: `Scene` holds every mesh in shared rest-vertex, attribute, delta, index and skinned-vertex buffers, one
  material buffer and the bindless textures (`texture`). `add_mesh`, `add_texture` (RGBA8, mips generated),
  `add_material`; sized up front with `Capacity`.
- `skin`: compute skinning and sparse blendshapes. `SkinnedMesh::new` takes an instance's range of the skinned
  buffer; `Skinning::record` skins a frame's instances from their skinning matrices and blendshape weights,
  applying only shapes with non-zero weight (fixed-point atomics, ADR 0022). Output per vertex: position, normal,
  tangent.
- `material`: material records and `PipelineKey` (shader, blend, double-sided: twelve pipelines).
- `forward`: `View` (camera with reverse-Z infinite projection, one directional light, ambient), `Draw`s from
  `MeshData::draws`, `batch` into runs by pipeline, and `Forward`, which adds the upload and forward passes: one
  indirect multi-draw per run, vertices pulled from the shared buffers.
- `shaders/*.slang`: `skin` (compute) and `mesh` (vertex, `pbr`, `toon`, `unlit`), compiled by `cargo xtask shaders`
  (needs the Vulkan SDK) into committed `shaders/spv/`; `cargo xtask ci` fails when a source changed since.

Windows come in as raw handles (`raw-window-handle`), so the crate has no window loop: that is the client's (#27).
`cargo run -p civbb-render --example clear [-- <frames>]` opens a window and clears it.

Tests: `graph`, `select`, `forward` and the active list in `skin` need no GPU; `device`, `draw` and the `skin`
reference comparison run headless on a real device and skip without one (CI).
