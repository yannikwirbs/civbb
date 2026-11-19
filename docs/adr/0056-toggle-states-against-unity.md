# 0056: Toggle states against Unity: muscles ignore Write Defaults off, exit times recur, the reference runs state behaviours

Status: accepted
Date: 2026-11-20

## Context

After ADR 0055 the rest poses matched Unity, but toggled parity states did not: Yumeka stayed standing after
`VelocityX=1` where Unity fell to the muscle-zero pose, HAOLAN stayed undressed after `Reset=1`, Cokoro jumped from
0.37 to 0.79. The reference run now logs every state's layers (`CIVBB <state> layer`, with each playable layer's
mixer weight and each clip's blend weight), and small probes in the SDK host project settled each rule:

1. **Muscles and Write Defaults off.** A WD-off state blending a muscle clip with an empty clip at 0.5 gave half the
   clip's value, whatever the previous state wrote; a WD-off state animating no muscles gave 0. A generic property
   in the same setup held the incoming value (ADR 0010 stands for those). Yumeka's locomotion is WD off and its
   moving states blend its standing clip with the SDK's (empty) proxy clips; ours held the standing pose.
2. **Exit times.** With the condition turning true after the first crossing, exit time 0.5 fired at the next 0.5
   and exit time 0 at the next 1.0 for a looping clip, a non-looping clip and a state without motion alike. ADR
   0008's rule re-checked only looping motions, so Cokoro's reset (an empty state, exit time 0) never fired.
3. **The reference's state behaviours.** The SDK's parameter driver, playable layer control and animator layer
   control do nothing without the client: they call `VRC_AvatarParameterDriver.OnApplySettings` and the instance
   `ApplySettings` that the static `Initialize` (called from each behaviour's Awake) is expected to set. Our runtime
   runs them (ADR 0011), so the reference undressed HAOLAN where VRChat redresses it and kept Cokoro's AFK action at
   weight 0.

## Decision

1. Write Defaults off never holds humanoid muscles: a WD-off controller starts every frame with its muscle slots at
   their default (0, ADR 0055); other slots keep the last pose.
2. An exit time below 1 is reached once per cycle whether or not the motion loops; at 1 or more, once.
3. `Reference.Render` sets those hooks by reflection before creating the controllers: drivers apply Set, Add,
   Random (Unity's generator) and Copy (unclamped range conversion, as ours) to every playable layer's controller;
   playable layer control blends a group's mixer input and animator layer control one controller layer linearly to
   the goal over the blend duration. The final log line names each hook.

## Consequences

- Worst toggled state against Unity: Cokoro 0.71 to 0.37, Cokoro Quest 0.74 to 0.35, HAOLAN 0.54 to 0.23, HAOLAN
  Quest 0.56 to 0.18, Yumeka 0.61 to 0.58 (the rest of Yumeka is shading, #87). Blackbird's `Texture=1` and Inuinu's
  `IROKAE=1` (0.80, 0.82) are material-slot swaps (#88).
- Muscle WD-off across layers (a lower layer writing a muscle under a WD-off upper layer that blends it with an
  empty clip) is not probed; the upper layer still holds the lower layer's value there.
- Humanoid mirroring (`m_Mirror` on blend tree children and states: HAOLAN's and Cokoro's locomotion trees,
  KohzieBoi's action layer) is not in the format; it showed in no rendered state here.
