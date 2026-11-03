# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md, docs/ai/CODEMAP.md and docs/ai/LESSONS.md, then finish M0.7 Renderer and viewer with #49: animated material properties. Bind `Property::Material` curves (reported unbound by `civbb-anim` today) to fields of the native material models: map the Unity shader property names the common avatar shaders animate (lilToon, Poiyomi, Standard, MToon: `_Color`, `_EmissionColor`, `_MainTex_ST` and the like; design doc §10 "Shaders") to `format::material` fields, take defaults from the material, and decide per-renderer versus shared material instances (Unity animates a renderer's material instance, so each island needs its own records: read ADRs 0007 and 0023 first; `civbb-render` keeps one material buffer). Feed the animated values from `civbb-client`'s islands into the viewer's material records each frame. Record decisions the design doc does not make in an ADR. Then close M0.7 and start M0.8 with #28 if time allows. Keep `cargo xtask ci` green (CI runners have a Vulkan loader but no driver: device tests must skip), commit and push per issue, check that CI passes after each push, and when you stop, update STATUS.md, LESSONS.md and this NEXT_PROMPT.md and push.
```

## For the human, optional

- More Booth avatars: drop the zips in `corpus/local/` and ask the session to convert them (`corpus/README.md`).
- Local fuzzing is optional (CI runs it): `rustup toolchain install nightly`, `cargo install cargo-fuzz`, see `fuzz/README.md`.
