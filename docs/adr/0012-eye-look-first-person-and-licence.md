# 0012: Eye look, first-person visibility and licence metadata (format v3)

Status: accepted
Date: 2026-10-28

## Context

ADR 0006 left three parts of design doc §9 `X_avatar` open (#44): eye look, first-person mesh visibility and
creator/licence metadata. VRM carries all three; legacy avatars carry eye-look settings and rely on head-bone
shrinking. Uploads must honour VRM licences, so the licence has to survive import.

## Decision

`FORMAT_VERSION` goes to 3.

- `Face::eyes: Option<EyeLook>`: a driver (eye bones, or blendshape binds for up, down, left, right) and four
  range maps (horizontal inner and outer, vertical down and up), each linear from 0 to `output_scale` over
  0 to `input_max` degrees, held beyond. Outputs are degrees for bones, weights for shapes. This is VRM's
  model; legacy per-direction eye rotations map to it in the exporter. VRM 0.x curves other than the default
  straight line are treated as linear, with a warning.
- `Mesh::first_person: FirstPerson` (`Auto`, `Both`, `FirstPersonOnly`, `ThirdPersonOnly`); `Auto` hides the
  parts skinned to the head, which is legacy head shrinking.
- `Avatar::meta: Option<Meta>`: authors, version, copyright, contact, references and a `Licence` modelled on
  VRM 1.0. Every licence default is the most restrictive value, and a missing or unknown VRM field takes it.
- VRM 0.x licences: `licenseName` maps to our fields (CC0: no credit; BY: credit; NC: never commercial,
  whatever `commercialUssageName` says; ND: modification without redistributing it; SA as BY), its
  Creative Commons deed becomes `url` (unversioned, as 0.x names none); `Other` and unknown names are
  restrictive and keep `otherLicenseUrl`. 0.x has no political or antisocial permission: both are denied.

## Consequences

- Uploads (Phase 1) can refuse or warn on licence terms; showing credits needs only `Meta`.
- One-eye blinks (VRM `blinkLeft`/`blinkRight`) still have no field; add one with the eye-look runtime if needed.
- The CC mapping is our reading of the licences, not legal advice; revisit with the licence review in
  design doc §2 before uploads open.
