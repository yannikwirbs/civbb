# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md, docs/ai/CODEMAP.md and docs/ai/LESSONS.md, then continue M0.8 Compiler optimisations with #29: merge meshes that share a material and toggle together, so avatars have fewer skinned meshes and draws without changing how any toggle state looks (design doc §9 stage 3). Run it in `civbb build` after `prune` (ADR 0026, `crates/pipeline/src/prune.rs`). Meshes are referenced by index (face `ShapeBind`s, `civbb-anim` targets) and by node path (clip curves: `Active`, `RendererEnabled`, `BlendShape` by name, `Material` per renderer per ADR 0025), so decide what "toggle together" means (same node activity across every animator state, same renderer-enabled curves, no per-renderer material animation that would leak onto merged parts), how blendshapes and skins combine (joint lists merge, shape names may collide) and what first-person visibility requires. Prove it behaviour-identical: compare islands' world poses, shape weights and per-submesh materials before and after merging on the corpus avatars, and check the draw count drops on the bloated avatar in `civbb-client view`. Record decisions in an ADR. Then #30 (collapse toggle layers into one direct blend tree) if time allows. Keep `cargo xtask ci` green (CI runners have a Vulkan loader but no driver: device tests must skip), commit and push per issue, check that CI passes after each push, and when you stop, update STATUS.md, LESSONS.md and this NEXT_PROMPT.md and push.
```

## For the human, optional

- More Booth avatars: drop the zips in `corpus/local/` and ask the session to convert them (`corpus/README.md`).
- Local fuzzing is optional (CI runs it): `rustup toolchain install nightly`, `cargo install cargo-fuzz`, see `fuzz/README.md`.
