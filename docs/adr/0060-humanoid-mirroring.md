# 0060: Humanoid mirroring

Status: accepted
Date: 2026-11-20

## Context

Unity plays a humanoid motion mirrored left to right when its state has Mirror set (or a bool mirror parameter on)
or a blend tree child has Mirror set (#89). HAOLAN (PC and Quest), Cokoro and KohzieBoi use it in locomotion, action
and gesture controllers. The format had no flag and the exporter did not read it. ADR 0056 showed that hand-derived
animator rules go wrong; this one was recorded first.

`tools/unity-truth/Assets/Editor/Mirror.cs` plays one clip with a distinct value per muscle on the generated test
skeleton (every human bone) plainly, in a mirrored state, as a mirrored blend tree child, and both, and writes
`crates/anim/tests/unity/mirror.json` (Unity 6000.3; the same mapping showed on HAOLAN and Yumeka in the SDK host
project, 2022.3).

## Decision

1. The mirror is a map of muscle values, exact to the recording: limb and finger muscles swap sides unchanged; the
   spine, chest, upper chest, neck, head and jaw negate their left-right and twist muscles; each eye keeps its side
   and negates in-out (`muscle::mirror`). Other curves (transforms, blendshapes, materials) are not mirrored.
2. A child's mirror and its state's (or parents') combine by exclusive or: a mirrored child in a mirrored state plays
   plain.
3. Format v21: `State::mirror`, `State::mirror_param` (a bool parameter replacing the flag) and `ChildMotion::mirror`,
   defaulting to off. The exporter writes them.
4. When any state or child mirrors, the binder also gives each bound muscle's counterpart a slot; the controller keeps
   per slot the slot and sign a mirrored leaf writes to, and the mixer applies it, so masks and Write Defaults act on
   the side actually written. Toggle collapsing (ADR 0028) leaves layers with a mirrored state alone.
5. Not mirrored yet: the body pose (root motion, #52) and IK goals.

## Consequences

- The muscle map is checked against the recording (`anim/tests/mirror.rs`), and the animator against plain,
  mirrored state, mirror parameter, mirrored child and both.
- No rendered parity state gave a mirrored motion weight when #89 was opened; rest and toggled states are unchanged
  where no mirrored motion plays.
