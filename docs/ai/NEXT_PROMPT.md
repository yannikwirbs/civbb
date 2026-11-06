# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md, docs/ai/CODEMAP.md and docs/ai/LESSONS.md. Unity works again (#72 closed; Editor 6000.3.28f1, find it with `unity editors path`, run with `unity run`, see LESSONS). Create the exporter's host project as tools/unity-exporter/README.md describes, import the five Booth avatar packages in corpus/local/booth with Civbb.Exporter.Batch.Import, export each humanoid prefab with Batch.Export, import the GLBs with `civbb inspect` / `civbb build` and compare them with the Blender conversions in corpus/local/glb (meshes, triangles, shapes, joints, humanoid bones), fix what differs, commit the package's generated .meta files, and record the comparison on #36. Then continue #36 with animator controllers, clips (ADR 0039) and Unity constraints into X_* extensions, with the Rust side in #71. If Unity batch runs fail again (exit 198), do not activate the licence yourself: work on #71 and #55 instead. Never commit corpus/local contents. Keep `cargo xtask ci` green (CI runners have a Vulkan loader but no driver: device tests must skip), commit and push per issue, check that CI passes after each push (Linux too), cap any parallel texture encoding at 4 threads (LESSONS), and when you stop, update STATUS.md, LESSONS.md and this NEXT_PROMPT.md and push.
```

## For the human

- More Booth avatars: drop the zips in `corpus/local/` and ask the session to convert them (`corpus/README.md`).
- Local fuzzing is optional (CI runs it). Nightly and cargo-fuzz are installed. Works on Windows (`fuzz/README.md`); put the MSVC ASan
  runtime on `PATH` first or the target exits `STATUS_DLL_NOT_FOUND`; see `fuzz/README.md`.
