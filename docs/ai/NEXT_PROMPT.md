# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB: milestone M0.11 (Phase 0 gate, ADR 0078), visual half. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md and docs/ai/LESSONS.md, ADR 0082 (the CPU half, passed), the "Scorecard" and "Update" sections at the end of docs/ai/reviews/phase-0-health-check.md, LESSONS-render.md and LESSONS-unity.md. Work on main.

Order (STATUS.md):
1. #101: compiled avatars within a measured threshold of Unity's renders; none green yet (rest means 0.057 to 0.194, worst 0.487). Itemise the causes per export from corpus/local/scorecard/<name>/unity-parity.json and the images (`civbb-client parity ... --images`), fix the largest shared cause first, and decide from data the thresholds for a Unity reference (ADR). Blackbird's 0.35 worst states are a wing pose.
2. #104 (Av3Emulator animator check; cannot change ADR 0082's result) if time is left.
3. When #101 is decided, write the follow-up ADR closing the gate, close M0.11, then split epic #39.

Rules: one logical change per commit, `cargo fmt --all && cargo xtask ci` green before each (start Bash calls that pipe it with `set -o pipefail`), tests before or with features, decisions the docs don't make as ADRs, found work as issues. Measure only with nothing else compiling (`measure` pins itself, ADR 0080). Never commit corpus/local contents or Unity packages, cap parallel texture encoding at 4 threads, never stop Unity processes you did not start, and check that GitHub CI passes after pushing (Linux too).
```
