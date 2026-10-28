# 0007: Curves bind to a flat slot buffer at compile time; keys follow Unity's AnimationCurve

Status: accepted
Date: 2026-10-28

## Context

Design doc §7 "Animation" and §9 stage 5 ask for curves bound to component indices at compile time, with Unity
key semantics and exact Write Defaults. The doc does not say how bindings are represented, what a property's
default is, how partial rotations and Euler curves work, or what happens to curves that bind to nothing (#13).

## Decision

1. `civbb_anim::bind(&Avatar)` gives every animated property one slot in a flat `f32` buffer, shared by all clips,
   with a `Target` and a default. Clips become lists of `(slot, keys)` plus the sorted set of slots they write,
   which is what Write Defaults needs (#16). Values stay in curve units (blendshapes 0 to 100); the pose stage
   converts.
2. Defaults are the avatar's initial state: rest transform, blendshape rest weight, node active flag, renderer on,
   parameter default. These stand in for Unity's "values when the animator binds".
3. Keys: Hermite between keys with tangents per second (scaled by segment length), clamped to the end keys outside
   them, and a segment holds its left key's value when either of its two tangents is not finite. No clamping of
   overshoot. Equal key times are rejected by validation; Unity weighted tangents are not represented (#50).
4. Rotations: a node's 4 quaternion components always get 4 consecutive slots. A clip animating some components
   fills the rest with constant rest values and normalises after sampling. Euler curves are degrees about our
   axes, applied Z, then X, then Y (`q = qy * qx * qz`, Unity's order after the exporter mirrors values to our
   axes); missing Euler components hold the rest rotation's angles. If a clip has both on one node, the quaternion
   curves win.
5. Curves that bind to nothing are reported with a reason and skipped, as Unity ignores them: missing node, mesh,
   blendshape or parameter, empty curves, duplicates (first wins), non-float parameters, and for now humanoid
   (#48) and material (#49) curves.
6. Binding is a pure function of the avatar, so it runs wherever it is called. Storing the bound tables in the
   package comes with ACL compression; until then the client binds at load.

## Consequences

- The runtime never touches strings; sampling is a loop over slot curves.
- Choices not yet checked against Unity: empty-curve handling, float-only animated parameters, quaternion-over-Euler
  precedence, and the default of a renderer flag. The Unity recordings (M0.10) settle them; supersede this ADR if
  any differ.
