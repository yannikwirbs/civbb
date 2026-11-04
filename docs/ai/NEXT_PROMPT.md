# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md, docs/ai/CODEMAP.md and docs/ai/LESSONS.md, then continue M0.9 Reports and parity with #35: FLIP parity per toggle state (design doc §10 "Parity scoring", §21 Phase 0 deliverable 5). Unity reference renders come later (#37, #38 need the human), so decide and record in an ADR what parity means now: our renderer on the source import (no compiler stages) against the compiled package, same camera rig, poses and toggle states, so every compiler stage is proven visually identical; and how the Unity reference slots in later. Decide the camera rig (views, resolution, framing from the bounds), which states (rest plus each toggle, as `tests/common::states` steps them), how pixels come back (`Renderer::headless` draws into `TransferSrc` images: add a readback), the FLIP implementation (a maintained crate or a port of NVIDIA's reference, licence checked, one-line reason in the commit), the thresholds (traffic light) and how results enter the report (ADR 0032 rules: new keys within report v1). Prove it: FLIP of an image against itself is 0 and of a known perturbation matches the reference implementation's value, the lean corpus avatar passes in every state with CI skipping without a device, and every local avatar in corpus/local gets a parity section (`civbb build --report --measure`). Then continue with #58 (toon outlines). Keep `cargo xtask ci` green (CI runners have a Vulkan loader but no driver: device tests must skip), commit and push per issue, check that CI passes after each push (Linux too), and when you stop, update STATUS.md, LESSONS.md and this NEXT_PROMPT.md and push.
```

## For the human, optional

- More Booth avatars: drop the zips in `corpus/local/` and ask the session to convert them (`corpus/README.md`).
- Local fuzzing is optional (CI runs it). Nightly and cargo-fuzz are installed. On Windows, put the MSVC ASan
  runtime on `PATH` first or the target exits `STATUS_DLL_NOT_FOUND`; see `fuzz/README.md`.
