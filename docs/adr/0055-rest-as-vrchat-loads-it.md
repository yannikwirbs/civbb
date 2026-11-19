# 0055: Rest as VRChat loads an avatar: zero muscles, standing built-ins, settled states

Status: accepted
Date: 2026-11-19

## Context

After #82 most of the FLIP error against the Unity reference renders (ADR 0048) was pose at rest (#86). The
reference run now logs each playable layer's current clips and the human pose's muscles at rest
(`Reference.LogRest`). Three causes:

1. **Muscle defaults.** A humanoid whose active states animate no muscles reads every muscle as exactly 0 in Unity
   (HAOLAN, KohzieBoi: arms down, hands forward). Our runtime defaulted a muscle to the bind pose's value
   (`Rig::rest`), so those avatars stood in T-pose.
2. **Built-ins.** Both sides started every parameter at the animator's default, and our runtime set only `IsLocal`.
   Creators leave `Grounded` at 0, so VRChat's locomotion layer leaves Standing for SmallHop at once (Cokoro: Unity
   rendered a third of the way into that transition, every muscle at two thirds). In VRChat the engine sets
   built-ins every frame; a standing avatar is grounded.
3. **Transients.** States rendered after three 60 Hz frames (ADR 0033). The Idle clip swings Tessakuchan's arms from
   the bind pose on the first frame, and the SDK's version 1.0 PhysBones on her lower arms still lag 100 degrees after
   three frames (the "missing blades"); our solver does not model those transients (#83).

## Decision

1. A humanoid muscle's default value is 0 (Unity's default human pose), for muscles no clip binds and for bound
   muscles a state does not animate. `Rig::rest` stays as the bind pose in muscle space (tested against Unity).
2. `Instance::new` starts built-ins at `Builtin::at_rest`: an avatar standing still on the ground at its own scale
   (`Grounded`, `Upright`, `IsAnimatorEnabled`, `ScaleFactor`, `ScaleFactorInverse` 1; `AvatarVersion` 3). Others keep
   the animator default until the engine sets them. The parity hand-off's parameters and toggled states come from a
   fresh local instance, so the Unity reference starts where our runtime starts (and will follow #85's expression
   defaults the same way).
3. `parity::FRAMES_PER_STATE` is 60 (one second) instead of three, for both the stage comparisons and the Unity
   hand-off: a state renders settled, not mid-transition. This supersedes the frame count of ADR 0033.

## Consequences

- Rest means against Unity (front view): HAOLAN PC 0.47 to 0.17, HAOLAN Quest 0.46 to 0.09, KohzieBoi 0.28 to 0.14,
  Cokoro Quest 0.45 to 0.22, Cokoro 0.37 to 0.29, Tessakuchan 0.31 to 0.21, Inuinu 0.09 to 0.06 (table on #86).
- Parity costs 20 times the simulation frames; rendering is unchanged. Unity reference runs take seconds longer.
- Transient behaviour (PhysBone startup, transition timing) is no longer in the parity scores; #83 and #54 cover it.
- Blackbird's wings are material-slot swaps (`m_Materials.Array.data[n]` object-reference curves), not a pose, and its
  `WingsClosed` expression default (1) differs from the animator's (0): #85 and a new issue.
