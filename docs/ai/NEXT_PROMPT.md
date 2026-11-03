# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md, docs/ai/CODEMAP.md and docs/ai/LESSONS.md, then continue M0.8 Compiler optimisations with #31: texture atlasing within one material model and KTX2 with Basis Universal (UASTC for normal maps and high-quality slots, ETC1S otherwise, per-platform size caps; design doc §6 and §9 stages 4 and 5). Decide and record in an ADR: the encoder (a Basis Universal binding or CLI; licence, build on Windows and on the Linux CI runners, determinism), how the client transcodes KTX2 on load (BC7 on desktop; the viewer currently skips KTX2 textures), which materials may share an atlas (same model, alpha mode, culling, sampler wrap; UVs outside 0..1 or `_ST` tiling, including animated `_MainTex_ST` per ADR 0025, cannot be atlased), and where atlasing runs relative to `merge` (ADR 0027): atlasing first lets more submeshes share one material and join. Prove it: atlased avatars sample the same texels (compare decoded colours at every vertex UV before and after, within the codec's error), packages shrink, and the viewer still renders AliciaSolid textured (window capture per LESSONS). Then #32 (PhysBone chain merging) if time allows. Keep `cargo xtask ci` green (CI runners have a Vulkan loader but no driver: device tests must skip), commit and push per issue, check that CI passes after each push, and when you stop, update STATUS.md, LESSONS.md and this NEXT_PROMPT.md and push.
```

## For the human, optional

- More Booth avatars: drop the zips in `corpus/local/` and ask the session to convert them (`corpus/README.md`).
- Local fuzzing is optional (CI runs it): `rustup toolchain install nightly`, `cargo install cargo-fuzz`, see `fuzz/README.md`.
