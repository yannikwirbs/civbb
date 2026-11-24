# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB: Phase 1, milestone M1.1 (Native content model). Phase 0's gate passed (ADR 0084) and epic #39 is split into M1.1 to M1.9. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md and docs/ai/LESSONS.md, then issue #100 in full and design doc §9 (Native content format and content compiler; read by heading), and the STATE.md sections for the compiler and the format. Work on main.

#100 is a design question before it is code: the content model today is Unity's and VRChat's data model written as Rust types (bindings, Mecanim behaviour, shader-shaped materials such as `Toon`'s lilToon and Poiyomi fields, one global FORMAT_VERSION, state as the animator's parameter table). Nothing is published, so changing it costs nothing now. Steps:
1. Survey what #100 lists against the code (`crates/format`, the `X_*` extensions, ADR 0041, ADR 0043) and the parts Phase 1 will consume (protocol state sync, scripting, worlds). Note what must change before M1.2 builds on it and what can wait.
2. Write the decisions as ADRs (spec and schema, per-component versioning and migrations, platform state and events, native bindings, materials as a native model with Unity families lowered into it), each with options and the one chosen. Ask the owner (needs-user issue) only where the design doc leaves a product choice open.
3. Split the agreed work into issues in M1.1, smallest first, and start the first one: tests before or with the change; the parity table in ADR 0084 must not regress (Yumeka's rest worst view 0.1197 has no margin).

Rules: one logical change per commit, `cargo fmt --all && cargo xtask ci` green before each (start Bash calls that pipe it with `set -o pipefail`), decisions the docs don't make as ADRs, found work as issues in the matching M1.x milestone. Never commit corpus/local contents or Unity packages, never stop Unity processes you did not start, and check that GitHub CI passes after pushing (Linux too).
```
