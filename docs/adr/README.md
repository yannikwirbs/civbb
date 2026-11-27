# Architecture decision records

One file per decision, `NNNN-title.md`, from `0000-template.md`. Accepted ADRs are not edited; a later ADR supersedes
them in whole or in part. Read an ADR together with the ones that supersede parts of it:

- 0035 supersedes the codec, mip and gutter parts of 0029.
- 0036: the rig bounds of 0033.
- 0044: the session-state and orientation bullets of 0005.
- 0046: the capture frame and rebaking of 0040.
- 0047 refines 0017's shapes and proximity.
- 0049 supersedes 0015 and the solver points of 0016.
- 0055: the frame count of 0033.
- 0062: the HDR conversion and reference time of 0052.
- 0065: 0010's WD-on empty states and 0028's `Start` state.
- 0066: the interruption points of 0008.
- 0067: the material default of 0025.
- 0068: the version 1.0 gravity of 0049.
- 0070: the collider velocity of 0049.
- 0072 corrects 0064's thumb account.
- 0074: the partial output step and in-frame interpolation of 0049.
- 0077 defers 0023 point 7's GPU culling to Phase 2 (#59).
- 0085: points 1 and 5 of 0041.
- 0092: the session start, end and handover parts of 0044.
- 0099: the constant curves of 0089 point 3 (unanimated vector components are baked) and 0025's flat name table.
- 0101: the node-level placement of 0086 point 4 (components stay root lists).
- 0102: point 2 of 0085 (fork on divergence) for shape-only changes: dialect 1 is rewritten as JSON.
- 0103 supersedes 0003 and the `jobs` part of 0020 point 1 (bevy_tasks pools instead of rayon, with a fallback).
- 0105: point 3 of 0103 for island and extract jobs (an own fork-join pool, no ComputeTaskPool or rayon fallback).
- 0109: 0108's consequence that an empty Gesture group shows no hand gestures (a default Gesture layer per hand).
