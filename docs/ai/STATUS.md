# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`, superseded ADRs in `docs/adr/README.md`.

**Milestone:** M1.1 Native content model (https://github.com/yannikwirbs/civbb/milestones).

## Next

#120 (ADR 0089 points 1 and 3: native units in curve targets). Targets are typed now (#119, format v30), so #120
changes values only: radians and `rotation_euler` axis order (Unity's Z, X, Y lowered in
`gltf_import/unity_clip.rs`), weights 0..1, native muscle identifiers, material curves as native fields with
`srgb`, explicit constant curves for unanimated vector components (today `civbb_anim::bind` adds them). The importer
gets the Unity material name table (`civbb_anim::material::parse`) per fingerprinted family. Then #121 to #125 in
number order (events are Phase 2, #127). Parity reruns are ~1 min with a warm texture cache (ADR 0097); last full
run `corpus/local/unity-ref/scores/s119`.

## Now

- **Phase 0's gate is passed** (ADR 0084; table of all 13 Unity exports there, every red state after rest
  itemised with a Phase 1 issue). Phase 1 is split into milestones M1.1 to M1.9: M1.1 Native content model (#100)
  first, then ECS client runtime, desktop mode/locomotion/IK, OpenXR, protocol/instance server/nodes, voice,
  minimal hub and join flow, avatar fidelity (#109, #107, #110, #92, #93, #94, #77), Phase 1 gate (#61, #62, #63,
  #104). Milestones without issues get them filed from their design doc sections by the session that starts them.
- **M1.1 (#100) is designed**: ADRs 0085 to 0090, design doc §9. Done: #113 to #119 (state table ADR 0095, chain
  outputs, platform values ADR 0096, sync encoding ADR 0098, typed curve targets; format v30: packages in
  corpus/local need `civbb build` again). #128: build-stage timings and the texture cache (ADR 0097).
- Parity rerun after any change to materials, dynamics or the renderer: `tools/parity.sh --against <last run>
  <out dir>` (~1 min with a warm texture cache). The ADR 0084 table must not regress; Yumeka's rest worst view is 0.1197, 0.0003
  under red. Last full run: `corpus/local/unity-ref/scores/s119` (#119), every report identical to s118.
- Session cost: ADRs 0091 and 0092 (2026-11-26). After about five sessions, compare `python
  tools/session-stats.py --since 2026-11-26` with the baseline in `docs/iteration-baseline.md` ($9.2 and 28 M
  cache-read tokens per closed issue) and record the result there.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` (ADR 0045).

## Blockers and needs-user

- #126 (needs-user, not blocking): register a glTF vendor prefix (`CIVBB_` is provisional until the final name, D9).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-26 (3): #116 chain outputs (`Chain::outputs`, one entry per output, written by `Island::step`; format v27)
  and #117 platform values (`state::Platform`, `platform/` names, read-only to content, ADR 0096; format v28).
  Parity s116 and s117 equal to s115. #128 filed (parity
  is build-bound). Next: #128, then #118.
- 2026-11-26 (4): #128 per-stage build timings (`compress` is ~all of a build) and an opt-in texture cache (ADR
  0097; parity rerun 15 min cold, 1 min 12 s warm, byte-identical packages); #118 sync encoding per entry
  (`Entry::range`, `sync_bits`/`encode`/`decode`, ADR 0098, format v29; corpus totals unchanged). Next: #119.
- 2026-11-26 (5): #119 typed curve targets (`animator::Target`: node, mesh, humanoid, entry, constraint by index;
  masks by node; format v30). Unity paths resolved by `gltf_import/unity_clip.rs`, unresolvable curves dropped with
  a warning; prune, merge and atlas remap through `renumber::Renumber`; `node_paths` left `civbb-anim`. Next: #120.
