# 0028: Toggle layers collapse into one direct blend tree layer per group

Status: accepted
Date: 2026-11-03

## Context

#30: design doc §7 and §9 stage 5 collapse toggle layers into a single direct blend tree, behaviour-identical. The
doc does not say which layers count as toggles, how the collapsed layer keeps our animator model's results (ADRs
0008 to 0011: transition timing, Write Defaults, mid-update parameter drivers, the first frame), or what the
optimisation buys in our runtime, where a layer costs far less than in Unity.

## Decision

1. **`civbb_pipeline::collapse` runs in `civbb build` after `merge`** and reports the layers it removed.
2. **A toggle layer** is an override layer at weight 1 without a mask, with exactly two states, each playing one
   clip and with one transition to the other: no exit time, zero duration, one condition on the same bool (`If`
   leaving the off state, `IfNot` leaving the on state); no entry or any-state transitions and no behaviours. Every
   curve of both clips is constant at all times: one key, or equal keys whose segments are stepped. Hermite
   segments between equal keys round differently at different times, and the collapsed layer samples clips at
   other times than the toggle states did.
3. **It collapses** only if no other layer's clips write any of its slots (summing is then the same as layering),
   no clip animates its bool (a clip can set a fraction the blend tree would blend), and no behaviour sets its
   layer's weight. Toggles collapse per layer group, into one layer in the first toggle's place; a later toggle
   joins only if no layer between the two drives its bool, since a driver takes effect mid-update for the layers
   after it. Groups with one toggle stay as they are.
4. **The collapsed layer** has a `Start` state playing each toggle's default-state clip, because each toggle layer
   spent its first frame in its default state whatever its bool, and a `Toggles` state entered on the second frame
   (exit time 0, zero duration) playing a direct tree with one child per toggle: a 1D tree on the bool with the off
   clip at 0 and the on clip at 1. Every direct child weighs 1 through a new float parameter `civbb/One` (default 1,
   not synced). Both states have Write Defaults on: with weight 1 per toggle, each slot then gets exactly its clip's
   value, `d + 1 · (v - d)`, as the toggle layer computed it (ADR 0010); Write Defaults off would add the other
   toggles' held weight. If the toggles held the controller's only Write Defaults off states, the layer gets an
   unreachable `Hold` state with it off, so frames still start from the last pose.
5. **Layer weight behaviours** of the remaining layers are remapped to the new layer indices.
6. **Blend trees evaluate without allocating** for direct and 1D trees (`civbb-anim` `motion::each_weight`); the 1D
   picker is checked bit for bit against sorting the thresholds. Before this, the collapsed layer was four times
   slower than the 48 layers it replaced.
7. **Proof:** tests step islands before and after through 240 frames of random toggling and frame times, with
   bools set before the first frame and a driver layer, and compare every animator slot, node activity, local
   transform and parameter bit for bit; mutating the start state or the driver rule makes them fail.

## Consequences

- The bloated corpus avatar goes from 48 layers to 1; an animator update takes 2.08 µs instead of 2.34 µs. The
  gain is small in our runtime because a layer is cheap here; the remaining cost is per clip. Real converted
  animators (M0.10) will show whether more toggle shapes (any-state toggles, float toggles, two-key clips with flat
  tangents) are worth admitting.
- The rules are proven against our animator model, not Unity's. When Unity recordings change the model (#54), the
  collapse tests run against the new model and any failure supersedes this ADR.
