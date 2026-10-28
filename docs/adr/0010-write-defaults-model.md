# 0010: Write Defaults model and golden-file conformance cases

Status: accepted
Date: 2026-10-28

## Context

Design doc §7 requires Write Defaults (WD) on, off and mixed reproduced exactly (#16). Unity does not document
the mechanism. The VRChat community has measured its effects (VRC School "Write Defaults", VRCFury "Write
Default Bugs"): WD-on empty states revert properties; WD-off ones keep them; one WD-off state makes a whole
controller (all playable layers together) behave as WD off; a WD-on blend tree at partial weight overrides upper
layers; WD-off direct trees converge to `x / (1 - S)`. There is no Unity install to record against yet (ADR 0002).

## Decision

One model that reproduces every measured effect above:

1. Each frame starts from the defaults when every state is WD on, else from the previous frame's pose.
2. A layer writes the slots animated by any of its sources with weight above 0 (ADR 0008). For such a slot, a
   source (clip leaf, empty state or interruption snapshot) not animating it contributes the default if its
   state is WD on, or the layer's incoming value if WD off. Weight short of 1 goes to the default (ADR 0009):
   `v = d + Σ w_a (v_a - d) + W_off,not animating · (in - d)`.
3. Additive layers ignore WD: sources not animating a slot add nothing.
4. Golden files (`crates/anim/tests/golden/*.json`) are small controllers with per-frame parameter changes and
   expected values, each citing its source. Hand-derived now; Unity recordings in the same shape replace them
   (M0.10), and any case the model then fails supersedes this ADR.

## Consequences

- Not modelled yet: WD-off additive drift, muscle and transform "ownership" across playable layers (VRCFury
  bugs 3 to 5; need #48 and per-group streams), mask quirks on material slots, WD-on blendshape tripling.
- Rule 1 makes WD-off avatars cost a pose copy less but keep state between frames, so a remote avatar's pose
  depends on its history, as in Unity.
