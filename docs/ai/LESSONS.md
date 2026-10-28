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
