# 0031: A skin-aware LOD chain from meshoptimizer: three prefix levels per mesh, built after merging (format v8)

Status: accepted
Date: 2026-11-03

## Context

#33: design doc §9 stage 3 ends with "generate a skin-aware LOD chain with meshoptimizer", the runtime format lists
a "LOD chain", §8's degradation ladder has "lower mesh LOD" as step 3 and the cost model counts "triangles per LOD".
Avatars arrive merged (ADR 0027): one skinned mesh per toggle group with hundreds of shapes, many submeshes,
UV seams and open edges (clothing hems, separate parts). A lower level must keep skinning and shapes working,
keep material boundaries and seams closed, and save GPU work: triangles, and skinned vertices too.

## Decision

1. **Binding: the `meshopt` crate 0.7** (MIT OR Apache-2.0, meshoptimizer 1.3 vendored and built by `cc`, prebuilt
   bindings, no bindgen). It builds with MSVC and GCC like `basisu_c_sys`. meshoptimizer is single-threaded and has
   no randomness, so a build is deterministic on one platform (tested: building twice gives equal avatars).
   Windows and Linux builds may differ in the last float bits (compiler contraction), which only matters for
   cross-platform byte-equal packages; we don't promise that. Its input is the compiler's own validated avatar
   (indices in range, finite positions and normals; non-finite attributes are zeroed), not untrusted bytes.
2. **Levels: up to three below the full mesh**, each aiming at 1/2, 1/4 and 1/8 of the full mesh's triangles with an
   error cap of 0.5 %, 1.5 % and 4 % of the mesh's extent (meshoptimizer's scale). Simplification stops at
   whichever comes first. Levels cascade (each simplifies the one above, error caps account for the error already
   spent). A level is kept only with at most 80 % of the level above's triangles, and the chain stops at the first
   that isn't; meshes that cannot shrink (small props, low-poly avatars) get fewer or no levels. On a 1.6 m avatar
   the caps are 8 mm, 2.4 cm and 6.4 cm: below a pixel at about 4, 13 and 35 m in a 90° 1080-pixel view. The
   error stored per level (in mesh units) lets the governor pick a level by screen-space error later.
3. **Prefix vertices.** A level is its own index list and submeshes over the mesh's vertices. The stage reorders
   vertices so the ones the lowest level uses come first, then those the next level adds, and so on (cascading
   only drops vertices, so the sets nest). A level then uses the first `vertices` vertices: it skins and applies
   shapes to that prefix only (shape deltas are sorted, so a level's deltas are a prefix too). Kept vertices keep
   their own skin weights and shape deltas, so every level skins exactly like the full mesh at its vertices.
   Vertex streams, shapes and the skin are stored once.
4. **Skin- and shape-aware attributes.** meshoptimizer's attribute metric gets, per vertex: the normal (weight 0.5),
   UV0 (1), the skin anchor (the weighted bind positions of its joints, divided by the mesh extent like positions,
   weight 1) and shape motion (the summed length of its shape deltas over the extent, weight 1). Collapsing a vertex
   onto one that skins to other joints, or that shapes move differently, costs about as much as moving it that far,
   so hard joint and shape boundaries survive at every level (tested on a grid; zeroing either weight fails it).
5. **Seams and material boundaries.** Each submesh is simplified on its own, so triangles never change material, and
   with meshoptimizer's `LockBorder`: open edges of a submesh, material boundaries included, keep their vertices.
   meshoptimizer welds UV seams by position and collapses along them, so seams stay closed. Tested on every local
   avatar with vertices welded by position: each level still draws every open edge of the full mesh and opens no
   edge that two triangles shared. **Limit:** an edge three or more triangles share (non-manifold, seen in
   HAOLAN_Lowpoly's flat-shaded wedges) can end up open; locking its vertices did not prevent that, since the
   other triangles go by collapsing their third vertex.
6. **Format v8:** `Mesh::lods: Vec<Lod>`, `Lod { vertices, indices, submeshes, error }`, validated (at most 8 levels,
   vertices not above the previous level's, strictly fewer indices, indices below `vertices`, submeshes matching
   the mesh's materials, finite error). `Mesh::level(n)` gives the mesh as level `n` draws it.
7. **Stage order:** `prune`, `atlas`, `merge`, `lod`, `collapse`, `compress`. It runs after everything that changes
   vertices, indices or submesh materials (`atlas` rewrites materials, `merge` joins meshes and submeshes, both
   would invalidate levels) and before `compress`, which only touches textures.
8. **Viewer:** `civbb-client view --lod N` draws every mesh at level N (`Mesh::level`); the title shows triangles
   per copy.

Measured (`civbb build`, triangles per level, error relative to extent at the lowest level): AliciaSolid 31.8k,
16.4k, 9.2k, 7.8k (4.0 %); Seed-san 45.1k to 11.6k; HAOLAN 146k, 73k, 39k, 21k; Yumeka 84k to 11.6k; Taro 44k to
6.3k; HAOLAN_Lowpoly stops at 9.6k (level 3 saved too little); 256tessaku (256 triangles) gets none. AliciaSolid's
package grows 2.83 to 3.07 MB (index lists).

## Consequences

- The governor (ladder step 3) can switch an avatar's level per frame by drawing another index range and skinning a
  shorter prefix, without new vertex data. Wiring that into the renderer and governor is later work (#63); the
  viewer swaps meshes on load.
- Packages grow by the extra index lists (roughly as much index data again; AliciaSolid's package +9 %). Meshlets and `EXT_meshopt_compression`
  (§9) will shrink both.
- Error caps and attribute weights are judgement calls checked by eye on AliciaSolid; revisit with parity captures
  (M0.9) and when the governor picks levels by screen-space error.
- Faces simplify at level 1 too; shapes still work at kept vertices, but fine expressions lose detail with the
  geometry. If that shows at the distances level 1 is used, weight shape motion higher or lock face vertices.
