# civbb-ui

Retained-mode UI on our renderer (design doc §4; ADRs 0116, 0117). Engine crate: depends only on `civbb-format` (and
cosmic-text).

- `draw`: `DrawList`, a triangle list in window pixels (origin top left) with plain colour, glyph atlas or avatar
  texture per vertex; the renderer's overlay pass draws it.
- `text`: labels shaped, wrapped and rasterised by cosmic-text from the system's fonts into one RGBA8 glyph atlas.
- `radial`: the radial expression menu: pages of a format `Menu` around a ring, toggles, buttons, submenus and two-,
  four-axis and radial puppets driven by a pointer, writing `(entry, value)` pairs the client applies to the
  animator.

World-space canvases, taffy layout and SDF text come with VR (M1.4) and uGUI mapping.
