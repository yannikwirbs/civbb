# Lessons: renderer and viewer

Read before working on `civbb-render`, the viewer, parity renders, shaders or window captures. Add a line whenever something costs more than one attempt.

- GitHub runners have a Vulkan loader but no driver: instance creation fails with `ERROR_INCOMPATIBLE_DRIVER`, not a load error. Device tests must skip on that too (`Error::NoDevice`); reproduce locally with `VK_DRIVER_FILES=C:/nonexistent.json`.
- The Vulkan SDK (`C:/VulkanSDK/<version>`) sets `VULKAN_SDK` machine-wide, but sessions started before the install don't see it: pass `VULKAN_SDK=C:/VulkanSDK/1.4.363.0` to `cargo xtask shaders`. `VK_KHRONOS_VALIDATION_VALIDATE_SYNC=true` turns on synchronization validation for a run.
- Slang for Vulkan: `SV_VulkanVertexID` and `SV_VulkanInstanceID` are `gl_VertexIndex` and `gl_InstanceIndex`
  (including base vertex and `firstInstance`); the HLSL semantics may subtract the base. Check buffer struct
  offsets with `spirv-dis` (in the SDK's `Bin`). `slangc` warns E41012 "profile implicitly upgraded" for
  `Sampler2D` arrays; harmless.
- To check what a window shows, capture only that window: PowerShell with `EnumWindows` by title prefix,
  `GetWindowRect`, then `PrintWindow(hwnd, hdc, 2)` (PW_RENDERFULLCONTENT) into a bitmap, then Read the PNG.
  `PrintWindow` reads the window's own content, the Vulkan swapchain included, even when another window covers it;
  `CopyFromScreen` copies whatever is on top. `Process.MainWindowHandle` is 0 for processes started from Git Bash,
  so find the window by title. At 165 Hz `--frames 900` ends in 5 s: start the viewer and capture in one
  PowerShell call (`Start-Process -PassThru`, sleep, capture, `Stop-Process`).
- Viewing pipeline content: `civbb keygen <scratchpad>/key`, `civbb build <src> --key <key> -o <scratchpad>/x.civpkg`,
  then `civbb-client view <pkg> --key <key>`. Booth GLBs converted from FBX carry no textures (Yumeka: 0), so
  they draw white; AliciaSolid (VRM) has textures.
- Debug-build frame times mean nothing: the viewer took 45 ms CPU per frame in debug for what takes 1.5 ms in
  release (validation layer, unoptimised workspace crates).
- Window captures: `SetForegroundWindow` can fail silently and `CopyFromScreen` then grabs whatever is on top (the
  user's other apps). Capture once while the window is known to be in front, check the title bar in the image, and
  delete any capture that shows something else without describing it.
- Viewer timings: the first frame creates the swapchain (170 ms), so short `--frames` means need a warm-up; and CPU
  per frame reads ~10 % higher at vsync than flat out (the CPU idles and cools between paced frames; a 4.5 ms sleep
  between headless frames reproduces it). Compare paced with paced, flat out with flat out.
- Reusing the viewer's draw for other cameras: check every hard-coded camera value (its field of view was fixed at
  0.9 rad, which made the parity rig's framing look wrong).
- Parity renders are a stage ablation tool: run `parity::run` on the import after each stage prefix (a throwaway
  `#[ignore]` test in `crates/pipeline/tests`, release) to see which stage adds FLIP error.
- FLIP against Unity: any geometry mismatch (pose, a toggle, PhysBone sag) puts a view's p99 near 1 and the mean
  above 0.4, swamping shading. Look at the `--images` side-by-sides before reading scores as shader quality; static
  avatars without an animator (Megumi, Ostgaroa) isolate shading.
- Unity reference renders depend on the hand-off's framing, which depends on our runtime (rest bounds include
  PhysBone sag since ADR 0049): after a runtime change, regenerate `rig.json` and `cmp` it with the one the Unity
  images were rendered from before re-scoring; re-render when it differs. Keep the release binaries fixed while a
  render-and-score batch runs (it scores with `target/release`).
- A light-model change can help one family and hurt another: score per family before applying it to all (VRChat's
  mobile shaders got worse clamped; ADR 0050).
- Unity linearises plain colour properties above 1 with `pow(x, 2.2)`, not the sRGB curve (`Mathf.GammaToLinearSpace`,
  probed: 32 gives 2048, the extended sRGB curve 3617), but passes `[HDR]` properties as saved (5.99 stays 5.99; ADR
  0062): a probe of one property type does not cover the other. `Shader.GetPropertyFlags` lists a shader's `[HDR]`
  colours. Probe a Unity fact with a 10-line static method in the host
  project's `Assets/Editor` (`tools/unity-batch.sh`, about 16 s) instead of reasoning from memory.
- A uniform brightness gap between Unity and ours on one avatar (Cokoro ten times darker) is likely an FX-layer
  default animating a material property (`_LightMaxLimit` from a slider at 0): grep the export's `X_animator` for
  material bindings before changing shading.
- Before modelling a shader feature, survey which of the 13 exports use it (a short script over each GLB's
  `X_toon_material`): lilToon Multi turns features on by repurposed keywords (`AUTO_KEY_VALUE` is rim shade,
  `EFFECT_HUE_VARIATION` tone correction; table in lilToon's `lil_replace_keywords.hlsl`), so check keywords too.
- Parity states against Unity must settle (ADR 0055: 60 frames): after three frames Unity still showed startup
  transients (the first-frame swing from the bind pose dragging version 1.0 PhysBones 100 degrees, a SmallHop
  transition from `Grounded` 0) that our runtime does not share. Before blaming geometry, log the reference's rest
  layers and compare a bone's play-mode and edit-mode transforms.
- Map a shader family's parameters from its source, not from memory or old comments: lilToon's shadow ramp (half-
  Lambert, `lilTooningScale`) had been mapped into MToon's shift as if in `n . l` terms and lit most shadowed
  surfaces for months (ADR 0057). lilToon's source is in the host project's `Packages/jp.lilxyzw.liltoon/Shader/
  Includes` (`lil_common_frag.hlsl`).
- HLSL promotes a scalar `Texture2D.Sample` coordinate to `(u, u)` (lilToon's `LIL_SAMPLE_1D`): a "1D" gradient
  sample at time 0 reads the texture's corner, not its middle row.
- A state whose images match colour on colour (Blackbird's black wings on a black body) can hide a geometry
  difference: compare a state with a distinct texture before calling the geometry right.
- Split a brightness gap term by term on both sides: render ours and Unity with the same property changed (emission
  black, emission 1, textures swapped) and compare a few pixels. Ostgaroa's panel matched at emission 1 and not at
  5.99, which pointed at the colour conversion after compression, mips and gradient precision were ruled out.
- Frustum and box tests are conservative at the edges: a box touching a plane at one corner counts as seen. Keep
  "outside" test cases clear of the plane by more than the box's extent (a 45 degree side at z = -6 passes x = 6).
- Before tuning a Unity family's toon parameters, read whether the shader shades at all: VRChat Mobile Toon Lit and
  MatCap Lit have no Lambert term, Poiyomi 7.3 lights through `_ToonRamp` (white, so unshaded, when unset) whatever
  `_LightingMode` says, TSF is unlit. A per-export "ours/Unity luminance on interior pixels" ratio (about 0.9 on
  half the pixels) flagged all of them in one script (#101).
- Telling shading from geometry against Unity: the interior/silhouette split (`ViewScore`) and the avatar masks' IoU
  from the `--images` side-by-sides (rest 0.98 to 1.0, pose differences 0.79 to 0.97). Parity states stack, so a
  pose difference in one state carries into every later state.
- Unity's texture import max size (2048) does not matter at the 640 px rig: shrinking 4096 sources to it moved
  Megumi by 0.003 and Taro by nothing.
- A state's light against Unity is its worst view's (ADR 0083), not the mean's: #106 was closed at mean 0.117 with
  two views above 0.12. Check every view before closing a parity issue.
- On near-black materials a faint layer matters: Poiyomi's `_MatcapReplace` 0.02 of a white matcap is a visible
  sheen on KohzieBoi's black cloth (luminance ratio ours/Unity 0.9). Test which term it is by an experiment build
  before modelling a light model from the source (a "ramp lighting" model of it made the score worse).
- Unity feeds a shader's UV1 from UV0 when the mesh has no second UV set (KohzieBoi: main texture on `_MainTextureUV`
  1, rendered as on UV0); the renderer does the same (`scene.rs`).
- A batch that runs `target/release` binaries for many minutes (`tools/parity.sh` over 13 exports) must not overlap
  a release rebuild; debug `cargo test`/`xtask ci` can run meanwhile.
- `powershell -File script.ps1 -Args a,b` hands a `[string[]]` parameter one string `"a,b"` (no array parsing with
  `-File`): the viewer got bad arguments and exited before its window opened. Take one `[string]` of space-separated
  arguments for `Start-Process -ArgumentList`.
- Window captures by title prefix: match `civbb-client`, not `CIVBB`: the VS Code window's title starts with the repo
  name and gets captured instead (with the owner's account details on it; delete such a capture unread).
- `SV_ViewID` in a Slang shader adds the SPIR-V `MultiView` capability to the whole module (`mesh.spv`), so every
  device that builds those pipelines must enable Vulkan 1.1 `multiview`, single-view passes included (it reads 0
  there). Any other device creation path (OpenXR's `xrCreateVulkanDeviceKHR`, #162) must enable it too.
- Scripted doc edits keyed on a whole sentence fail when the file wraps it differently than you wrote it: grep the
  line first and key on what is there.
