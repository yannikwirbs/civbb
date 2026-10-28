# corpus

Test avatars. Only CC0 or generated assets are committed (the generator lives in `tools/corpus`); everything
third-party stays in `corpus/local/` (gitignored, design doc §21).

Layout of `corpus/local/` as the tests expect it:

- `khronos/`: Khronos glTF samples (fetch list on #9).
- `vrm/`: VRM files.
- `glb/`: Booth avatars converted from FBX.
- `booth/<name>/`: extracted Booth archives; `tools/blender/`: portable Blender.

Converting a Booth archive (untrusted: extract into a new directory, never let Blender run embedded scripts):

```sh
python -I corpus/extract_zip.py corpus/local/X.zip corpus/local/booth/X
blender --background --factory-startup --disable-autoexec --python corpus/fbx_to_glb.py -- \
  corpus/local/booth/X/.../X.fbx corpus/local/glb/X.glb corpus/local/booth/X
cargo test -p civbb-pipeline local_glbs -- --nocapture
```

`fbx_to_glb.py` relinks textures that the FBX references by the author's absolute paths. The conversion carries
meshes, skins, shape keys and textures only; PhysBones, animators and menus live in the `.unitypackage` (#47).
