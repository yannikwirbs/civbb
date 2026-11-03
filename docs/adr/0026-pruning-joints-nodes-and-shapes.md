# 0026: The compiler prunes unweighted joints, unreferenced nodes and undriven blendshapes

Status: accepted
Date: 2026-11-03

## Context

#28: design doc §9 stage 3 prunes "bones and blendshapes nothing references", checking animations, menus and face
tracking first. The doc does not say what counts as a reference, how pruning keeps an avatar's behaviour, which
face-tracking names count, or where the stage runs. Avatars converted from FBX skin every mesh to every bone
(Yumeka: 5900 joint slots with no weight), and creators ship hundreds of shapes nothing drives (the bloated corpus
avatar: 1200).

## Decision

1. **`civbb_pipeline::prune` runs in `civbb build` between import and packaging**, on the validated avatar, and
   reports what it removed. The result is valid and animates, simulates and draws as before.
2. **Skin joints:** a joint no vertex gives a non-zero weight leaves the skin with its inverse bind matrix; vertex
   joint indices are remapped (zero-weight entries point at joint 0). A skin whose vertices use no joint is kept.
3. **Nodes:** a node is referenced by a mesh, a remaining skin joint, a humanoid bone, an eye bone, a chain root or
   ignore entry, a collider, a contact volume, a constraint target, source or up node, or a clip curve whose path
   resolves to it (as `civbb-anim` resolves paths, first match wins). Every descendant of a chain root is kept,
   because the chain simulates it, and every ancestor of a kept node is kept, so kept nodes keep their paths,
   parents and world placements. The root always stays. Everything else is removed and indices are remapped;
   node order, and so parents before children, is preserved.
4. **Blendshapes:** a shape stays if its rest weight is not 0, a clip curve on its mesh's path names it, the face
   setup binds it (visemes, blink, eye look), or its name is a face-tracking name: the Unified Expressions shapes
   (base and blended, from docs.vrcft.io) and the ARKit names that do not already match those, compared without
   case. Menus drive parameters, not shapes, so they are covered by the clips. Face binds are remapped.
5. **Not pruned yet:** shapes with a non-zero rest weight that nothing drives (baking them into the base mesh is a
   later optimisation), shapes whose deltas are all zero, and textures or materials nothing uses.

## Consequences

- Real avatars lose most of their skinning matrices and unused shapes before the GPU sees them: AliciaSolid
  sheds 101 joints and 46 shapes; the Booth GLBs shed thousands of joints. Lean loses nothing.
- A package without an animator loses every shape outside face setup and face tracking, because nothing could
  drive them. Avatars converted with their VRChat animator (M0.10) keep what that animator drives.
- New node or shape references in the format must be added to `prune.rs`; the corpus test checks that binding is
  unchanged by pruning, and validation after pruning catches dangling indices.
