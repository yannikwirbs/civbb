"""Blender headless: imports one FBX into an empty scene and exports it as GLB (meshes, skins, shape keys).

blender --background --factory-startup --disable-autoexec --python fbx_to_glb.py -- in.fbx out.glb search-dir

Textures referenced by absolute paths on the author's machine are relinked by file name (any image extension)
found under search-dir.
"""
import os
import sys

import bpy

src, dest, search = sys.argv[sys.argv.index("--") + 1 :][:3]
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.fbx(filepath=src)
found = {}
for d, _, files in os.walk(search):
    for f in files:
        stem, ext = os.path.splitext(f)
        if ext.lower() in (".png", ".jpg", ".jpeg", ".tga", ".psd"):
            # Prefer PNG/JPEG over PSD for the same stem.
            if stem.lower() not in found or ext.lower() in (".png", ".jpg", ".jpeg"):
                found[stem.lower()] = os.path.abspath(os.path.join(d, f))
for i in bpy.data.images:
    if i.source == "FILE" and not i.has_data and not i.packed_file:
        stem = os.path.splitext(os.path.basename(i.filepath.replace("\\", "/")))[0].lower()
        if stem in found:
            i.filepath = found[stem]
            i.reload()
missing = [i.filepath for i in bpy.data.images if not i.packed_file and not os.path.exists(bpy.path.abspath(i.filepath))]
bpy.ops.export_scene.gltf(
    filepath=dest,
    export_format="GLB",
    export_animations=False,
    export_skins=True,
    export_morph=True,
    export_morph_normal=True,
    export_apply=False,
)
meshes = [o for o in bpy.data.objects if o.type == "MESH"]
keys = sum(len(o.data.shape_keys.key_blocks) - 1 for o in meshes if o.data.shape_keys)
print(f"CONVERTED {dest}: {len(meshes)} meshes, {keys} shape keys, {len(bpy.data.images)} images, missing {missing}")
