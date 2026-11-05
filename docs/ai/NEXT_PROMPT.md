# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md, docs/ai/CODEMAP.md and docs/ai/LESSONS.md. First check the Unity licence (`C:/Users/xxsay/AppData/Local/Unity/bin/unity.exe license status --format json`, #72). If it is active: close #72, create the exporter's host project as tools/unity-exporter/README.md describes, import the five Booth avatar packages in corpus/local/booth with Civbb.Exporter.Batch.Import, export each humanoid prefab with Batch.Export, import the GLBs with `civbb inspect` / `civbb build` and compare them with the Blender conversions in corpus/local/glb (meshes, triangles, shapes, joints, humanoid bones), fix what differs, commit the package's generated .meta files, and record the comparison on #36. Then continue #36 with animator controllers, clips (ADR 0039) and Unity constraints into X_* extensions, with the Rust side in #71. If the licence is still not active, do not activate it yourself (accepting the terms is the owner's call): work on #71 (start with X_avatar.humanoid, which the exporter writes; ADR 0038) and #55 (box contacts, receiver value, component node; VRChat's public Contacts page; mark what it leaves open as unverified). Never commit corpus/local contents. Keep `cargo xtask ci` green (CI runners have a Vulkan loader but no driver: device tests must skip), commit and push per issue, check that CI passes after each push (Linux too), cap any parallel texture encoding at 4 threads (LESSONS), and when you stop, update STATUS.md, LESSONS.md and this NEXT_PROMPT.md and push.
```

## For the human

- **#72: the Unity Personal licence lapsed.** Every Unity batch run exits 198. Reactivate it with
  `C:/Users/xxsay/AppData/Local/Unity/bin/unity.exe license activate --personal --accept-eula` (this accepts the
  Unity Personal terms), or in Unity Hub under Preferences, Licenses.
- The Unity CLI has an update (1.0.0-beta.14 to beta.16): `unity self-update`, if you want it.
- More Booth avatars: drop the zips in `corpus/local/` and ask the session to convert them (`corpus/README.md`).
- Local fuzzing is optional (CI runs it). Nightly and cargo-fuzz are installed. On Windows, put the MSVC ASan
  runtime on `PATH` first or the target exits `STATUS_DLL_NOT_FOUND`; see `fuzz/README.md`.
