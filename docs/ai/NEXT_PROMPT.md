# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md, docs/ai/CODEMAP.md and docs/ai/LESSONS.md, then work through the open issues of the current milestone (M0.4 Animation runtime, starting with #13) in order. `anim` is an engine crate: it may depend only on `core` and `format`. Write the conformance tests (Unity key semantics, Write Defaults) before or with each feature. Keep `cargo xtask ci` green, commit and push per issue, check the fuzz workflow after pushes that touch parsers, and when you stop, update STATUS.md, LESSONS.md and this NEXT_PROMPT.md and push.
```

## For the human, optional

- More Booth avatars: drop the zips in `corpus/local/` and ask the session to convert them (`corpus/README.md`).
- Local fuzzing is optional (CI runs it): `rustup toolchain install nightly`, `cargo install cargo-fuzz`, see `fuzz/README.md`.
