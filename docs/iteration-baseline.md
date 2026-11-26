# Iteration baseline

What one iteration (change request, edit, build, test, result) cost before the 2026-11-18 tuning, and what changed.
Reference PC (i5-12600K, 16 threads, RTX 5070 Ti), Windows 11, Rust stable MSVC, `corpus/local` present. Tokens are
estimated as bytes / 4.

## Build and test timings

All warm timings in the repo's own `target/`, run alone (nothing else compiling).

| Measurement | Command | Time |
| --- | --- | --- |
| Clean build (fresh target dir) | `cargo test --workspace --no-run` | 119 s |
| No-op build | `cargo test --workspace --no-run` | 4.0 s |
| Incremental, one line in `crates/format/src/lib.rs` (everything depends on it) | `cargo test --workspace --no-run` | 18.4 s |
| Incremental, one line in `crates/pipeline/src/report.rs` (a glue crate) | `cargo test --workspace --no-run` | 6.5 s |
| Incremental, one line in `crates/anim/src/curve.rs` | `cargo build --workspace` | 3.4 s |
| Full test suite, binaries built | `cargo test --workspace --quiet` | 82.1 s |
| Smallest run covering one module | `cargo test -p civbb-anim --test curve --quiet` | 5.6 s |
| Full gate | `cargo xtask ci` | 88.0 s |

The test suite is 45 test binaries; cargo runs them one after another. Three single tests are 67 of the 82 s:

| Test | Time |
| --- | --- |
| `civbb-pipeline` `parity::bloated_compiled_matches_its_import_in_every_state` | 25.6 s |
| `civbb-pipeline` `lod::corpus_avatars_keep_their_look` | 23.2 s |
| `civbb-pipeline` `merge::corpus_avatars_look_the_same` | 18.4 s |
| `civbb-pipeline` `lod::local_avatars_keep_their_look` (local data only) | 13.0 s |
| `civbb-pipeline` `merge::local_avatars_look_the_same` (local data only) | 8.8 s |

They step avatar islands and skin every vertex on the CPU in workspace crates, which the dev profile compiles at
opt-level 0 (dependencies were already at 2). Experiment with `CARGO_PROFILE_DEV_OPT_LEVEL`: opt-level 1 gave a full
test run of 19.4 s, opt-level 2 18.7 s at a 14 s slower rebuild of the workspace crates.

## Context loaded per session

Loaded automatically:

| Source | Bytes | ~Tokens |
| --- | --- | --- |
| `~/.claude/CLAUDE.md` (global) | 532 | 130 |
| `CLAUDE.md` (project) | 4,006 | 1,000 |
| Memory (`MEMORY.md`), rules, imports | none | 0 |

Read by the session protocol in `CLAUDE.md` before any work:

| File | Bytes | ~Tokens |
| --- | --- | --- |
| `docs/ai/STATUS.md` (13 KB of it is a per-area narrative of everything built, 6 KB the full session log) | 20,762 | 5,200 |
| `docs/ai/CODEMAP.md` (read whole) | 21,868 | 5,500 |
| `docs/ai/LESSONS.md` (77 lessons, any area, no sections) | 21,333 | 5,300 |
| `docs/ai/NEXT_PROMPT.md` (pasted by the owner) | 2,221 | 550 |
| Total | 70,190 | 17,500 |

`docs/design-doc.md` (84 KB) is read by section only. `target/`, `corpus/local/` and the Unity `Library/` folders
are gitignored, so the Grep and Glob tools (ripgrep) already skip them.

## Tool use in the last 10 sessions

From the transcripts under `~/.claude/projects/` (10 sessions, 2026-11-03 to 2026-11-07).

Calls: Bash 1,395, Edit 162, Write 79, Read 67, PowerShell 19, other 13.

Most frequent Bash patterns:

| Pattern | Calls | Note |
| --- | --- | --- |
| `python - <<'EOF'` / `'PY'` / `'PYEOF'` heredocs | 190 | mostly file edits by string replace; three lessons record escape bugs from them |
| `export PATH="$HOME/.cargo/bin:$PATH" && ...` | ~90 | required by CLAUDE.md step 5; `cargo` is on PATH in these shells |
| `S=<scratchpad>; ...` analysis scripts | ~160 | per-session corpus and Unity analysis |
| `grep -n` / `sed -n` chains | ~250 | reading code in slices (`sed -n -> sed -n` 86 times in a row) |
| `gh issue ...`, `gh run list/watch` | ~70 | queue and CI watching |
| Unity batch runs (`-executeMethod`) | 23 | each rebuilt the editor path, log path and log grep by hand |
| `cargo test -p ... --test ...` | ~30 | targeted runs between gates |

Tool output by kind (characters, all 10 sessions): ad-hoc Bash 1.44 M, Read 382 K, Unity runs and log greps 179 K,
`cargo test` 45 K, `gh` 32 K, `git` 11 K, `cargo xtask ci` 3 K.

Session start: in full (non-resumed) sessions the protocol reads put 25 to 60 KB of `docs/ai` into context before
the first edit (e.g. 59,892 chars in e446bce0, 46,635 in 8d7ced96, 42,149 in 3c85b956).

Largest single outputs (over 15 K characters): `STATUS.md` + `LESSONS.md` read together (24 to 28 K, five
sessions), `CODEMAP.md` whole (21 K, two sessions), `tools/unity-exporter/Editor/Glb.cs` and `AnimatorExport.cs`
read whole (26 K each, five times), a Unity import log grep (16 K), per-node debug dumps from a test (16 K).

A passing `cargo xtask ci` prints 281 lines (7.1 KB, ~1,800 tokens): one `running N tests` / `test result` block
per test binary plus cargo's `Checking` lines.

## Changes

Kept, each in its own commit with the numbers in the message. Times are warm unless marked clean.

| change | before | after |
| --- | --- | --- |
| Workspace crates at opt-level 1 in dev builds (`Cargo.toml`) | full tests 82.1 s; gate 88.0 s; incremental format 18.4 s, pipeline 6.5 s; clean 119 s | full tests 13.9 s; gate 22.6 s; incremental format 13.1 s, pipeline 4.9 s; clean 119 s |
| Session state split by read frequency (ADR 0044) | 70,190 B (~17,500 tokens) read at session start | 15,938 B (~4,000 tokens); 24,795 B (~6,200) with the current area's STATE section and topic lessons |
| `cargo xtask ci` prints one line, failures filtered | passing gate 7,121 B; one failing test 1,828 B | 93 B; 493 B |
| `cargo xtask test`: changed crates and dependents only | `cargo test --workspace --quiet` after a pipeline change 17.7 s, 6,235 B | 11.2 s, 72 B (anim or format change: same time ±1 s, 97 to 162 B) |
| `tools/unity-batch.sh` | per Unity run: 344 B command, ~4,000 B log grep | 79 B command, 143 B summary |
| CLAUDE.md "How to iterate", PATH prefix step dropped (`cargo` is on PATH) | ~90 `export PATH=...` prefixes per 10 sessions | none required |

Measured and reverted:

| change | before | after |
| --- | --- | --- |
| `debug = "line-tables-only"` in dev builds | incremental format 13.1 s, pipeline 4.9 s; clean 119 s | 13.6 s, 5.3 s; clean 154 s |
| `rust-lld` instead of MSVC `link.exe` | incremental format 13.1 s, pipeline 4.9 s | 16.3 s, 6.0 s |

Not pursued: merging each crate's integration tests into one binary. After a format change the 44 test targets
take 149 of 186 CPU seconds, but they link in parallel; the wall-time path is the pipeline library (7.0 s) and then
one test binary (at most 6.7 s), which merging would not shorten. The slowest test is now
`parity::bloated_compiled_matches_its_import_in_every_state` at 6.5 s (GPU with validation in debug builds).

## Session cost baseline (2026-11-26)

Measured with `python tools/session-stats.py --since 2026-11-19` (repeat it the same way): the 20 sessions after the
iteration tuning, 2026-11-19 to 2026-11-24. Tokens are the API's usage counts. Dollars use per-token prices fitted
to Claude Code's own `cost-state` totals (Opus 5.5: $4 input, $20 output, $0.20 cache read, $8 cache write per
Mtok; fitted and reported totals agree within 2 %). All sessions ran `opus[1m]` at high effort; every cache write
used the 1-hour TTL and only one call missed the cache, so cache writes are simply new content.

| Measure | Total | Per closed issue |
| --- | --- | --- |
| Cost | $295 (mean $14.75 per session) | $9.2 |
| Issues closed (`Closes #N` in the session's commits) | 32 (median 2 per session) | |
| Cache-read tokens | 900 M | 28.1 M |
| Cache-write tokens | 7.0 M | 219 k |
| Output tokens (thinking 1.12 M of them) | 2.95 M | 92 k |
| API calls (main thread; subagents: 1 call in 20 sessions) | 3,539 | 111 |
| Tool calls / tool output | 3,955 / 6.1 MB | 124 / 190 KB |
| Agent time (wall minus time waiting for the owner) | 20.1 h | 38 min |
| of it: API (model) time / tool time | 9.1 h / 7.4 h | |

Where the money goes: cache reads $180 (61 %), cache writes $56 (19 %), output $59 (20 %). Each call re-reads the
whole context, so a session's cost grows with the square of its length: the mean call carried 254 k tokens, the mean
session peaked at 359 k (max 572 k), and no session compacted. Cache-read dollars by context size at the call:
0-100 k $5, 100-200 k $29, 200-300 k $51, 300-400 k $49, over 400 k $47.

What a call carries:

- **Base context, 51 k tokens before anything is read** (20 % of the mean call). Mostly the harness: tool schemas
  69 KB (Artifact 26 KB plus 30 KB of Artifact instructions, never used in this repo; PowerShell 10 KB, used 10
  times; AskUserQuestion, ScheduleWakeup, ReportFindings, ListAgents 15 KB), skill listings ~20 KB (Unity plugin
  8 KB; its skills were called 3 times in 47 sessions), MCP and browser instructions ~8 KB. CLAUDE.md is 5.6 KB.
- **Tool output**, about 30 % of a session's final context. Carried cost (written once, re-read by every later
  call): reading code with Bash $27 (`sed -n` slices 1.5 MB, `cat` of whole files 0.8 MB, `grep` 0.7 MB), python
  $8, Read $5, Unity $2, `gh issue` $2, parity $2; everything else under $2 each.
- **The session's own output**, about 45 %: code written through Write, Edit and heredocs, text, and thinking
  (38 % of output tokens).

Session start (until the first edit outside `docs/`), mean of 19 sessions: 18 calls, $1.34, 4 min, 104 KB of tool
output (61 k tokens). Of that 104 KB: code and data 35 KB, `STATE.md` sections 15 KB, `STATUS.md` + `LESSONS.md`
11 KB, `LESSONS-<topic>.md` 8 KB (several topic files read whole, up to 26 KB), other Reads 10 KB, `gh issue` 5 KB,
design doc 4 KB, ADRs 4 KB. Session end (docs upkeep after the last code edit): 13 calls, $1.21, 9 min.

Wall time, tool side (minutes over the 20 sessions): Unity 84, gate (`cargo xtask ci`, 142 runs, 33 s mean) 79,
polling loops and `sleep` waiting on background jobs 56, parity 50, `cargo build` (mostly release builds for parity)
40, `civbb` runs 43, GitHub CI watching (`gh run watch` / `list`) 36, `cargo test` 30. The warm gate takes 19 s
alone (33 s on the first run after a build).

Other findings:

- `rust-analyzer-lsp` is enabled in the user settings, but `rust-analyzer` is not installed (not a component of the
  pinned toolchain), so the LSP tool has never worked; sessions navigate with `grep -n` and `sed -n`.
- Errors are rare: 47 failed tool calls in 3,955 (python heredocs 26, gate 7, Unity 6), not a cost driver.
- Most-read files across sessions: `unity_material.rs`, `solver.rs`, `Reference.cs`, `STATE.md` (each re-read in
  2 to 4 sessions, 19 to 53 KB in all).
- Tool output sizes are already in the transcripts (`toolUseResult`), so a logging hook would duplicate them.

## Changes (2026-11-26)

Each in its own commit with an ADR where it is a process decision. Measured on the reference PC the same day.

| change | before | after |
| --- | --- | --- |
| Project settings deny unused tools and the Unity plugin (ADR 0091) | first-call context in this repo 47,262 tokens | 22,631 tokens (-25 k on every call; ~7 % of the measured cost) |
| Read every session: CLAUDE.md, STATUS.md, LESSONS.md and the pasted prompt; `/continue` replaces `NEXT_PROMPT.md` (ADR 0092) | 21,181 B | 14,927 B (`LESSONS-method.md` and `docs/adr/README.md` read on demand) |
| Gate tests with cargo-nextest (ADR 0093) | warm gate 19 to 20 s (tests 18.7 s) | 12 s (tests 9.5 s + doctests 1.4 s); a failing test prints 806 B (was 644 B) |
| `tools/ci.sh` | failed CI job, `gh run view --log-failed`: 100,768 B | 1,173 B; GitHub CI is no longer watched after a push (36 min over 20 sessions) |
| `tools/parity.sh` | per export: `parity.json` 29,549 B or a hand-written summary that rounded to 3 digits | one 263 B line, numbers copied unrounded |
| `tools/issue.sh show` | two calls (`gh issue view`, `--comments`: 1,031 + 1,909 B) | one call, 2,721 B |
| LSP through rust-analyzer (ADR 0094) | references of `Machine::start` with `grep`: 503 B, wrong hits | 123 B, exact (needs ~20 s of indexing after the first call) |
| Context hook and status line, `tools/context.py` (ADR 0092) | sessions could not see their context size | note past 250 k tokens; 85 ms per Bash/Read/Grep/Glob/Agent call |

Measured and dropped: turning off claude.ai connectors (`ENABLE_CLAUDEAI_MCP_SERVERS=false`) or Chrome changed the
first-call context by under 10 tokens; a hook logging tool output sizes would duplicate the transcripts; rust-lld was
already slower (above); no subagent definitions (ADR 0092).

Per closed issue, measured 2026-11-26 with `python tools/session-stats.py --since 2026-11-26` over 9 sessions
(#115 to #122 and #128; the last session before its end phase): $5.5 (fitted $49, Claude Code's own total $45)
and 13.6 M cache-read tokens (122 M), against $9.2 and 28.1 M before: -40 % and -52 %. The simulation predicted 8
to 11 % from the 250 k limit alone; the rest comes from one issue chain per session and the helpers above, and
from M1.1's issues being more mechanical than Phase 0's. Peak context stayed under 256 k in every session; at
these sizes the script finds a lower limit would cost more (150 k: +16 %, 200 k: +4 %).

Recommendation, not applied (the owner's setting): thinking is 1.12 M of 2.95 M output tokens and stays in the
context. Output is 20 % of the cost; trying `medium` effort on two or three sessions of mechanical issues, compared
by dollars per closed issue with the script, would show whether it saves the estimated 5 to 10 % without more
failed gates.
