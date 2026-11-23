# 0066: Transition interruption as recorded in Unity

Status: accepted
Date: 2026-11-23

## Context

ADR 0008 chose, unverified, that only an any-state transition may interrupt itself and that an interruption blends
from a static snapshot of the layer's pose. #54 records the golden cases in Unity (`Goldens.cs`, ADR 0065);
`interruption_source.json` and `interruption_self_restart.json` cover interruption sources and ordering.

## Decision

Supersedes the interruption points of ADR 0008:

- A state's transition with unordered interruption interrupts itself whenever it is valid in its own queue, so a
  transition whose condition keeps holding restarts every frame and its fade never progresses. Ordered interruption
  still stops at the running transition; any-state transitions keep ADR 0008's rule.
- The snapshot an interruption blends from is the pose last output, before this frame's advance, not the pose after it.
- Unchanged and recorded: ordered interruption lets only transitions listed before the running one interrupt; source
  `None` lets nothing but any-state transitions in.

## Consequences

One extra layer evaluation per frame while a cross-fade runs (the snapshot is taken before advancing). Avatars with
an unordered `Source` transition on a held Bool now freeze mid-fade, as in Unity.
