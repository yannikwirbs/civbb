# 0027: The compiler merges meshes that show together and joins submeshes that share a material

Status: accepted
Date: 2026-11-03

## Context

#29: design doc §9 stage 3 merges "meshes that share a material and toggle together". The doc does not say what
"toggle together" means, how skins, blendshapes and their names combine, what first-person visibility needs, or
how clips that reach meshes by node path keep working. Each mesh is one skinning job and each submesh one draw
(ADRs 0022, 0023). Avatars converted from FBX ship many meshes with nothing toggling them (Yumeka: 20).

## Decision

1. **`civbb_pipeline::merge` runs in `civbb build` after `prune`** (ADR 0026) and reports the meshes and submeshes
   it removed. The avatar stays valid and shows the same triangles, materials and shape weights in every state.
2. **Shown together:** two meshes merge when the nodes above them (their own included) that a clip's `Active`
   curve reaches are the same set. Every other ancestor must be active at rest, so activity is the same function of
   the same animated nodes in every animator state, whatever the layers, transitions or Write Defaults do. Their
   first-person visibility and their optional vertex streams (tangents, UV0, UV1) must match too.
3. **Never merged:** a mesh whose node a `RendererEnabled` or `Material` curve reaches (merging would spread the
   curve to other parts), a mesh that shares its node with another mesh or whose node's path resolves to an
   earlier node (curves would bind elsewhere), a mesh with a blended submesh (blended draws keep their order), and a
   mesh some inactive, unanimated ancestor always hides.
4. **Grouping:** in mesh order, a mesh joins the first group with its key unless one of its shape names is already
   in the group (curves and face tracking find shapes by name), a shape curve on the group's path names one of its
   shapes (that curve was unbound and would start binding), or the joints would overflow 16-bit indices.
5. **Combining:** the merged mesh sits on the first member's node and keeps its name. Vertex streams, indices,
   submeshes and shapes are appended with offsets. Skins merge with joints shared when node and inverse bind are
   equal; a rigid mesh becomes skinned to its own node with an identity inverse bind, which places it as before.
   Face shape binds are remapped; shape curves on a merged member's path move to the merged mesh's path when they
   named one of that member's shapes. Merged-away nodes stay (they may carry curves or children).
6. **Submeshes:** in every mergeable mesh (merged or alone), submeshes with the same material join into one, in
   order of first use. Only opaque and cutout draws are reordered this way; they are depth-tested.
7. **Proof:** tests step islands of the avatar before and after merging through the rest pose and each parameter
   flipped in turn, and compare every visible triangle in world space after shapes and skinning, with its
   material, plus the animated material records: bit-identical on the corpus and the local avatars.

## Consequences

- Draws per copy in `civbb-client view`: Yumeka 23 to 7 (skinned meshes 20 to 1), HAOLAN 15 to 9, AliciaSolid 20
  to 13. The bloated corpus avatar only loses one skinned mesh (body and tails): each prop and outfit has its own
  toggle, so nothing that shares a material shows together. Merging across toggles would need per-part visibility
  in the renderer (a shader toggle), which changes the runtime; not done.
- Merged meshes are bigger units for later stages: GPU culling (#59) culls them as one, LODs (#33) simplify them as
  one. A draw-order change between coplanar opaque triangles is possible in principle and accepted.
- `civbb-pipeline` tests use `civbb-client` as a dev-dependency to step islands; the layering rule covers normal
  dependencies only.
- Shapes with the same name on meshes that show together keep those meshes apart. Joining same-name shapes driven
  identically is a later optimisation if real avatars need it.
