# CIVBB: instructions for AI sessions

CIVBB is a social VR platform on a custom Rust + Vulkan engine. The product spec is `docs/design-doc.md` (large; read only the section you need, by heading). This project is built entirely by AI sessions. The human owner (GitHub `yannikwirbs`) only debugs and does the things listed under "needs-user". You make the engineering decisions; record them as ADRs.

## Start of every session

1. Read `docs/ai/STATUS.md` (milestone, **Next**, blockers) and `docs/ai/LESSONS.md` (general gotchas).
2. `tools/ci.sh`: GitHub CI of the last push. If it is red, fix that first.
3. `tools/issue.sh show <N>` for the issue STATUS.md's Next names (`tools/issue.sh queue` lists the milestone). Before working in an area, read its section of `docs/ai/STATE.md` (`grep -n '^## '`, then by range) and its `docs/ai/LESSONS-<topic>.md` (listed in LESSONS.md).
4. Find code with `grep -n <word> docs/ai/CODEMAP.md` (one line per file: its job). Open files by range, not whole.

## Session shape (ADR 0092)

- One session takes one issue or one chain of dependent issues. End it when the chain is done or when the context hook reports more than 250 k tokens, at the next clean point. Every call re-reads the whole context, so a long session costs far more per step than a fresh one.
- Knowledge for the next session goes into STATUS.md's Next, issue comments (work in progress) or STATE.md, never only into chat.
- Don't wait idle: run long jobs (parity, Unity, release builds, studies) in the background and do other work meanwhile. Don't watch GitHub CI after pushing; the next session checks it.

## Working loop

- Take the issue in STATUS.md's Next, else the lowest-numbered open one in the current milestone. Comment on the issue when work spans sessions.
- Trunk-based: small commits straight to `main`, message `area: what (#N)`; `Closes #N` in the commit that finishes an issue.
- Gate before every commit: `cargo fmt --all && cargo xtask ci` (fmt, clippy `-D warnings`, tests, arch rules, source rules, code map freshness). Never commit red; start a Bash call that pipes the gate with `set -o pipefail`.
- Enforced by that gate: every `unsafe` block has a `// SAFETY:` comment above it; no Rust file over 1,000 lines (split along a real seam); no `unwrap()` in a crate's `src/` before its first `#[cfg(test)]` (use `?` or an `expect` saying why it cannot fail; keep test modules last).
- New file: start it with a `//!` line saying its one job (the code map uses it). New crate: `description` in Cargo.toml, a `README.md`, and a layer in `tools/xtask/src/main.rs`.
- A decision the design doc doesn't make, or a deviation from it: write an ADR in `docs/adr/` (copy `0000-template.md`). ADRs are not edited after acceptance; supersede them (list in `docs/adr/README.md`).
- Found work outside the current issue: `tools/issue.sh new <milestone> <labels> <title> < body`, don't do it now.
- Something the human must do (install, hardware test, licence call): an issue labelled `needs-user`, listed in STATUS.md.
- Sessions may run Unity themselves (`tools/unity-batch.sh`; `LESSONS-unity.md`) on projects inside this repo. Never drive an Editor the human has open on another project.

## How to iterate

Use these helpers, not raw command lines: each prints one line on success and only the relevant lines on failure.

- Build: `cargo build --workspace` (dev profile: workspace crates at opt-level 1, dependencies at 2). `--release` only for timings.
- Quick test: `cargo xtask test` (crates changed since `HEAD` and their dependents). One test file: `cargo test -p <crate> --test <file> --quiet`.
- Full gate before every commit: `cargo fmt --all && cargo xtask ci`. `.githooks/pre-push` runs it on push (once per clone: `git config core.hooksPath .githooks`).
- Unity: `tools/unity-batch.sh <project> <Class.Method> [args]` (the method must exit itself; `Muscles.Record` still needs `unity run`).
- Parity against Unity: `tools/parity.sh [--against <old dir>] <out dir> [name regex]` (all exports ~25 min: background it). Numbers are printed unrounded.
- GitHub: `tools/issue.sh show|queue|new`, `tools/ci.sh [commit]` (failed jobs' error lines only).
- Edit with the Edit tool, not Python heredocs (escape bugs, LESSONS.md).

## End of every session (always, even when stopping early)

1. `cargo xtask codemap`, then `cargo fmt --all && cargo xtask ci`.
2. `docs/ai/STATUS.md`: Now, **Next** (the issue to take and what the next session must know that the issue doesn't say), blockers and needs-user, the newest three sessions. One line in `docs/ai/SESSIONS.md`. The `docs/ai/STATE.md` sections of areas you changed.
3. New gotchas: `docs/ai/LESSONS.md` if general, else the matching `LESSONS-<topic>.md`.
4. Commit and push; don't wait for CI. The owner starts the next session with `/continue`.

## Hard rules (from the design doc; the doc wins on conflict)

- Layering (enforced by `cargo xtask arch`): `format`, `protocol`, `wit` depend on nothing internal. Engine crates (`core`, `render`, `anim`, `dynamics`, `audio`, `xr`, `ui`, `script-host`) depend only on `core` and `format`. `client`, `node`, `pipeline` are glue; nothing depends on them. `node` never depends on `render`.
- Untrusted content is data, never code. Parsers of untrusted input are Rust, bounded, and fuzzable.
- No VRChat code, SDK, branding or UI copies. Never accept or fetch built bundles (`.vrca`, `.vrcw`) or anything from VRChat's API/CDN.
- Third-party avatars live in `corpus/local/` (gitignored) and never get committed. Only CC0 or generated assets go in the repo.
- Licences: everything MIT OR Apache-2.0, except `crates/node` which is AGPL-3.0-only.
- Minimum code that solves the issue. Boring, widely used dependencies; give each new dependency a one-line reason in the commit message.
- Tests are the spec: conformance, golden files and benchmarks come before or with features.

## Environment

Windows 11, Git Bash (the PowerShell tool is off, ADR 0091: run `powershell -NoProfile -Command ...` from Bash). i5-12600K (6P+4E), RTX 5070 Ti, 32 GB RAM, the design doc's reference PC. Rust MSVC via rustup, version pinned by `rust-toolchain.toml` (ADR 0045; bump it deliberately). Unity 6000.3.28f1 (Personal licence active); never hardcode an Editor path, ask `unity editors` / `unity editors path <version>`. Unity CLI at `C:/Users/xxsay/AppData/Local/Unity/bin/unity.exe`. No VRChat SDK or Creator Companion.
