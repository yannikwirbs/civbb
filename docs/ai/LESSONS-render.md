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
