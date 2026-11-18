# CIVBB: instructions for AI sessions

CIVBB is a social VR platform on a custom Rust + Vulkan engine. The product spec is `docs/design-doc.md` (large; read only the section you need, by heading). This project is built entirely by AI sessions. The human owner (GitHub `yannikwirbs`) only debugs and does the things listed under "needs-user". You make the engineering decisions; record them as ADRs.

## Start of every session

1. Read `docs/ai/STATUS.md` (milestone, focus, blockers, last sessions) and `docs/ai/LESSONS.md` (general gotchas).
2. Before working in an area, read its section of `docs/ai/STATE.md` (what exists there) and its topic file `docs/ai/LESSONS-<topic>.md` (listed in LESSONS.md).
3. Find code with `grep -n <word> docs/ai/CODEMAP.md` (one line per file: its job) instead of walking the tree or reading the whole map. Open a file only when you need its contents.
4. Get the queue: `gh issue list --repo yannikwirbs/civbb --milestone "<current milestone>" --state open`.

## Working loop

- Take the lowest-numbered open issue in the current milestone unless STATUS.md says otherwise. Comment on the issue when you start if the work spans sessions.
- Trunk-based: small commits straight to `main`, message `area: what (#N)`; use `Closes #N` in the commit that finishes an issue.
- Gate before every commit: `cargo xtask ci` (fmt, clippy `-D warnings`, tests, arch rules, code map freshness). Never commit red.
- New file: start it with a `//!` line saying its one job (the code map uses it). New crate: `description` in Cargo.toml, a `README.md`, and a layer in `tools/xtask/src/main.rs`.
- A decision the design doc doesn't make, or a deviation from it: write an ADR in `docs/adr/` (copy `0000-template.md`). ADRs are not edited after acceptance; supersede them.
- Found work outside the current issue: open an issue with labels and milestone, don't do it now.
- Something the human must do (install, hardware test, licence call): open an issue labelled `needs-user` and list it in STATUS.md.
- Sessions may run Unity themselves (batch mode via `unity run`, or the Editor binary from `unity editors path` for play-mode methods; `LESSONS-unity.md`) on projects inside this repo. Never drive an Editor the human has open on another project.

## End of every session (always, even when stopping early)

1. `cargo xtask codemap`, then `cargo xtask ci`.
2. Update `docs/ai/STATUS.md` (focus, blockers, needs-user, the newest three sessions), append one line to `docs/ai/SESSIONS.md`, and update the `docs/ai/STATE.md` sections of the areas you changed.
3. Add any new gotcha to `docs/ai/LESSONS.md` (general) or the matching `LESSONS-<topic>.md`.
4. Rewrite `docs/ai/NEXT_PROMPT.md` with the exact prompt the human should paste next.
5. Commit and push.

## Hard rules (from the design doc; the doc wins on conflict)

- Layering (enforced by `cargo xtask arch`): `format`, `protocol`, `wit` depend on nothing internal. Engine crates (`core`, `render`, `anim`, `dynamics`, `audio`, `xr`, `ui`, `script-host`) depend only on `core` and `format`. `client`, `node`, `pipeline` are glue; nothing depends on them. `node` never depends on `render`.
- Untrusted content is data, never code. Parsers of untrusted input are Rust, bounded, and fuzzable.
- No VRChat code, SDK, branding or UI copies. Never accept or fetch built bundles (`.vrca`, `.vrcw`) or anything from VRChat's API/CDN.
- Third-party avatars live in `corpus/local/` (gitignored) and never get committed. Only CC0 or generated assets go in the repo.
- Licences: everything MIT OR Apache-2.0, except `crates/node` which is AGPL-3.0-only.
- Minimum code that solves the issue. Boring, widely used dependencies; give each new dependency a one-line reason in the commit message.
- Tests are the spec: conformance, golden files and benchmarks come before or with features.

## Environment

Windows 11, PowerShell and Git Bash. i5-12600K (6P+4E), RTX 5070 Ti, 32 GB RAM. This matches the design doc's reference PC. Rust stable MSVC via rustup. Unity 6000.3.28f1 (Personal licence active); never hardcode an Editor path, ask `unity editors` / `unity editors path <version>`. Unity CLI at `C:/Users/xxsay/AppData/Local/Unity/bin/unity.exe`. No VRChat SDK or Creator Companion.
