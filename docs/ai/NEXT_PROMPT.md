# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md, docs/ai/CODEMAP.md and docs/ai/LESSONS.md, then start M0.6 Parallel runtime and benchmark with #22: create `civbb-core` (engine layer; description, README, xtask layer already listed) with the per-avatar island runtime of design doc §5 step 5 (animator, then constraints, then an IK stub, then bone dynamics, one job per avatar, rayon across avatars per ADR 0003) plus the step 6 contact pass (`civbb_dynamics::contact::step`, feeding receiver values into next frame's animator parameters), with per-system timing counters. Move the duplicated math helpers (`civbb-dynamics::math`, the private ones in `civbb-anim::constraint`) into `civbb-core` where the layering allows it, and record any runtime decision the design doc does not make in an ADR. Then #23 (headless N-avatar CPU benchmark with core scaling, JSON output). Keep `cargo xtask ci` green, commit and push per issue, check the fuzz workflow after pushes that touch parsers, and when you stop, update STATUS.md, LESSONS.md and this NEXT_PROMPT.md and push.
```

## For the human, optional

- More Booth avatars: drop the zips in `corpus/local/` and ask the session to convert them (`corpus/README.md`).
- Local fuzzing is optional (CI runs it): `rustup toolchain install nightly`, `cargo install cargo-fuzz`, see `fuzz/README.md`.
