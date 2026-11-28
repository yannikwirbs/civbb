# 0116: Text through cosmic-text with system fonts, rasterised into one glyph atlas on the CPU

Status: accepted
Date: 2026-11-28

## Context

#149's radial menu shows labels that legacy avatars write in any script (Booth avatars mostly in Japanese). Design
doc §4 fixes retained-mode UI on our renderer and `taffy` for layout, but no text stack, font or rasteriser; §10 maps
TextMeshPro onto "SDF font atlases converted" later. The repo holds only CC0 or generated assets, so no font can be
committed without a licence call.

## Decision

1. **cosmic-text 0.19** (MIT OR Apache-2.0; the text stack of COSMIC, iced and glyphon) shapes (harfrust, a
   HarfBuzz port), lays out, wraps, falls back across fonts per script and rasterises (swash) on the CPU. One crate
   covers what a label in any script needs; the alternatives (fontdue, ab_glyph) neither shape nor fall back.
2. **Fonts are the system's** (`FontSystem::new()`: fontdb scans the platform's font directories, pure Rust, no
   fontconfig library). Windows has Segoe UI and Yu Gothic or Meiryo for Japanese. A machine without fonts shows no
   text; menus still work. A bundled font (OFL or CC0) is a licence call for later, with the settings UI.
3. **One glyph atlas** (`civbb_ui::text::Atlas`): RGBA8, 1024 x 1024, linear; coverage glyphs are white with the
   coverage in alpha, colour glyphs (emoji) keep their colours. Glyphs are packed on shelves the first time they are
   drawn; a full atlas is cleared at the start of the next frame and refilled (glyphs that did not fit are skipped
   for one frame). Each change bumps `Atlas::version`; the renderer replaces its copy when the version differs.
4. Glyphs are drawn at whole pixels, the size cosmic-text rasterised them at: screen-space text has no scaling, so
   no SDF is needed now. SDF atlases come with world-space canvases (VR, M1.4) and TextMeshPro mapping.

## Consequences

- Text costs megabytes of font scanning once (`Text::new`, measured in STATE.md); the client makes it on first use.
- Tests cannot rely on fonts (Linux runners may have none): menu logic is tested without text, the atlas with
  whatever fonts there are.
- The atlas is shared by every label in a frame; menus use far fewer glyphs than it holds.
