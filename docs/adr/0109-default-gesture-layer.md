# 0109: A default Gesture layer per hand

Status: accepted
Date: 2026-11-27

## Context

ADR 0108 added default Base and Action layers but left an empty Gesture group without hand poses (#152). Cokoro,
Inuinu, KohzieBoi and Taro export one (they use the SDK's default hands layer), so the desktop player's gestures
(ADR 0107) moved no fingers on them.

## Decision

1. `civbb_anim::defaults::apply` also adds two layers to a Gesture group without layers, left hand then right, after
   the Base and Additive layers. Each is masked to its hand's fingers and has one state per gesture value 0 to 7
   (relaxed, fist, open, point, peace, rock, gun, thumbs up) playing the generated hand clips, entered by an any-state
   transition on `GestureLeft`/`GestureRight` equal to the value (0.1 s).
2. The fist state is a 1D blend from the relaxed hand to the fist on `GestureLeftWeight`/`GestureRightWeight`, as
   analog triggers close it; the desktop player sets 1 (ADR 0107).

This supersedes ADR 0108's consequence that an empty Gesture group shows no hand gestures.

## Consequences

Every humanoid with finger bones poses its hands on gestures. Emotes stay #151.
