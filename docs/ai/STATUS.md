# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues.

## Now

- Phase 0 (prove the avatar compiler). Current milestone: **M0.4 Animation runtime** (M0.3 done).
- Next issue: #48 humanoid muscle and root motion curves (last open M0.4 issue). Then M0.5 (#18 onwards).
- `civbb-anim` (engine crate) has curve binding, clip sampling, the Mecanim state machine, blend trees, Write Defaults,
  built-ins and behaviours (ADRs 0007 to 0011). Semantics not verifiable without Unity are listed as "unverified"
  in each ADR; the golden cases in `crates/anim/tests/golden/` get replaced by Unity recordings at M0.10.
- Test avatars: `civbb-corpus` (`lean()`, `bloated()`, `gltf::to_glb`), and locally `corpus/local/khronos/` (23 Khronos samples) `corpus/local/vrm/` (AliciaSolid 0.x, Seed-san and the twist sample 1.0) and `corpus/local/glb/` (Booth: HAOLAN, Taro, Tessakuchan, Yumeka, converted from FBX; see `corpus/README.md`). `civbb inspect <file>` prints what an avatar contains.
- Progress for the human: https://github.com/yannikwirbs/civbb/milestones

## Blockers and needs-user

- #37, #38 Unity tasks (M0.10, later). Unity 6.3 is now installed and licensed (CLAUDE.md, Environment), so sessions can run it in batch mode; ADR 0002's "hand-derived until the exporter" can be revisited.

## Decisions in flight

None. ADRs 0007 to 0012 (today) are accepted; their unverified Unity choices wait for recordings (M0.10).

## Session log

One line per session, newest last: date, what moved, what's next.

- 2026-10-28: repo, CI, xtask (ci/arch/codemap), ADRs 0001-0005, labels, 14 milestones, 43 issues. M0.1 and M0.2 done: `format` (avatar model, animator/dynamics/contacts/constraints/cost types, signed package, validation). #8 `civbb-corpus` generator done.
- 2026-10-28 (2): M0.3 done. `civbb-pipeline`: glTF importer with own bounded accessor reader (#9), VRM 0.x/1.0 (#10), `civbb` CLI (#12), fuzz crate + workflow (#43; found and fixed two gltf-crate panics). Format v2 adds `face`. Opened #44 (eye look, first person, licence), #45 (quantized/compressed glTF), #46 (VRM node constraints). Next: M0.4 from #13.
- 2026-10-28 (3): #11 done: four Booth avatars converted FBX to GLB with portable Blender, all import (up to 99k verts, 563 shapes). Their VRChat components sit in .unitypackage files; #47 decides how to read them.
- 2026-10-28 (4): M0.4 nearly done: new crate `civbb-anim` (#13 binding and sampling, #14 state machine, #15 blend trees, #16 Write Defaults and golden harness, #17 built-ins and behaviours), format v3 eye look, first person and licence (#44). ADRs 0007 to 0012. Opened #48 (muscles), #49 (material curves), #50 (weighted tangents), #51 (audio behaviour). Next: #48.
