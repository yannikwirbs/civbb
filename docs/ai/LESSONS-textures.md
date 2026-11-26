# Lessons: textures

Read before working on atlasing, mips, KTX2 or the Basis Universal encoder. Add a line whenever something costs more than one attempt.

- `basisu_c_sys` (Basis Universal encoder) runs bindgen at build time: it found Visual Studio's libclang without
  `LIBCLANG_PATH`. Its `default-features = false` build fails (transcoder code compiled out but still referenced):
  keep the defaults and add `encoder`. Its own threading (`BU_COMP_FLAGS_THREADED`) makes UASTC RDO output differ
  between runs; encode single-threaded and parallelise across textures.
- Unity-exported UVs often sit a whole tile off 0..1 (AliciaSolid's V runs 1 to 2); with a repeating sampler
  that is equal to 0..1, so check "each triangle in one unit tile", not "inside 0..1".
- Basis Universal encodes need about 3 GB per 4096-texel texture (UASTC and XUASTC). One encode per core (16) committed
  51 GB and swapped the reference PC: cap encoder threads at 4 (`texture::MAX_WORKERS`), in throwaway studies too,
  and check a long run's memory (`Get-Process ... PagedMemorySize64`) in its first minutes.
- `basisu_c_sys` details: in 2D mode `set_image_slice(i)` for i > 0 supplies mip i (then leave out the GEN_MIPS
  flags); unified UASTC quality below 100 turns RDO on, and a low-level RDO lambda needs `quality_level = -1`.
  ETC1S's quality setting barely changes its error (its ceiling, not a knob). `basisu` 0.1 multiplies the KTX2
  header's level, layer and face counts unchecked (XUASTC path): the client checks them first.
- GPU blitted mips and offline mips of the same image differ visibly in FLIP (0.006 to 0.017 on textured avatars):
  when two paths must render alike, give them one mip function (`civbb_core::mips`), not two filters that "should"
  match. To isolate codec error from mip error, decode the codec's level 0 back to PNG and let both sides mip it.
- Size budgets per texture: an atlas PNG written with fast compression is larger than the textures it replaces and
  lets a bigger codec "fit"; compare like with like (best-compressed PNG).
- `local_avatars_atlas_and_shrink` is `#[ignore]`d (two minutes of codec-ladder encodes, and CI has no corpus/local):
  run it with `--ignored` after changing `atlas` or `texture`.
