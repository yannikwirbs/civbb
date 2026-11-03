# Lessons

Environment gotchas and mistakes not to repeat. Add a line whenever something costs more than one attempt.

- Git Bash may lack `~/.cargo/bin` on PATH: prefix with `export PATH="$HOME/.cargo/bin:$PATH" &&`.
- Git Bash rewrites leading-slash args into Windows paths: use `gh api repos/...`, not `gh api /repos/...`.
- A `cd` in a Bash call persists into later calls; use absolute paths.
- Never `canonicalize()` repo paths on Windows: it adds a verbatim `\\?\` prefix and breaks comparisons with cargo metadata paths.
- Python `open(p, "w")` on Windows writes CRLF; use `newline="\n"`. rustfmt is pinned to Unix newlines.
- The `gltf` crate (1.4) is not safe on hostile input by itself: its accessor reader uses unchecked arithmetic, `image.source()` unwraps unvalidated fields, GLB parsing underflows on short header lengths, and gltf-json's validation indexes POSITION accessors before checking them. Use `gltf_import::parse` and `accessor.rs`, never `gltf::import` or `Reader`.
- Fuzz in CI: the prebuilt cargo-fuzz (taiki-e/install-action) is a musl binary and defaults to the musl target, which ASan rejects. Pass `--target x86_64-unknown-linux-gnu`.
- Test that hostile-input cases fail for the intended reason (assert the error text); a missing required field can make every case fail early and pass vacuously.
- Python heredocs via Bash: a Rust escape like backslash-x02 inside a normal triple-quoted string becomes a real byte; use raw strings (`r'''...'''`) when the text holds Rust escapes. A Bash heredoc containing a stray quote fails to parse; write long content to the scratchpad with Write and `cat >>` it.
- Booth archives: zip names are Shift-JIS without the UTF-8 flag (`corpus/extract_zip.py` decodes them), FBX textures point at the author's absolute paths (`corpus/fbx_to_glb.py` relinks by file name), and Blender images report `has_data = False` until loaded, so check file existence instead. Always run Blender with `--factory-startup --disable-autoexec` on third-party files.
- serde_json `Number` equality is type-sensitive: integer `0` != float `0.0`. Compare JSON numbers via `as_f64()`.
- Unity animator semantics: the manual's Transitions page and `AnimatorStateTransition.exitTime` reference are the authoritative sources (any-state first, ordered interruption, exit time); Write Defaults is only documented by the community (VRC School, VRCFury `technical/wd`). Johansen's thesis PDF (runevision.com/thesis) gives the 2D blend formulas; extract it with `pdftotext` (in Git Bash).
- Unity is installed outside the default paths: Editor `G:/Unity6/Editor/Unity.exe` (Hub `D:/zProgramm/UnityHub`, list in `%APPDATA%/UnityHub/editors-v2.json`). The Unity CLI (`C:/Users/xxsay/AppData/Local/Unity/bin/unity.exe`) is on the user PATH but not in sessions started before it was installed: call it by full path. Don't conclude "not installed" from `C:/Program Files` alone.
- Unity batch runs take about 20 s: `Unity.exe -batchmode -nographics -quit -projectPath tools/unity-truth -executeMethod Class.Method -logFile <log> <own args>`; read own args with `Environment.GetCommandLineArgs()`. Licensing and `config.uca.cloud.unity3d.com` errors in the log are harmless. `Avatar` internals (`GetPreRotation`, `GetPostRotation`, `GetLimitSign`, `GetZYRoll`) are reachable by reflection; `AnimationUtility.GetAnimatableBindings` lists a humanoid's clip property names.
- The workspace globs `tools/*`: a non-crate directory there (the Unity project) must be in `[workspace] exclude`.
- Unity muscle space is not "angle-axis swing then twist": it is half-angle tangents (`zyroll`). Check reconstructions found online against recordings before trusting them.
- Comparing rotations by `acos(|dot|)` in f32 is noisy near 0 (0.05 degrees); compare components with a sign flip instead.
- Discrete collision tunnels when a particle moves further than a collider per step; test contacts with gentle forces, and remember single-pass collide-then-length can settle through a surface (iterate them).
- Unity constraints only evaluate in play mode. Batch mode can enter it: set `EditorSettings.enterPlayModeOptions` to disable domain and scene reload, `EditorApplication.EnterPlaymode()`, tick in `EditorApplication.update`, exit with `EditorApplication.Exit(0)` (no `-quit`). Pass output dirs as absolute paths (relative ones resolve against the project), and wrap the work in try/finally around `Exit`: an exception in an update callback leaves the editor running until the timeout.
- To identify an unknown blend in recordings, fit the output as a linear combination of the inputs (least squares, all sign choices): a near-zero residual proves linearity and gives the effective weights (how Unity's sequential nlerp of constraint sources showed up).
- `python -I` drops the script's directory from `sys.path`, so own scratch helper modules don't import; use it for scripts that read untrusted downloads, not for own analysis scripts.
- Boundary tests in f32: `0.15 + 0.05` is not exactly `0.2`. Test "touching counts" with binary-exact values (0.25 + 0.25 at 0.5) and keep other cases clear of the boundary.
- The workspace globs `crates/*`: an empty `crates/<name>/` without a `Cargo.toml` breaks every cargo command, `fmt` included. Create the manifest first.
- Benchmarks: `cargo run` defaults to debug; numbers only mean something from `--release` (the report's `profile` field says which).
- GitHub runners have a Vulkan loader but no driver: instance creation fails with `ERROR_INCOMPATIBLE_DRIVER`, not a load error. Device tests must skip on that too (`Error::NoDevice`); reproduce locally with `VK_DRIVER_FILES=C:/nonexistent.json`.
- The Vulkan SDK (`C:/VulkanSDK/<version>`) sets `VULKAN_SDK` machine-wide, but sessions started before the install don't see it: pass `VULKAN_SDK=C:/VulkanSDK/1.4.363.0` to `cargo xtask shaders`. `VK_KHRONOS_VALIDATION_VALIDATE_SYNC=true` turns on synchronization validation for a run.
- Python run from a Bash heredoc turns Rust or Windows-path backslash escapes inside normal strings into real characters (a carriage return in a Rust literal, which clippy reports as `no_effect_replace`; `\1` in a path). Write Rust and docs with the Write/Edit tools; if Python must write them, use raw strings and check the output.
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
- Corpus toggles have Write Defaults off: a value a clip once wrote persists after switching state. Tests that
  compare islands must start from a toggle that defaults off (`parameters` with `default == 0.0`).
- The Bash tool rejects `sleep N; cmd` waits; watch CI with `gh run watch <id> --exit-status` in a background call.
- Measure a compile-time optimisation in our runtime before shipping it: collapsing toggle layers into a blend tree
  (a big win in Unity) made our animator 4x slower until blend tree weights stopped allocating. Time it with a
  throwaway release test (`Instance::update` in a loop) and delete the test afterwards.
- Mutation-check equivalence tests, and make the mutated case reachable: a parameter driver that only fires on
  frame 1 (when every layer starts in its default state anyway) could not show a driver-timing bug.
- Before/after binaries: `git worktree add --detach <scratchpad>/wt HEAD`, build there with
  `--target-dir <scratchpad>/wt-target`, then `git worktree remove --force` it. Piping `civbb build` into `head`
  makes it panic with "failed printing to stdout"; harmless.
- Python's `open()` on Windows defaults to cp1252: read and write repo files with `encoding="utf-8"`, or
  replacements containing `§` or other non-ASCII text silently fail to match. Git Bash's `iconv` reports every file
  as invalid UTF-8; check with Python's strict `decode("utf-8")` instead.
- `basisu_c_sys` (Basis Universal encoder) runs bindgen at build time: it found Visual Studio's libclang without
  `LIBCLANG_PATH`. Its `default-features = false` build fails (transcoder code compiled out but still referenced):
  keep the defaults and add `encoder`. Its own threading (`BU_COMP_FLAGS_THREADED`) makes UASTC RDO output differ
  between runs; encode single-threaded and parallelise across textures.
- Pure-Rust parsers still need hostile-input tests: `basisu` 0.1 allocates a KTX2 level's stated uncompressed
  length unchecked (a 1 EB abort from three flipped bits). It also adds the header's `sgdByteOffset` unchecked (an
  overflow panic the CI fuzzer found): check every header range (DFD, KVD, SGD, levels) before calling it. A test that truncates and bit-flips a small valid file
  a few thousand times finds such things in seconds, on Windows, without cargo-fuzz.
- Unity-exported UVs often sit a whole tile off 0..1 (AliciaSolid's V runs 1 to 2); with a repeating sampler
  that is equal to 0..1, so check "each triangle in one unit tile", not "inside 0..1".
- Python `str.replace` silently does nothing when the pattern is not there: a "no effect" experiment may never
  have run. `assert old in s` before every replace, or use the Edit tool (it fails loudly).
- meshoptimizer (`meshopt` crate): `LockBorder` keeps open edges, and seams stay closed on manifold surfaces, but
  an edge three or more faces share can open (its faces go by collapsing their third vertex; locking the edge's
  vertices does not help). Check simplified meshes with vertices welded by position and edge use counts, not raw
  indices: wedges (split normals, UV seams) make every flat-shaded edge look open.
- `gh run list --commit <sha>` right after `git push` can come back empty; list runs a few seconds later (or by
  `--limit`) and watch each run ID.
