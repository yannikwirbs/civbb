# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB: finish the Phase 0 health check. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md and docs/ai/LESSONS.md, then docs/ai/reviews/phase-0-health-check.md in full (findings, plan, and the "Progress" section at the end) and LESSONS-parsers.md. Work on the branch `health/phase-0` (`git checkout health/phase-0`), not on main.

The task, unchanged from the last session: bring the codebase into a state Phase 1 can build on. Find problems, decide, fix, verify rigorously (nobody reviews), one logical change per commit, `cargo xtask ci` green after every commit (start Bash calls that pipe it with `set -o pipefail`), refactors behaviour-preserving with the behaviour pinned before them, decisions the docs don't cover as ADRs, anything too big as an issue with a concrete plan. Phase 0's gate is not passed (ADR 0078); milestone M0.11 (#38 Unity CPU baselines, #101 parity against Unity) comes before Phase 1, and epic #39 is not split yet.

Do the report's "Still to do" list in order:
1. Run `python corpus/scorecard.py` alone (release build first, nothing else compiling; about 40 minutes, so work on step 2's reading meanwhile without compiling). Put the table in the report and comment it on #101. If a step fails, fix the cause, not the script's error handling.
2. Structure: math helpers onto civbb_core::math (vrm.rs, client::cull, the Mat4 multiply in render::forward and client::view); split validate::V::avatar per avatar part; split gltf_import/vrm.rs and gltf_import.rs along their seams. Before each refactor, record a behaviour pin with a scratch, uncommitted test (import results for every corpus/local vrm/glb/unity-glb/khronos file plus a few thousand digit-mutated source GLBs and VRMs and byte-mutated CBOR avatars through validate) and compare it after: identical output or no commit.
3. Cosmetics: the raw newline in a string literal in crates/pipeline/src/main.rs; civbb-dynamics' Cargo description ("XPBD").
4. Guardrails: apply docs/ai/reviews/phase-0-xtask-rules.patch (cargo xtask rules: files at most 1,000 lines, no unwrap() outside tests), then delete the patch; add clippy undocumented_unsafe_blocks to the workspace lints after adding // SAFETY: comments to the 13 test unsafe blocks in crates/render/tests; document both in CLAUDE.md.
5. Finish: complete the report (goal-alignment verdict, scope table, fixed, deferred with issue links, ADRs, guardrails, before/after metrics: CI, largest files and functions, dependency count; baseline numbers are at the top of the report), update STATUS.md, STATE.md, ARCHITECTURE.md, SESSIONS.md, LESSONS.md and this NEXT_PROMPT.md (next: M0.11 from #38), run `cargo xtask ci` on the branch, and if green merge health/phase-0 into main and push.

Never commit corpus/local contents, cap parallel texture encoding at 4 threads, never stop Unity processes you did not start, and check that GitHub CI passes after pushing (Linux too).
```
