# 0047: Contacts follow the SDK's own runtime: boxes, face proximity, the component's node, proximity's first frame (format v11)

Status: accepted
Date: 2026-11-19

## Context

ADR 0017 built contacts from VRChat's documentation and marked the proximity end points, the capsule "centre" and
OnEnter's speed **unverified**; boxes, face proximity and the component's own node waited for #55. The VRChat SDK
(3.10.6) simulates contacts in editor play mode (#75). `tools/unity-truth/Assets/Editor/VrcContacts.cs` records it
in the exporter's host project: in each generated case a sender moves through a receiver, one step a frame at a fixed
1/60 s, and the receiver's `paramValue` is read every frame. 39 cases (`crates/dynamics/tests/unity/vrc_contacts.json`):
sphere, capsule and box receivers and senders, the three receiver types, Min Velocity, offsets, turned and scaled
volumes, face proximity at the face, its edges and behind it, and a receiver component on another node than its root
transform, switched off. The docs' Contacts page says face proximity is "the closest point between the surface of the
sender and the positive Z face"; it is silent on the rest below. SDK 3.10.6 has no receiver *Value* field (#55
listed one), and adds receiver features ADR 0017 never had (advanced parameters such as sender velocity and point
separation, writing to materials); those are #77. Automatic body-part senders, also in #55, are #78.

What the recordings show:

- Sphere and capsule receivers behave as ADR 0017 says: proximity is 1 minus the distance from the receiver's core
  segment to the sender's surface over the receiver's radius; OnEnter's speed is the centres' displacement over the
  frame; scale applies.
- Capsule and box senders are measured by their true closest point.
- Box receivers overlap a sender geometrically, but their proximity is not a distance over a radius: it is
  `1 - max_k |q_k| / h_k`, `q` the sender's point closest to the box centre in the box's frame and `h` the half edges
  (a sphere stretched to the box, measured in the box's own units).
- Face proximity is 1 at the +Z face and 0 at the -Z face: `1 - (h_z - top) / 2 h_z`, `top` the highest Z of the
  sender within the box's X and Y extent (a sender beside the box counts only with the part of it over the face).
- A Proximity receiver reads 0 on the first frame a sender touches it and its value from the second (the same
  distance reads 0.12 leaving and 0 arriving). Constant and OnEnter fire on the first frame.
- Deactivating the receiver component's node switches it off; deactivating its root transform only stops it moving.

## Decision

1. **Format v11.** Contacts get their own `ContactShape` (sphere, capsule, box with `size`); colliders keep `Shape`
   (a plane is no longer a contact shape to reject). `ContactReceiver::face` and `ContactVolume::component` (the
   node whose activity enables the contact, `None` when it is `node`) default for older data. The exporter writes
   all three.
2. **Runtime.** A placed volume is a core segment with a radius or a box. Overlap: segment distances, segment-to-box
   distance, separating axes for two boxes; touching counts. Proximity as recorded above. Segment-to-box distance and
   the face's highest point are golden-section searches over the core segment (both are convex or concave along it).
3. **Proximity's first frame.** A sender counts towards a Proximity receiver from its second frame of contact.
4. The rest of ADR 0017 stands; its unverified proximity end points, capsule centre and OnEnter speed are now
   verified for one sender. Still unverified: several senders at once (the docs: the closest wins), box senders'
   highest point beside a face receiver's extent (taken as the highest corner), OnEnter's used-up rule, and anything
   between avatars (the recordings run one scene with Allow Self and Allow Others on).

## Consequences

- `crates/dynamics/tests/vrc_contact.rs` replays every frame within 1e-4 (from the SDK's second frame: it registers
  contacts during the first). Mutating the first-frame rule or face proximity fails 29 and 248 frames.
- The recorded paths sit a quarter step off the distances where shapes just touch: float noise at a touching
  distance went either way in the SDK.
