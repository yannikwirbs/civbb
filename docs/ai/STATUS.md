# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`, superseded ADRs in `docs/adr/README.md`.

**Milestone:** M1.1 Native content model (https://github.com/yannikwirbs/civbb/milestones).

## Next

#122 (ADR 0085 points 5 and 6: document role and VRM 1.0 humanoid bone names), then #123 to #125 in number order
(events are Phase 2, #127). #122 is a root split touching every crate that names `Avatar` (`crates/format/src/
avatar.rs`); renaming `HumanBone` variants also renames muscle targets through serde (#120). #121 is done (format
v32): `State::unwritten`, `Action`, `EntryOp`, `PosePart`/`PoseSource`, `Mask`; dialect 1 lowered by
`unity_state::lower_states`. Parity reruns are ~1 min with a warm texture cache (ADR 0097); last full run
`corpus/local/unity-ref/scores/s121`.

## Now

- **Phase 0's gate is passed** (ADR 0084; table of all 13 Unity exports there, every red state after rest
  itemised with a Phase 1 issue). Phase 1 is split into milestones M1.1 to M1.9: M1.1 Native content model (#100)
  first, then ECS client runtime, desktop mode/locomotion/IK, OpenXR, protocol/instance server/nodes, voice,
  minimal hub and join flow, avatar fidelity (#109, #107, #110, #92, #93, #94, #77), Phase 1 gate (#61, #62, #63,
  #104). Milestones without issues get them filed from their design doc sections by the session that starts them.
- **M1.1 (#100) is designed**: ADRs 0085 to 0090, design doc §9. Done: #113 to #121 (state table ADR 0095, chain
  outputs, platform values ADR 0096, sync encoding ADR 0098, typed curve targets, native curve units and material
  fields ADR 0099, native actions and `unwritten`; format v32: packages in corpus/local need `civbb build` again). #128: build-stage timings and the texture cache (ADR 0097).
- Parity rerun after any change to materials, dynamics or the renderer: `tools/parity.sh --against <last run>
  <out dir>` (~1 min with a warm texture cache). The ADR 0084 table must not regress; Yumeka's rest worst view is 0.1197, 0.0003
  under red. Last full run: `corpus/local/unity-ref/scores/s121` (#121): every report identical to s120.
- Session cost: ADRs 0091 and 0092 (2026-11-26). After about five sessions, compare `python
  tools/session-stats.py --since 2026-11-26` with the baseline in `docs/iteration-baseline.md` ($9.2 and 28 M
  cache-read tokens per closed issue) and record the result there.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` (ADR 0045).

## Blockers and needs-user

- #126 (needs-user, not blocking): register a glTF vendor prefix (`CIVBB_` is provisional until the final name, D9).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-26 (5): #119 typed curve targets (`animator::Target`: node, mesh, humanoid, entry, constraint by index;
  masks by node; format v30). Unity paths resolved by `gltf_import/unity_clip.rs`, unresolvable curves dropped with
  a warning; prune, merge and atlas remap through `renumber::Renumber`; `node_paths` left `civbb-anim`. Next: #120.
- 2026-11-26 (6): #120 native curve units (format v31, ADR 0099): weights 0..1, Euler radians with
  `EulerOrder::Zxy`, muscles `{bone, axis}`, material curves on `MaterialField` with `srgb`; Unity names per shader
  family, smoothness and V-up offsets converted, unanimated vector components baked into materials. Parity s120:
  Cokoro_Quest red states 9 to 7, the rest identical. Next: #121.
- 2026-11-26 (7): #121 native actions (`Action::Entries/PoseSource/LayerWeight/GroupWeight/Locomotion`, `EntryOp`)
  and `State::unwritten: Hold | Rest`, `Mask`; format v32. Dialect 1 lowered by `unity_state::lower_states`, raised
  back by the corpus writer. Goldens and parity s121 unchanged. Next: #122.
