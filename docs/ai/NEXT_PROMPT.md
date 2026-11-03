# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md, docs/ai/CODEMAP.md and docs/ai/LESSONS.md, then finish M0.8 Compiler optimisations with #33: a skin-aware LOD chain with meshoptimizer (design doc §9 stage 3 and §8 degradation ladder). Decide and record in an ADR: the binding (the `meshopt` crate or another; licence, build on Windows and the Linux runners, determinism), how many levels and at which target ratios, how LODs keep skinning, blendshapes (shape deltas on simplified vertices, or none on lower levels), UV seams and material boundaries intact, where the levels live in the format (a format version bump if needed) and which stage runs it (after `merge`, ADR 0027, before `compress`, ADR 0029). Prove it: error metrics per level within the chosen bounds on the corpus and the local avatars, triangle counts fall per level, every level skins like the full mesh at its kept vertices (step islands as in tests/merge.rs), and the viewer can show a chosen level (a `--lod N` flag) with a window capture of AliciaSolid at each level per LESSONS. Keep `cargo xtask ci` green (CI runners have a Vulkan loader but no driver: device tests must skip), commit and push per issue, check that CI passes after each push, close M0.8 when #33 is done, and when you stop, update STATUS.md, LESSONS.md and this NEXT_PROMPT.md and push.
```

## For the human, optional

- More Booth avatars: drop the zips in `corpus/local/` and ask the session to convert them (`corpus/README.md`).
- Local fuzzing is optional (CI runs it): `rustup toolchain install nightly`, `cargo install cargo-fuzz`, see `fuzz/README.md`.
