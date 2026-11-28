# 0121: The client's locomotion page in the radial menu

Status: accepted
Date: 2026-11-28

## Context

ADR 0119 point 6 promises a native radial menu page for floor poses and flight, since VR has no keys for them
(#160). `civbb_ui::radial` (ADR 0117) works on a format `Menu` whose controls write avatar entries by index. Floor
poses and flight are not avatar parameters: they live on `Player` (`floor_pose`, `flying`), which the player system
turns into the `civbb/pose` entry and the body's movement, and they are offered only when the avatar has
`civbb/pose` and the world allows flight.

## Decision

1. **The client appends a page to the avatar's root menu.** The menu system builds, once per package, pose
   availability and flight rule, a copy of the avatar's root with a last submenu "Locomotion": Stand (button,
   value 0), Sit (toggle, 1) and Lie down (toggle, 2) when the avatar has `civbb/pose`, Fly (toggle, 1) where
   `WorldRules::flight`. Without either the root is the avatar's own; an avatar without a menu gets a root holding
   only the page. The widget stays as it is, with its navigation (#158) and later the VR pointer (#170).
2. **Synthetic entries past the island's own.** The page's controls write entry `n` (floor pose) and `n + 1`
   (flight), where `n` is the island's parameter count. The menu reads a buffer of the island's parameters
   followed by these two values from `Player`, and routes writes at or past `n` to `Player::floor_pose` and
   `Player::flying` instead of the island; the player system then applies them as it does the keys' (moving ends
   a pose, a world without flight lands the player).
3. **The root's ninth sector.** Legacy roots hold up to 8 controls; the page makes a ninth. The ring takes any
   count, so nothing is moved or nested.

## Consequences

- No new widget, no change to `civbb_ui` or the format; the page is client code only.
- The page shifts the root's sector count where it appears, so content cannot place its own control in that spot;
  acceptable for a platform page that is always last.
- More client pages (emotes #151, world stations #156) extend the same builder with further synthetic entries; if
  the root grows crowded they move under one "Client" submenu, decided then.
