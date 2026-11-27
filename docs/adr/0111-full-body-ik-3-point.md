# 0111: Full-body IK: avatar-space bone targets, hips follow the head, limbs keep their animated bend

Status: accepted
Date: 2026-11-27

## Context

Design doc §7 asks for one full-body IK solver for 3-point, 6-point and 10+ tracker setups: analytic two-bone IK for
limbs, an iterative solver for spine and hips. #145 builds it for 3-point (head and hands) and the desktop view
pitch. The doc leaves open what a target is, how the head target splits between spine and hips, what the legs do
when the hips move, how a limb picks its bend direction, how tracking control (ADR 0011's pose sources) gates the
solver, and how desktop mode drives the head.

## Decision

1. **Targets are bone placements in avatar space** (the island origin's frame): `ik::Targets` per island, set before
   the step: for the head and each hand bone, optionally a `Goal` (position and rotation). Device-to-bone offsets
   (calibration) are the caller's job (OpenXR, M1.4), so remote clients solve from the same networked bone goals.
2. **Order**: the solver runs after constraints and before dynamics (ADR 0020), on the animated pose, and rotates bones
   only, apart from the hips. With a head goal: the hips move by the head's height change; the mapped spine bones
   (spine, chest, upper chest, neck) bend the head towards the goal over 2 passes, each bone from the root up taking
   `1 / (bones left)` of the remaining turn; the hips take whatever offset is left (so the head always arrives); the
   head takes the goal rotation. Crouching therefore lowers the hips and leaning bends the spine.
3. **Legs keep their animated feet** when the hips move: two-bone IK of each leg to its foot's placement before the
   solve, then the foot's rotation. 3-point has no foot targets; 6-point replaces these with tracker goals.
4. **Two-bone limbs** bend towards where the middle joint was (its offset from the root-to-end line, projected off the
   new line), or, when the limb is straight (T-pose), towards a rest pole fixed in the limb parent's frame: elbows
   down and back, knees forward. An unreachable target stretches the limb straight at it; a target closer than
   `|upper - lower|` folds it as far as it goes. The end bone takes the goal rotation; without a goal it keeps its
   local rotation.
5. **Tracking control**: a part whose pose source is `Animation` (the animator's pose source actions) ignores its
   goal: `Head` gates the head goal and the view pitch, `LeftHand` and `RightHand` the hands. Other parts have no
   3-point goals yet.
6. **Desktop look**: `Targets::look` (the view pitch) turns the animated head about the avatar's side axis, shared as
   0.2 to the upper chest (or chest), 0.3 to the neck and the rest to the head; missing bones leave their share to
   the head. It turns the animated pose rather than setting an absolute head rotation, so prone, sitting and
   emote poses keep their head pose plus the pitch.

## Consequences

- No targets and no pitch leave the pose bit for bit as animated: parity and existing goldens are untouched.
- The solver walks parents to place each bone it touches (no world pose cache): cheap for the dozen bones of 3-point,
  allocation-free, and never stale. 6-point and 10+ (hip and foot goals, chest and knee trackers) extend `Targets`;
  the hips-follow-head rule then yields to a hip tracker.
- Hips that rise above the legs' reach float the feet off their animated placement (the legs stretch straight);
  calibration that scales the avatar to the user's height keeps this rare.
- Twist is left to the animation: two-bone swings never add roll, and twist bones stay as constraints place them.
