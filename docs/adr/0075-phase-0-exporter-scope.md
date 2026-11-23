# 0075: Phase 0 exporter without the non-destructive build and export-time renders

Status: accepted
Date: 2026-11-23

## Context

Design doc §21 lists Phase 0's first deliverable as the Unity exporter "including the non-destructive build step
and the reference renders". By the end of M0.10 the exporter covers every component the 11 local avatar packages
use (#36), and Unity renders each export as a parity reference through `Reference.Render` and a hand-off from
`civbb-client rig` (ADR 0048). Two parts are not done:

- The NDMF / Modular Avatar / VRCFury build before export. A survey of all 13 local `.unitypackage` files
  (pathnames and text assets searched for `nadena`, `ndmf`, `ModularAvatar`, `VRCFury`, `VF.Model`) found no use;
  the host project has 0 missing scripts in 26 prefabs. Nothing in the corpus can verify it.
- Rendering during export on the creator's machine (§10 step 3) and a `unity_parity` key in
  `civbb build --report` (ADR 0033). The rig hand-off comes from a built package, so an export cannot make it yet.

## Decision

Phase 0 ends with both parts deferred to Phase 3 (creator upload pipeline), where creator exports first matter:
#97 (pre-build, with a generated test avatar that uses those tools) and #98 (export-time renders and
`unity_parity`). Parity against Unity keeps running locally through `corpus/local/unity-ref`.

## Consequences

- Phase 1 starts without them; none of its avatars needs them.
- A kitbashed avatar that relies on Modular Avatar or VRCFury exports in its unbuilt form until #97: its outfits and
  menus are missing or doubled. The exporter should warn when it sees their components (part of #97).
- Revisit if a corpus avatar or the first external creator uses these tools before Phase 3.
