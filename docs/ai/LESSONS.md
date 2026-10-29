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
