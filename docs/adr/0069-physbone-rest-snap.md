# 0069: A PhysBone bone near its rest direction snaps onto it

Status: accepted
Date: 2026-11-23

## Context

After ADR 0068, the SDK's per-bone state (read by reflection) showed the remaining single-step misses of #84 all at
frames where a bone ends within a tenth of a degree of its rest direction: the SDK puts it exactly on the rest
direction (within float noise), where our step leaves it 0.016 to 0.081 degrees off. Each miss is under 0.15 mm, but
with stiffness 0.8 and momentum they build to 2.7 mm (`stiff_v11_advanced`). Over all recorded steps the largest
snapped angle is 0.08104 degrees (`1 - cos` = 1.0003e-6) and the smallest unsnapped 0.08422 (1.0803e-6), which
brackets a float32 `dot > 0.999999` (1.0133e-6).

## Decision

After the length constraint, before colliders, a bone whose direction is within that angle of its rest direction
(the animated offset turned with the parent bone, before version 1.1's gravity bend) is set onto it. The test is the
squared sine against `2 x 1.0133e-6`, because an f32 cosine cannot resolve the threshold (the dot form missed the
snap and drifted 2.6 mm). The step's velocity keeps its unsnapped value (snapping it too sends spring 1 off by
20 cm).

## Consequences

`stiff_v11_advanced` replays within 0.01 mm at 60 and 30 Hz (was 2.7 mm), and steady sway cases drop from about
0.14 mm to a few micrometres in the numpy model. A chain at rest now sits exactly at rest instead of creeping
towards it. Spring 1 (`simplified_p10_s100`, 0.7 mm) stays bounded: each step is exact against the SDK's state,
but its undamped velocity keeps f32 rounding.
