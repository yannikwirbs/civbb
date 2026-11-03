# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md, docs/ai/CODEMAP.md and docs/ai/LESSONS.md, then continue M0.7 Renderer and viewer with #26 in `civbb-render`: toon and PBR uber-shaders (Slang, compiled by `cargo xtask shaders`; read ADRs 0021 and 0022 first), bindless textures through descriptor indexing, all meshes in shared vertex and index buffers, one material storage buffer, and indirect multi-draws grouped by pipeline (design doc §6 "GPU-driven path" and "Materials and shaders"). Add depth attachments and graph-owned transient images to the render graph as the passes need them, and make buffers graph resources where a second consumer needs ordering (ADR 0022 point 6). Draw the skinned output of `skin::Skinning`. Record decisions the design doc does not make in an ADR. Then #27 (desktop viewer in `civbb-client`: winit loop driving `Renderer`, loads a package or glb, orbit camera) if time allows. Keep `cargo xtask ci` green (CI runners have a Vulkan loader but no driver: device tests must skip), commit and push per issue, check that CI passes after each push, and when you stop, update STATUS.md, LESSONS.md and this NEXT_PROMPT.md and push.
```

## For the human, optional

- More Booth avatars: drop the zips in `corpus/local/` and ask the session to convert them (`corpus/README.md`).
- Local fuzzing is optional (CI runs it): `rustup toolchain install nightly`, `cargo install cargo-fuzz`, see `fuzz/README.md`.
