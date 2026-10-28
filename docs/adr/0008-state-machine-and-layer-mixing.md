# 0008: Mecanim state machine timing, interruption and layer mixing

Status: accepted
Date: 2026-10-28

## Context

Design doc §7 asks for a Mecanim-compatible state machine (#14). Unity's manual (Transitions page,
`AnimatorStateTransition.exitTime`) and the "State Machine Transition interruptions" blog post define most of it;
the rest is unspecified and there is no Unity install to record against yet (ADR 0002).

## Decision

Per frame and layer, in layer order: advance, complete a finished cross-fade, take at most one transition,
evaluate the pose, blend it over the layers below. The first frame only enters the start state.

From Unity's documentation:

- Entry: the first entry transition whose conditions hold, else the default state. Exit re-runs entry.
- Transitions with neither conditions nor exit time are ignored. Any-state transitions are checked before the
  current state's. Triggers stay set until a transition (or entry transition) using them fires.
- Exit time is true on the first frame that reaches it, never on the state's first evaluation, every cycle when
  below 1 in a looping state, once otherwise.
- During a cross-fade the queue is the any-state transitions, then the interruption source's lists. Ordered
  interruption stops at the running transition; only an any-state transition may interrupt itself. An
  interruption blends from a static snapshot of the layer's pose.
- Fixed durations are seconds, normalised ones a fraction of the source state's length; the offset is the
  destination's start time.

Our choices, unverified against Unity:

- Empty states and zero-length clips are 1 s long. Normalised durations ignore state speed.
- An any-state transition with "can transition to self" off is skipped when its destination is the state the
  layer is in or heading to. After an interruption the "current state" is the firing transition's owner.
- Mixing: the pose starts from defaults. A layer writes exactly the slots animated by a source with weight
  above 0; for a slot, sources not animating it contribute the default (Write Defaults on; #16 adds off and
  mixed). Quaternions are summed in the fill's hemisphere and normalised; layers blend by nlerp.
- Additive layers add each clip's difference from its first frame (rotations: the local delta `q0⁻¹ q`).
- Masks filter by the node path of a slot's target (blendshapes and renderer flags by the mesh's node);
  parameter slots are never masked; body parts wait for humanoid curves (#48).
- The first layer of each group has weight 1; group weights multiply layer weights. Animated parameters take
  the value of their slot at the end of the frame, only when some layer wrote it.

## Consequences

- Behaviour conformance tests (`crates/anim/tests/machine.rs`) encode all of the above; when Unity recordings
  exist (M0.10), each unverified choice gets checked and this ADR superseded if one differs.
- One transition per layer per frame: chains of zero-duration transitions take one frame each.
