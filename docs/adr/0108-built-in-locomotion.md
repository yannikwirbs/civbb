# 0108: Built-in locomotion: generated muscle poses merged into avatars at load

Status: accepted
Date: 2026-11-27

## Context

Avatars on the SDK's default playable layers export those groups empty, and SDK proxy clips as empty clips that keep
their name (ADR 0042). Of the 13 corpus exports, KohzieBoi and Taro have no Base layer; Cokoro, HAOLAN, Inuinu and
Yumeka bring their own Base layer whose states play proxy clips (`proxy_walk_forward`, `proxy_crouch_still`, ...);
Blackbird, HAOLAN and Tessakuchan's Gesture layers play `proxy_hands_*`. Without substitutes the player slides around
in the rest pose (#143). CLAUDE.md rules out VRChat's animations. Options for our own: CC0 mocap converted to muscle
clips (a download, retargeting and a licence trail per clip, megabytes) or poses generated in code.

Corpus clips fix the muscle conventions the generator needs: standing straight is upper leg front-back about 0.6 and
lower leg stretch 1 (more negative front-back swings the thigh forward), arms hang at arm down-up about -0.4, finger
stretch +1 is open and -1 curled; the body position (`RootT`) stands near 0.97 and sits on the floor at 0.23.

## Decision

1. **Generated, not captured.** `civbb_anim::defaults` builds every clip from a few key poses in muscle space (a
   procedural gait: thigh, knee, foot and arm swings phased per leg, sampled 16 keys per cycle with Catmull-Rom
   tangents). No captured or third-party data; the curves are ours.
2. **Grounded per avatar.** Each key's body height is solved on the avatar's own skeleton: the pose's lowest human
   bone (with the body rotation applied) sits where the rest pose's lowest one does. One set of muscle curves fits
   every humanoid; only the body curves differ.
3. **Merged at load, not shipped as a package.** `defaults::apply(&mut Document)` runs on a loaded avatar before it
   compiles: it fills empty `proxy_*` clips by name (trailing digits and spaces ignored), adds a Base layer when the
   Base group has none (standing, crouching and prone 2D blend trees on `VelocityX`/`VelocityZ`, a falling state on
   `Grounded`, postures from `Upright` at 0.7 and 0.4) and an Action layer when the Action group has none (AFK and
   sitting, raising the Action group's weight while they play). It adds the platform entries those layers read.
   A package of layers without an avatar has no format and no binding step yet; generation costs well under a
   millisecond per avatar. Packages stay as built, so parity, `civbb build` reports and hashes are unchanged.
4. **Covered proxies**: standing, walking, running, sprinting and strafing in every direction the SDK names,
   crouching and crawling, falling and landing, AFK, sitting, and the eight hand gestures. Emotes (`proxy_dance`,
   `proxy_backflip`, `proxy_stand_wave`, seated emotes, ...) stay empty until #151.
5. Only the desktop client applies them (viewer and player); parity and the benchmark show packages as built.

## Consequences

- Avatars without a Base layer idle, walk, run, crouch, crawl and fall; creator locomotion that plays proxy clips
  animates; hand gestures that play `proxy_hands_*` pose the fingers.
- The poses look generic and are not VRChat's: #109's parity of default-layer states stays out of reach by design.
- An empty Gesture group still shows no hand gestures (#152).
- Network peers must apply the same defaults to show the same pose; the merge is deterministic in the document, so
  every client that applies it agrees.
