# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB: milestone M0.11 (Phase 0 gate, ADR 0078) before any Phase 1 work. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md and docs/ai/LESSONS.md, the "Result" and "Scorecard" sections at the end of docs/ai/reviews/phase-0-health-check.md, and LESSONS-unity.md. Work on main.

Order (STATUS.md):
1. #103: make `civbb-client measure` pin its bench worker to a performance core and report the core it ran on (ADR for the hybrid-CPU policy; one boring dependency at most, reason in the commit). Then drop the pinning from corpus/scorecard.py. Check: two consecutive runs of one package agree within 10 %.
2. #38: Unity CPU baselines, following the plan in the issue's last comment (batch method in tools/unity-exporter, ProfilerRecorder markers, 1 and 40 copies, AvatarOptimizer via VPM in a copy of the host project, Av3Emulator check). Pin Unity to the same core class as our measure. Add the Unity columns to corpus/scorecard.py and record in a follow-up to ADR 0078 whether "clearly cheaper" holds.
3. Then #102 (Blackbird's compiled package costs 33 % more CPU) and #101 (compiled avatars against Unity's renders; none green yet).

Rules: one logical change per commit, `cargo fmt --all && cargo xtask ci` green before each (start Bash calls that pipe it with `set -o pipefail`), tests before or with features, decisions the docs don't make as ADRs, found work as issues. Measure only with nothing else compiling, pinned. Never commit corpus/local contents or Unity packages, cap parallel texture encoding at 4 threads, never stop Unity processes you did not start, and check that GitHub CI passes after pushing (Linux too).
```
