# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md, docs/ai/CODEMAP.md and docs/ai/LESSONS.md, then work through the open issues of the current milestone (M0.4 Animation runtime, starting with #13) in order. `anim` is an engine crate: it may depend only on `core` and `format`. Write the conformance tests (Unity key semantics, Write Defaults) before or with each feature. Keep `cargo xtask ci` green, commit and push per issue, check the fuzz workflow after pushes that touch parsers, and when you stop, update STATUS.md, LESSONS.md and this NEXT_PROMPT.md and push.
```

## For the human, optional

- #11: download 3-5 free popular Booth avatars plus one well-optimised control avatar, export them as VRM, and drop them in `corpus/local/vrm/` (gitignored). `cargo test -p civbb-pipeline local_vrms -- --nocapture` then imports each and lists anything skipped. Three sample VRMs from vrm-c are already there.
- Local fuzzing is optional (CI runs it): `rustup toolchain install nightly`, `cargo install cargo-fuzz`, see `fuzz/README.md`.
