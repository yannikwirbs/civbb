# Lessons: avatar data

Read before working on Booth archives, Blender conversions, GLB files, meshes or goldens. Add a line whenever something costs more than one attempt.

- Booth archives: zip names are Shift-JIS without the UTF-8 flag (`corpus/extract_zip.py` decodes them), FBX textures point at the author's absolute paths (`corpus/fbx_to_glb.py` relinks by file name), and Blender images report `has_data = False` until loaded, so check file existence instead. Always run Blender with `--factory-startup --disable-autoexec` on third-party files.
- meshoptimizer (`meshopt` crate): `LockBorder` keeps open edges, and seams stay closed on manifold surfaces, but
  an edge three or more faces share can open (its faces go by collapsing their third vertex; locking the edge's
  vertices does not help). Check simplified meshes with vertices welded by position and edge use counts, not raw
  indices: wedges (split normals, UV seams) make every flat-shaded edge look open.
- Goldens pin counts, not encoded sizes: the signed lean package was 125,221 bytes on Windows and 125,199 on the
  Ubuntu runner with every count equal. `serde_json::Value` sorts keys (no `preserve_order`), so a golden written
  through it is alphabetical; compare by value.
- Blender's GLB morph targets are sparse accessors without a `bufferView`; scratch GLB readers must handle sparse.
  The Unity export shares one attribute set across a mesh's primitives, so count vertices per accessor, not per
  primitive. Install numpy with `python -m pip install --user numpy` for such scripts.
