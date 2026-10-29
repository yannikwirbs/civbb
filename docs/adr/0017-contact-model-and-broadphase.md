# 0017: Contact model and a grid broadphase in `civbb-dynamics`

Status: accepted
Date: 2026-10-29

## Context

#20 adds contact senders and receivers (design doc §7) and the cross-avatar pass that evaluates them (§5 step 6:
one spatial broadphase over every avatar's contact shapes, split by grid cell, results feeding next frame's animator
parameters). The model below comes only from VRChat's public documentation: the Contacts page
(creators.vrchat.com/common-components/contacts), its built-in tag list and the community wiki. Where they are
silent, the choice is ours and is marked **unverified**.

Documented: shapes sphere, capsule (height along Y including the caps) and box; position and rotation offsets from
a root transform; radius at most 3 m and any extent at most 6 m, after scaling; up to 16 case-sensitive collision
tags, and a sender and receiver interact when they share at least one; `Allow Self` and `Allow Others` (receivers
on avatars); `Local Only` on senders and receivers ("only work on the local client"); receiver types Constant
(present while contact lasts), OnEnter (only on the frame contact begins, reset the next frame, optional
`Min Velocity` of the incoming collider) and Proximity (a float 0 to 1 by how close the sender's closest point is
to the receiver's centre, the closest sender winning); receiver parameters need not be synced; a world contact's
enter velocity is the sender's velocity relative to the receiver; humanoid avatars get automatic body-part senders
(`Head`, `Hand`, `FingerIndexL` ...) unless the author placed their own.

## Decision

1. **Where it lives.** `civbb-dynamics::contact`, an engine crate depending only on `civbb-format`. The broadphase
   is a function over plain per-avatar inputs (compiled contacts, world node poses, node activity, a stable avatar
   id, and per-avatar state), so it needs no client types. The client (glue) gathers every avatar after the island
   jobs, calls it once per frame and writes each receiver's value into that avatar's animator parameter before the
   next frame's animator step. `cargo xtask arch` needs no change.
2. **Format v5.** Senders gain `local_only`; contact shapes must be a sphere or capsule with finite, non-negative
   sizes (validation rejects planes). Boxes, face proximity, the receiver `Value` field and a component node
   separate from the root transform are not in the format yet (#55).
3. **Placement.** A volume follows its node's world pose (uniform scale, ADR 0015 `Pose`). A sphere is a point and
   a capsule a segment of length `height - 2 radius` (at least 0) along its local Y, both with a radius. After
   scaling, radius is clamped to 3 m and height to 6 m. A contact exists only while its node is active.
4. **Filters.** A pair counts when they share a tag (first 16 of each); the sender is on the receiver's avatar and
   `allow_self` is set, or on another avatar, `allow_others` is set and the receiving avatar's owner permits that
   avatar (the per-user "everyone, friends or nobody" setting, passed in by the client). Each client evaluates every
   receiver itself, since their parameters are not synced; a `local_only` sender or receiver exists only on its
   owner's client. That reading of "local client" is inferred, **unverified**.
5. **Overlap.** The receiver's and sender's core segments are within the sum of their radii (touching counts).
6. **Values,** each frame:
   - Constant: 1 while any sender overlaps, else 0.
   - OnEnter: 1 on a frame where a sender overlaps that did not overlap on the previous frame, with a speed relative
     to the receiver (`|v_sender - v_receiver|`, centres' displacement over the frame, 0 on the first frame) of at
     least `min_velocity` m/s; else 0. Each sender enters on its own; an entry that is too slow is used up until the
     sender leaves. Per-sender entry, the speed measure and the used-up rule are **unverified**.
   - Proximity: per overlapping sender, `1 - max(d - r_sender, 0) / r_receiver` clamped to 0 to 1, where `d` is the
     distance between the core segments: 0 when the sender's surface just touches the receiver's edge, 1 when it
     reaches the receiver's centre (core segment for capsules). The largest wins. These end points and the capsule
     "centre" are **unverified**.
   Constant and OnEnter output 1; the client maps it to `true` or `1` for bool and int parameters.
7. **Broadphase.** A uniform grid of 1 m cells. Senders are inserted into every cell their bounding box covers;
   each receiver collects the senders in its cells, deduplicated, then runs the narrow test. Receivers are
   independent of each other, so splitting them across workers (ADR 0003) is a later change to the caller.

## Consequences

- Hand-derived tests cover every receiver type, tags, the filters, local-only, permits, scale and clamps, inactive
  nodes, cell boundaries and a grid-against-brute-force comparison.
- The automatic body-part senders (sizes and placement undocumented) and the Avatar Descriptor's standard colliders
  are not generated yet; they come with the client or the converter (#55). Until then avatars only see the senders
  they carry.
- Everything marked unverified is checked against VRChat behaviour at parity (M0.9); a cheap one to confirm is the
  Proximity end points.
