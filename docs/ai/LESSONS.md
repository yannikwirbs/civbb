# Lessons

General gotchas; read every session. Read a topic file before working in its area:

- `LESSONS-unity.md`: the Unity Editor, the exporter (`tools/unity-exporter`) or ground truth (`tools/unity-truth`).
- `LESSONS-render.md`: `civbb-render`, the viewer, parity renders, shaders or window captures.
- `LESSONS-textures.md`: atlasing, mips, KTX2 or the Basis Universal encoder.
- `LESSONS-parsers.md`: parsers of untrusted input, fuzz targets or native (`-sys`) dependencies.
- `LESSONS-anim.md`: `civbb-anim`, `civbb-dynamics`, constraints, muscles or their tests.
- `LESSONS-data.md`: Booth archives, Blender conversions, GLB files, meshes or goldens.

Add a line to the matching file whenever something costs more than one attempt.

- `cargo` is on PATH in Git Bash sessions (checked 2026-11-18). A shell started before a toolchain install may lack it:
  then prefix `export PATH="$HOME/.cargo/bin:$PATH" &&`.
- Git Bash rewrites leading-slash args into Windows paths: use `gh api repos/...`, not `gh api /repos/...`.
- A `cd` in a Bash call persists into later calls; use absolute paths.
- Never `canonicalize()` repo paths on Windows: it adds a verbatim `\\?\` prefix and breaks comparisons with cargo metadata paths.
- Python `open(p, "w")` on Windows writes CRLF; use `newline="\n"`. rustfmt is pinned to Unix newlines.
- Python heredocs via Bash: a Rust escape like backslash-x02 inside a normal triple-quoted string becomes a real byte; use raw strings (`r'''...'''`) when the text holds Rust escapes. A Bash heredoc containing a stray quote fails to parse; write long content to the scratchpad with Write and `cat >>` it.
- The workspace globs `tools/*`: a non-crate directory there (the Unity project) must be in `[workspace] exclude`.
- `python -I` drops the script's directory from `sys.path`, so own scratch helper modules don't import; use it for scripts that read untrusted downloads, not for own analysis scripts.
- The workspace globs `crates/*`: an empty `crates/<name>/` without a `Cargo.toml` breaks every cargo command, `fmt` included. Create the manifest first.
- Benchmarks: `cargo run` defaults to debug; numbers only mean something from `--release` (the report's `profile` field says which).
- Python run from a Bash heredoc turns Rust or Windows-path backslash escapes inside normal strings into real characters (a carriage return in a Rust literal, which clippy reports as `no_effect_replace`; `\1` in a path). Write Rust and docs with the Write/Edit tools; if Python must write them, use raw strings and check the output.
- The Bash tool rejects `sleep N; cmd` waits; watch CI with `gh run watch <id> --exit-status` in a background call.
- Mutation-check equivalence tests, and make the mutated case reachable: a parameter driver that only fires on
  frame 1 (when every layer starts in its default state anyway) could not show a driver-timing bug.
- Before/after binaries: `git worktree add --detach <scratchpad>/wt HEAD`, build there with
  `--target-dir <scratchpad>/wt-target`, then `git worktree remove --force` it. Piping `civbb build` into `head`
  makes it panic with "failed printing to stdout"; harmless.
- Python's `open()` on Windows defaults to cp1252: read and write repo files with `encoding="utf-8"`, or
  replacements containing `§` or other non-ASCII text silently fail to match. Git Bash's `iconv` reports every file
  as invalid UTF-8; check with Python's strict `decode("utf-8")` instead.
- Python `str.replace` silently does nothing when the pattern is not there: a "no effect" experiment may never
  have run. `assert old in s` before every replace, or use the Edit tool (it fails loudly).
- `gh run list --commit <sha>` right after `git push` can come back empty; list runs a few seconds later (or by
  `--limit`) and watch each run ID.
- `gh run view <id> --log-failed` prints nothing while any job of the run is still going; fetch a finished job's log
  with `gh api repos/yannikwirbs/civbb/actions/jobs/<job id>/logs` (job ids from `gh run view <id> --json jobs`).
- Don't benchmark while compiling: run timing batches alone (background them and wait) or the numbers mean nothing.
- Python heredocs through the Bash tool turn `\n` inside Rust string literals into real newlines too (valid Rust,
  but it breaks string continuations and reads badly). Write Rust strings with escapes through Edit/Write only.
- A throwaway study that runs for minutes: check it uses more than one core (CPU seconds against wall time) and that
  its filters still apply after edits; a variant list that stopped honouring `STUDY_VARIANTS` ran full encodes per
  file and looked like a leak. Pipe long output through `grep --line-buffered` or progress stays invisible.
- Long measurement runs (the 2026-11-05 session took 202 min, about 60 of them avoidable): before starting a corpus-wide
  run, (1) run it on the smallest input first and on one real avatar, (2) after a minute check CPU seconds against
  wall time and committed memory, (3) drop variants the one-avatar run already rules out (corpus-wide RDO variants
  cost 13 min after AliciaSolid had shown XUASTC beats RDO), (4) run one variant when only images or one property
  are needed, and (5) never end the turn just to wait: do the next independent issue meanwhile.
- `local_avatars_atlas_and_shrink` is `#[ignore]`d (two minutes of codec-ladder encodes, and CI has no corpus/local):
  run it with `--ignored` after changing `atlas` or `texture`. Run `cargo fmt --all` before `cargo xtask ci`, and the
  gate once per commit, targeted `cargo test -p <crate> --test <name>` in between.
- `python -I` ignores `PYTHONIOENCODING`: print non-ASCII paths (the Booth packages' Japanese folder names) with
  `python -I -X utf8`. `xargs ... python -I -` takes stdin for the file list and leaves Python in the REPL; write
  the script to a file instead.
- Before calling a test a check, make each case one the code under test could fail: a freeze test whose source
  never moved while frozen passed with freezing switched off. Mutate the branch and see the test fail.
- Stopping a background job (TaskStop) can leave its children running (`timeout`, Unity); a second run then aborts
  with "another Unity instance is running with this project open". Find leftovers with PowerShell
  `Get-CimInstance Win32_Process` filtered by command line and stop them by PID; never stop every `Unity.exe` (the
  owner may have an Editor open). Git Bash has no `setsid`.
- `a && b >> file` in one Bash call skips the append when `a` fails (a `grep` with no match): check that an appended
  file changed. A background `cargo test` started before an edit compiles the old code; rerun after editing.
- Don't restore a file with `git checkout --` after a mutation check of a file you changed this session: it reverts to the commit and drops your uncommitted edits. Revert the mutation with the inverse edit.
