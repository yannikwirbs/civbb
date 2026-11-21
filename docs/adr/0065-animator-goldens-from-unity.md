# 0065: Animator golden cases recorded in Unity

Status: accepted
Date: 2026-11-21

Supersedes ADR 0010's "WD-on empty states revert" and hand-derived goldens, and ADR 0028 point 4's `Start` state.

## Context

The animator golden cases (`crates/anim/tests/golden/*.json`) were hand-derived from community write-ups (ADR 0010).
`tools/unity-truth/Assets/Editor/Goldens.cs` now builds each case's controller with the AnimatorController API, plays
its frames through a `PlayableGraph` (parameters set, then one evaluation of `dt`) and records every expected value
and current state into `tests/unity/goldens.json`. Unity 6000.3 and 2022.3.22 (the SDK host project) record the
same values. Against the hand-derived cases and new ones (#54):

- A Write Defaults on state **without a motion writes nothing**: returning to it leaves a toggled hat on. With an
  empty clip as its motion it resets the hat. The community rule "WD-on empty states revert" holds only for empty
  clips.
- The **first update** enters the start state and advances it by `dt` (a ramp reads 0.2 after the first 0.2 s);
  ours entered without advancing.
- A transition taken at its **exit time starts there**: the rest of the frame counts towards the fade and the
  destination (exit time 0.75, fade 0.25 s: at 0.8 s the fade is 0.2 through). Ours started it at 0.
- **Exit time 0 is not reached at the state's start**; it fires after a full cycle (at 1.0 s). Our first check
  counted the start time.
- The other six cases (Write Defaults on, off and mixed; direct tree feedback; partial blend override) and the new 1D
  blend tree case matched the hand-derived values.

## Decision

1. Golden expectations are Unity's: `golden_cases` checks every case against `goldens.json` before running it, so a
   case without a recording or with a hand-edited value fails. New cases are recorded by running `Goldens.Record`.
2. `writes_defaults(state)` is Write Defaults on and a motion; a motionless state holds like Write Defaults off
   (also for "one WD-off state makes the controller WD off").
3. The first update advances and checks transitions like any other; exit times count from after the start time; a
   transition fired at its exit time starts with the overshoot already elapsed.
4. The collapsed toggle layer (ADR 0028) is one `Toggles` state: each toggle takes its transition on its first
   update, so the `Start` state that played the default clips for one frame goes.

## Consequences

Avatars relying on WD-on empty states to reset toggles now keep them, as in VRChat. Still to record (#54): material
property curves (defaults across differing materials, animated HDR colours, `_ST` offsets, Write Defaults on
material properties), a masked playable layer, interruption sources.
