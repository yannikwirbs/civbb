# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md, docs/ai/CODEMAP.md and docs/ai/LESSONS.md, then start M0.9 Reports and parity with #34: a per-avatar report from `civbb build` (design doc §8 cost model, §21 Phase 0 "how success is measured"). Decide and record in an ADR: what the report holds (before/after stats per compiler stage: nodes, bones, meshes, draws, vertices, triangles per LOD level, blend shapes and deltas, materials, textures and bytes, animator layers, chains, contacts, constraints; package size), how CPU and GPU time per frame are measured for it (the pipeline may not depend on client or render: a `civbb-client` subcommand that reads the signed package and measures, as #57 asks, or another split; release builds only, GPU optional and skipped without a device), and the Markdown and JSON shapes (stable keys, versioned). Prove it: a golden JSON for the lean corpus avatar (counts only, timings excluded), the report for every local avatar in corpus/local, and timings that match `civbb-client bench` and `view --frames` within noise. Then continue with #57 if it is not already covered. Keep `cargo xtask ci` green (CI runners have a Vulkan loader but no driver: device tests must skip), commit and push per issue, check that CI passes after each push, and when you stop, update STATUS.md, LESSONS.md and this NEXT_PROMPT.md and push.
```

## For the human, optional

- More Booth avatars: drop the zips in `corpus/local/` and ask the session to convert them (`corpus/README.md`).
- Local fuzzing is optional (CI runs it): `rustup toolchain install nightly`, `cargo install cargo-fuzz`, see `fuzz/README.md`.
