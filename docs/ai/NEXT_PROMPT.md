# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md, docs/ai/CODEMAP.md and docs/ai/LESSONS.md, then continue M0.5 Dynamics, contacts, constraints with #20 (contact senders and receivers with broadphase): research VRChat's documented contact semantics (receiver types constant, on-enter and proximity, tags, local-only, allow self/others) from its public documentation only, record the model in an ADR marking anything unverified, and give it hand-derived conformance tests. Contacts are a cross-avatar pass (design doc §5 step 6), so decide where the broadphase lives without breaking the layering rules (`cargo xtask arch`). Then #21 (constraints, topologically sorted at compile time) and #46 (VRM node constraints), in order, and close M0.5. Where Unity behaviour can settle a question (Unity constraints, ADR 0013), record it with tools/unity-truth in batch mode. Keep `cargo xtask ci` green, commit and push per issue, check the fuzz workflow after pushes that touch parsers, and when you stop, update STATUS.md, LESSONS.md and this NEXT_PROMPT.md and push.
```

## For the human, optional

- More Booth avatars: drop the zips in `corpus/local/` and ask the session to convert them (`corpus/README.md`).
- Local fuzzing is optional (CI runs it): `rustup toolchain install nightly`, `cargo install cargo-fuzz`, see `fuzz/README.md`.
