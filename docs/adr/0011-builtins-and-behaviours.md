# 0011: Built-in parameters and state behaviours

Status: accepted
Date: 2026-10-28

## Context

Design doc §7: built-in parameters keep their legacy names and meanings, and state behaviours (parameter
drivers, tracking, layer and locomotion control, audio) are mapped (#17). The format already has the behaviour
types; the runtime needs their timing and conversions.

## Decision

- `Builtin` lists the 30 legacy built-ins with their names and engine types. The engine sets them each frame
  through `Instance::set_builtin`; one the animator does not declare is ignored, one declared with another type
  is converted to the declared type. `IsLocal` comes from the instance's owner flag.
- Conversion to a parameter's type: ints round, bools and triggers become 1 when not 0.
- Behaviours run once when their state is entered: at layer start and when a transition into it fires, right
  after it fires, so later layers see driven values in the same frame.
- Parameter drivers apply their operations in order; local-only drivers run only on the owner's instance (others
  receive synced values). Random: floats uniform in `[min, max)`, ints uniform over `min..=max`, bools and
  triggers true with `chance`, from a per-instance deterministic generator. Copy with a range maps linearly,
  unclamped.
- Layer and group weight control blend linearly from the current weight to the goal over the duration; a new
  blend on the same target replaces the old one.
- Tracking and locomotion control set instance state for the IK and locomotion systems to read.
- Audio playback needs a format type and the audio crate: #51.

## Consequences

- Unverified until Unity/VRChat comparisons (M0.10): rounding versus truncation for ints, bool conversion
  threshold, whether copy ranges clamp, and same-frame visibility of driven values.
- Built-in values themselves (gestures, velocity, visemes) come from the client's input, locomotion and lip sync.
