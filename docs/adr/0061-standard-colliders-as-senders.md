# 0061: The descriptor's standard colliders export as body-part contact senders

Status: accepted
Date: 2026-11-20

## Context

VRChat gives every humanoid avatar contact senders with built-in body-part tags "if they are not manually defined by
the avatar author" (Contacts docs, *Built-in contact tags*): `Head`, `Torso`, `Hand`/`HandL`/`HandR`,
`Foot`/`FootL`/`FootR`, `Finger`/`FingerL`/`FingerR`, `FingerIndex`, `FingerMiddle`, `FingerRing`, `FingerLittle` and
their `L`/`R` forms. Their shapes are the avatar descriptor's 14 standard colliders (head, torso, hands, feet, four
fingers per side), each Automatic, Custom or Disabled. All 224 in the host project's prefabs are Automatic with
nothing stored: the SDK computes them (#78). The docs also say the hand and finger colliders become global PhysBone
colliders for other avatars' and worlds' chains.

## Decision

- The exporter calls the SDK's public `VRCAvatarDescriptor.UpdateAutoColliders()` by reflection on the loaded prefab
  contents, then writes each collider that is not Disabled as an `X_contacts` sender: sphere when its height is 0,
  else a capsule along local Y, on the collider's bone with its stored position and rotation, in the bone's local
  units like every other contact (HAOLAN: head sphere r 5.76 on `Head`, its bones scaled 0.01). The SDK picks the
  bones: feet go on the toes when mapped, fingers on their last bone (Taro: intermediate), and a collider without a
  bone is skipped and counted (Blackbird's little fingers, Ostgaroa's fingers). 155 standard senders across the 13
  exports.
- Tags: each collider sends its specific tag and every broader one (`FingerIndexL` also sends `Finger`, `FingerL`,
  `FingerIndex`; `HandL` also `Hand`). A collider is left out when an author's sender already carries its most
  specific tag.
- No format change: these are ordinary senders, so the runtime, validation and caps treat them as authored ones.
- Custom colliders export as stored; the descriptor's `isMirrored` (right side copied from the left in the SDK's
  inspector) is not applied. No corpus avatar has a Custom collider.

## Consequences

- Receivers on other avatars (and on the avatar itself, with Allow Self) detect heads, hands and fingers as in
  VRChat. No corpus export has receivers (#77), so no parity score changes.
- Inferred, not recorded: the rule for "manually defined" (per most specific tag) and the capsule's height being
  total height as on contacts. The VRChat client creates the senders, not the SDK, so neither is probeable in Unity.
- glTF and VRM avatars without the exporter get no standard senders; generating them from the humanoid would need
  the SDK's `Calc*Collider` sizing rules.
- Hand and finger colliders as global PhysBone colliders for other avatars' chains are not modelled (#92).
