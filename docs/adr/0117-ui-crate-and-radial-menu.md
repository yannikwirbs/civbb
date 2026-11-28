# 0117: The `ui` crate: widgets emit a screen-space triangle list; the radial expression menu as its first widget

Status: accepted
Date: 2026-11-28

## Context

#149 asks for the radial expression menu in desktop mode (design doc §7 "Expressions and interaction") drawn by our
renderer (§4: retained-mode UI on our renderer, `taffy` for layout, world-space canvases). Engine crates depend only
on `core` and `format`, so `ui` cannot know the renderer or the animator. The format carries the menu as
`civbb_format::expressions::Menu` (controls with a kind, a parameter entry, a value and axis entries). Nothing fixes
the menu's look, its geometry or how puppets map the mouse.

## Decision

1. **`civbb-ui` (engine layer)** keeps widget state and turns it into a draw list; it never touches the GPU or
   the animator. Modules: `draw` (`DrawList`: a triangle list of `Vertex { pos, uv, color, texture }` in window
   pixels, origin top left; `Texture::None | Atlas | Icon(texture index of the avatar)`), `text` (ADR 0116), `radial`
   (the menu).
2. **The renderer draws the list** in an overlay pass after the forward pass (`civbb_render::overlay`): one pipeline,
   vertices pulled by address, no depth, straight alpha blending, textures from the bindless array. The client maps
   `Texture` to bindless slots (the glyph atlas, the shown package's textures) when the snapshot is drawn.
3. **The menu writes parameters, not the animator.** `Radial::update(menu, params, pointer, screen, &mut writes)`
   reads current values and pushes `(entry, value)` writes; the client applies them to the local player's animator,
   coerced to the entry's type, before the islands step.
4. **Geometry and input** (our own design): a ring centred on the window, outer radius 0.3 of its smaller side, the
   centre disc 0.3 of that. Controls are equal sectors clockwise from the top; the pointer selects by angle outside
   the centre disc, at any distance. Left press acts on the hovered sector; the centre, a right press or Escape goes
   back (the root closes). Tab or the pad's Start opens and closes it; while open the cursor is free and look mode
   off, and the movement keys still work.
5. **Controls**: a button writes its value while held and 0 on release; a toggle writes 0 when its entry holds its
   value, else the value; a submenu opens its page and holds its parameter at its value while open; puppets take
   the pointer until the next press or back, holding their parameter meanwhile. Two-axis: pointer offset over the
   outer radius, clamped to the unit disc, into axes (horizontal, vertical, up positive). Four-axis: the same offset's
   positive parts into (up, right, down, left). Radial: the angle clockwise from the top as 0 to 1. Leaving a puppet
   writes 0 to the held parameter and to two- and four-axis axes; the radial value stays. Closing the menu leaves
   every open page and puppet the same way.
6. **taffy waits** for the first box layout (world-space canvases, uGUI mapping, settings): the ring is polar and
   needs none.

## Consequences

- The menu is testable without a window or GPU: tests drive `Radial` with pointer positions and read the writes.
- World-space placement for VR (M1.4) reuses `DrawList` with a different projection; ADR 0116's atlas moves to SDF
  then.
- Synced parameter traffic and saved toggles (`saved` entries) persist nowhere yet; persistence comes with the
  account backend.
