# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md, docs/ai/CODEMAP.md and docs/ai/LESSONS.md. The Unity exporter runs (#36): host project corpus/local/unity-export (gitignored) already holds the five Booth packages; Batch.Import runs through the Editor binary without -quit, Batch.Export through `unity run` (tools/unity-exporter/README.md, controllers named with -civbbLayers as in the last #36 comment). First check #37: if the owner has created the VRChat SDK host project (corpus/local/unity-export-vrc, Unity 2022.3.22f1), add the exporter and the Booth packages there and continue #36 with the avatar descriptor (playable layers instead of -civbbLayers, visemes, viewpoint, eye look into X_avatar), expressions, PhysBones, contacts and VRChat constraints into X_* (ADR 0041), with the Rust side in #71. Otherwise: (1) verify exported data against Unity: sample a few exported clips (transform rotation, muscle, blendshape curves) and HAOLAN PC's two RotationConstraints in Unity (batch play mode as in tools/unity-truth) and compare with civbb-anim on the imported GLB; fix what differs; (2) X_toon_material: map the Booth materials' shaders (extras.unityShader, saved properties) to the native Toon/PBR models in the exporter and importer; (3) #55, #73. Never commit corpus/local contents. Keep `cargo xtask ci` green (CI runners have a Vulkan loader but no driver: device tests must skip), commit and push per issue, check that CI passes after each push (Linux too), cap any parallel texture encoding at 4 threads (LESSONS), and when you stop, update STATUS.md, LESSONS.md and this NEXT_PROMPT.md and push.
```

## For the human

- #37: install the Creator Companion (or `vrc-get`), accept the VRChat SDK licence, and create an avatar project
  with Unity 2022.3.22f1 at `D:/Documents/Repos/CIVBB/corpus/local/unity-export-vrc` with `com.vrchat.avatars`;
  then comment on #37. The exporter needs the SDK's scripts to read VRChat components.
- More Booth avatars: drop the zips in `corpus/local/` and ask the session to convert them (`corpus/README.md`).
