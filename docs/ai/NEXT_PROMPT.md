# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md, docs/ai/CODEMAP.md and docs/ai/LESSONS.md, then start M0.10 Unity exporter with #47: decide in an ADR how the VRChat components in the Booth avatars' .unitypackage files (humanoid map, visemes, PhysBones, contacts, FX animators, expression menus) reach the pipeline: (a) the Unity editor plugin of ADR 0002 and design doc §10 (#36, a separate repo), or (b) a bounded, fuzzable Rust reader of the unitypackage (tar.gz of Unity YAML with GUID references) inside the trust boundary. Look at the actual unitypackages in corpus/local/booth before deciding (never commit their contents), weigh what each option gives the compiler now and what it costs to own, and record the numbers (asset counts, component types, file sizes) in the ADR. Then continue with the lowest-numbered M0.10 issue that does not need the human (#37 and #38 do). Keep `cargo xtask ci` green (CI runners have a Vulkan loader but no driver: device tests must skip), commit and push per issue, check that CI passes after each push (Linux too), cap any parallel texture encoding at 4 threads (LESSONS), and when you stop, update STATUS.md, LESSONS.md and this NEXT_PROMPT.md and push.
```

## For the human, optional

- More Booth avatars: drop the zips in `corpus/local/` and ask the session to convert them (`corpus/README.md`).
- Local fuzzing is optional (CI runs it). Nightly and cargo-fuzz are installed. On Windows, put the MSVC ASan
  runtime on `PATH` first or the target exits `STATUS_DLL_NOT_FOUND`; see `fuzz/README.md`.
