# 0112: 6- and 10-point IK goals, and FABRIK for the spine

Status: accepted
Date: 2026-11-27

## Context

#146 extends the 3-point solver of ADR 0111 to 6-point (plus hips and feet) and 10+ point (plus chest, elbows and
knees) setups (design doc §7). With the hips pinned by a tracker, the spine must bring the head to its goal on its own.
ADR 0111's distributed CCD (each spine bone taking its share of the remaining turn) stalls there: a goal nearly in line
with a straight spine stayed 7 mm away after 16 passes, since CCD only aims the chain and barely folds it. In 3-point
this never showed, because the hips took whatever the spine left.

## Decision

1. **Goals**: `Targets` gains `hips` and `chest` (the upper chest bone, or the chest without one) as `Goal`s, `feet` as
   a pair of `Goal`s, and `elbows` and `knees` as pairs of points the middle joints bend towards. Hands become a pair
   too. Tracking control: `Hip` gates hips and chest, each foot its knee; each hand its elbow.
2. **Order**: look; hips placed at their goal; with a chest goal, the bones below it bend to bring it to its goal and it
   takes its rotation; the bones above it (all spine bones without a chest goal) bend the head to its goal; the head
   takes its rotation. ADR 0111's 3-point rule (hips follow the head's height and take the remainder) applies only
   when neither hips nor chest have a goal. Legs go to their foot goals, else to their animated feet when the hips
   moved; arms to their hand goals. A given elbow or knee point is the bend hint when off the root-to-goal line,
   before the animated middle joint and the rest pole.
3. **Spine solver: FABRIK** (supersedes ADR 0111 point 2's distributed CCD): joint positions in a fixed array (at most
   four spine bones and the end), up to 40 passes, stopping once the end is within 0.1 mm; a goal out of reach
   straightens the chain at it without passes. Then each bone, root first, swings onto its new direction (no added
   twist). From a straight rest spine FABRIK needs up to ~40 passes near full extension; each pass is position math
   only, so a solve stays 3.5 to 4.4 µs for 3, 6 and 10 points on the lean avatar (release, one thread).

## Consequences

- A head goal reachable from pinned hips is met within 1 mm; the scripted 6- and 10-point tests check every goal
  bone and the elbows and knees against the scripted pose.
- The spine's bend shape is FABRIK's (more bend near the head than CCD's even shares); calibration and real trackers
  (the rest of #146, OpenXR in M1.4) may want joint limits or weights, not yet needed.
- Calibration, the tracker input API and the viewer's scripted trackers are still open in #146.
