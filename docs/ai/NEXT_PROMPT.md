# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB: milestone M0.11 (Phase 0 gate, ADR 0078), visual half. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md and docs/ai/LESSONS.md, ADR 0083 (Unity parity thresholds and the itemised red states), the "Update (#101)" section at the end of docs/ai/reviews/phase-0-health-check.md, LESSONS-render.md and LESSONS-unity.md. Work on main.

Order (STATUS.md):
1. #101 closes when no export's rest state is red under ADR 0083. Three are: #105 (Cokoro_Quest: VRChat Mobile Toon Standard's ramp texture, albedo-coloured shadow, minimum brightness; source in corpus/local/unity-export-vrc/Packages/com.vrchat.base/.../Shaders/Mobile/ToonStandard), #106 (KohzieBoi: Poiyomi 7.3's second matcap with replace blend, UV1 masks and emission; source in corpus/local/unity-export-vrc/Assets/_PoiyomiShaders) and #108 (Yumeka: silhouette offsets at tail, hair and skirt hem, likely PhysBone rest). Map from the shader source, score with `civbb build` then `civbb-client parity corpus/local/unity-ref/<name> <pkg> --key corpus/local/unity-ref/key --images <dir>`, and post before/after on the issue.
2. When no rest state is red: rerun all 13 (STATUS.md has the commands), post the table on #101 and close it, write the follow-up ADR closing the gate (ADR 0078: CPU half ADR 0082, visual half ADR 0083 plus the final table), close M0.11, then split epic #39.
3. #104 (Av3Emulator animator check; cannot change ADR 0082's result) if time is left.

Rules: one logical change per commit, `cargo fmt --all && cargo xtask ci` green before each (start Bash calls that pipe it with `set -o pipefail`), tests before or with features, decisions the docs don't make as ADRs, found work as issues. Measure only with nothing else compiling (`measure` pins itself, ADR 0080). Never rebuild `target/release` while a parity batch uses it. Never commit corpus/local contents or Unity packages, cap parallel texture encoding at 4 threads, never stop Unity processes you did not start, and check that GitHub CI passes after pushing (Linux too).
```
