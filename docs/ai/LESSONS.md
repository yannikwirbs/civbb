# Lessons

General gotchas; read every session. Read a topic file before working in its area:

- `LESSONS-unity.md`: the Unity Editor, the exporter (`tools/unity-exporter`) or ground truth (`tools/unity-truth`).
- `LESSONS-render.md`: `civbb-render`, the viewer, parity renders, shaders or window captures.
- `LESSONS-textures.md`: atlasing, mips, KTX2 or the Basis Universal encoder.
- `LESSONS-parsers.md`: parsers of untrusted input, fuzz targets or native (`-sys`) dependencies.
- `LESSONS-anim.md`: `civbb-anim`, `civbb-dynamics`, constraints, muscles or their tests.
- `LESSONS-data.md`: Booth archives, Blender conversions, GLB files, meshes or goldens.
- `LESSONS-method.md`: tests meant to catch a bug, refactor pins, measurements, benchmarks, runs of minutes.

Add a line to the matching file whenever something costs more than one attempt.

- Shell: Git Bash rewrites leading-slash args into Windows paths (`gh api repos/...`, not `gh api /repos/...`). A
  `cd` persists into later calls; use absolute paths. `cargo` is on PATH (a shell started before a toolchain
  install may lack it: `export PATH="$HOME/.cargo/bin:$PATH"`).
- `cargo xtask ci 2>&1 | tail -3 && git commit ...` commits even when CI fails: the pipeline's status is `tail`'s.
  Start such Bash calls with `set -o pipefail`. Likewise `a && b >> file` skips the append when `a` fails (a `grep`
  with no match): check that an appended file changed.
- Never `canonicalize()` repo paths on Windows: it adds a verbatim `\\?\` prefix and breaks comparisons with cargo
  metadata paths.
- The workspace globs `crates/*` and `tools/*`: an empty `crates/<name>/` without a `Cargo.toml` breaks every cargo
  command (create the manifest first); a non-crate directory under `tools/` must be in `[workspace] exclude`.
- Write Rust and docs with the Write/Edit tools. Python run from a Bash heredoc turns backslash escapes inside normal
  strings into real characters (`\x02`, `\n` in a Rust literal, `\1` in a path), and a stray quote breaks the
  heredoc. Even a quoted `<<'EOF'` heredoc lost backslashes here, raw strings included (`r'\1'` arrived as byte
  0x01 and was written into sources): write such scripts to the scratchpad with the Write tool and run them.
- Scripted fixes keyed on compiler error lines: one fixer per build round. A second fixer in the same round reads
  line numbers the first one shifted, and rerunning on an already fixed line wraps it again.
- Python on Windows: `open()` defaults to cp1252 (replacements containing `§` silently fail to match) and `"w"`
  writes CRLF: use `encoding="utf-8", newline="\n"`. `str.replace` silently does nothing when the pattern is
  missing: `assert old in s` first, or use the Edit tool. Git Bash's `iconv` reports every file as invalid UTF-8;
  check with Python's strict `decode("utf-8")`.
- `python -I` drops the script's directory from `sys.path` and ignores `PYTHONIOENCODING` (print non-ASCII with
  `python -I -X utf8`); use it for scripts that read untrusted downloads, not for own analysis scripts.
  `xargs ... python -I -` leaves Python in the REPL; write the script to a file instead.
- GitHub CI: `tools/ci.sh` (it copes with runs not yet listed right after a push and with `--log-failed` printing
  nothing while a job runs). The Bash tool rejects `sleep N; cmd` waits; `tools/ci.sh --wait` in a background call.
  File issues with `tools/issue.sh new` (body on stdin): long `gh issue create` bodies with backticks and
  apostrophes break Bash quoting.
- Stopping a background job (TaskStop) can leave its children running (`timeout`, Unity); a second run then aborts
  with "another Unity instance is running with this project open". Find leftovers with
  `powershell -NoProfile -Command "Get-CimInstance Win32_Process"` filtered by command line and stop them by PID;
  never stop every `Unity.exe` (the owner may have an Editor open). Git Bash has no `setsid`.
- A background `cargo test` started before an edit compiles the old code; rerun after editing.
- Don't restore a file with `git checkout --` after a mutation check of a file you changed this session: it reverts
  to the commit and drops your uncommitted edits. Revert the mutation with the inverse edit.
- Renaming a field with `sed -E 's/\bname\b/new/'` across files also renames same-named fields of other types
  (`local_only` of drivers and of contact receivers): build right after and revert the hits the compiler names.
