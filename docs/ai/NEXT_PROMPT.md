# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md, docs/ai/CODEMAP.md and docs/ai/LESSONS.md, then start M0.7 Renderer and viewer with #24: create `civbb-render` (engine layer, already listed in xtask; description, README) with an `ash` + `winit` Vulkan 1.3 bootstrap: instance, device selection, swapchain with resize, validation layers in debug builds, and a render graph skeleton (passes declaring their attachments) that clears and presents. Check first whether the Vulkan SDK (for validation layers) is installed; the issue says its install is approved, so install it yourself if a silent installer is available, otherwise open a `needs-user` issue and keep the validation path optional. Record decisions the design doc (§6) does not make (shader language and compiler, render graph shape, frames in flight) in an ADR. Keep `render` free of `client` types; the window loop that drives it belongs to `civbb-client` (#27). Then #25 (compute skinning and sparse blendshapes) if time allows. Keep `cargo xtask ci` green (CI runners have no GPU: tests that need a device must skip cleanly without one), commit and push per issue, check the fuzz workflow after pushes that touch parsers, and when you stop, update STATUS.md, LESSONS.md and this NEXT_PROMPT.md and push.
```

## For the human, optional

- More Booth avatars: drop the zips in `corpus/local/` and ask the session to convert them (`corpus/README.md`).
- Local fuzzing is optional (CI runs it): `rustup toolchain install nightly`, `cargo install cargo-fuzz`, see `fuzz/README.md`.
