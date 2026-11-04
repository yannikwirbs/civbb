# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md, docs/ai/CODEMAP.md and docs/ai/LESSONS.md, then continue M0.9 Reports and parity with #65 and #68 together: the texture codec and quality choice. FLIP parity (ADR 0033, `civbb build --report --measure`) shows every textured corpus/local avatar yellow or red, from ETC1S compression (about 0.04 mean on AliciaSolid; AlphaBlendModeTest red because its stripes turn green) and from atlas mips (about 0.015), while #65 says KTX2 can be larger than the source PNG/JPEG. Measure per texture kind (colour, normal, masks) and platform: UASTC with and without RDO, ETC1S at higher quality, and what the atlas does to mips (gutters, tile alignment), against both package size and parity on all of corpus/local; record the choice in an ADR (superseding the codec part of ADR 0029 if it changes) with the numbers, and aim for green parity on the textured avatars at a size no larger than the source. Then #69 (frame the parity rig from the rest state's world-space vertices so Seed-san's hands fit) and #66 (constraint pass cost). Keep `cargo xtask ci` green (CI runners have a Vulkan loader but no driver: device tests must skip), commit and push per issue, check that CI passes after each push (Linux too), and when you stop, update STATUS.md, LESSONS.md and this NEXT_PROMPT.md and push.
```

## For the human, optional

- More Booth avatars: drop the zips in `corpus/local/` and ask the session to convert them (`corpus/README.md`).
- Local fuzzing is optional (CI runs it). Nightly and cargo-fuzz are installed. On Windows, put the MSVC ASan
  runtime on `PATH` first or the target exits `STATUS_DLL_NOT_FOUND`; see `fuzz/README.md`.
