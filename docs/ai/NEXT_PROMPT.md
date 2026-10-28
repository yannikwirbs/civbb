# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md, docs/ai/CODEMAP.md and docs/ai/LESSONS.md, then do #48 (humanoid muscle and root motion curves), the last open issue of M0.4 Animation runtime. Unity 6.3 is now installed and licensed (CLAUDE.md, Environment): consider an ADR superseding the hand-derived part of ADR 0002 so a small Unity project records ground truth (golden animator cases, and muscle-to-rotation via HumanPoseHandler) in batch mode. Start by researching Unity's muscle space (the muscle list with default ranges, and how a muscle value becomes a local bone rotation via the avatar's per-bone axes and limits) and record the model in an ADR, marking anything unverified. Muscles bind to slots in `civbb-anim` (ADR 0007) and need hand-derived conformance tests. Then close M0.4 and continue with M0.5 (#18 onwards) in order. Keep `cargo xtask ci` green, commit and push per issue, check the fuzz workflow after pushes that touch parsers, and when you stop, update STATUS.md, LESSONS.md and this NEXT_PROMPT.md and push.
```

## For the human, optional

- More Booth avatars: drop the zips in `corpus/local/` and ask the session to convert them (`corpus/README.md`).
- Local fuzzing is optional (CI runs it): `rustup toolchain install nightly`, `cargo install cargo-fuzz`, see `fuzz/README.md`.
