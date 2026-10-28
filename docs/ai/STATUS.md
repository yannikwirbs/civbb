# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues.

## Now

- Phase 0 (prove the avatar compiler). Current milestone: **M0.5 Dynamics, contacts, constraints** (M0.4 closed).
- Next issue: #20 contact senders and receivers with broadphase, then #21 constraints, then #46 VRM node constraints.
- `civbb-anim`: curve binding, clip sampling, state machines, blend trees, Write Defaults, built-ins, behaviours and
  humanoid muscles (ADRs 0007 to 0011, 0014). `civbb-dynamics`: XPBD PhysBone chains with grab/pose, colliders and
  limits (ADRs 0015, 0016). Format v4 (muscle frames).
- Unity ground truth: `tools/unity-truth` runs in batch mode and records JSON into crate test dirs (ADR 0013,
  `tools/unity-truth/README.md`). Muscle space is verified against it; animator goldens are still hand-derived (#54).
- Test avatars: `civbb-corpus` (`lean()`, `bloated()`, `generate`, `gltf::to_glb`), and locally `corpus/local/khronos/` (23 Khronos samples) `corpus/local/vrm/` (AliciaSolid 0.x, Seed-san and the twist sample 1.0) and `corpus/local/glb/` (Booth: HAOLAN, Taro, Tessakuchan, Yumeka, converted from FBX; see `corpus/README.md`). `civbb inspect <file>` prints what an avatar contains.
- Progress for the human: https://github.com/yannikwirbs/civbb/milestones

## Blockers and needs-user

- #37, #38 Unity tasks (M0.10, later). Sessions can run Unity themselves now; those two still need the human
  (Creator Companion projects, baselines).

## Decisions in flight

None. ADRs 0013 to 0016 (today) are accepted. Dynamics constants are unverified until parity (M0.9); #52 (body
pose), #53 (muscle frames for glTF/VRM) and #54 (Unity-recorded animator goldens) sit in M0.10.

## Session log

One line per session, newest last: date, what moved, what's next.

- 2026-10-28: repo, CI, xtask (ci/arch/codemap), ADRs 0001-0005, labels, 14 milestones, 43 issues. M0.1 and M0.2 done: `format` (avatar model, animator/dynamics/contacts/constraints/cost types, signed package, validation). #8 `civbb-corpus` generator done.
- 2026-10-28 (2): M0.3 done. `civbb-pipeline`: glTF importer with own bounded accessor reader (#9), VRM 0.x/1.0 (#10), `civbb` CLI (#12), fuzz crate + workflow (#43; found and fixed two gltf-crate panics). Format v2 adds `face`. Opened #44 (eye look, first person, licence), #45 (quantized/compressed glTF), #46 (VRM node constraints). Next: M0.4 from #13.
- 2026-10-28 (3): #11 done: four Booth avatars converted FBX to GLB with portable Blender, all import (up to 99k verts, 563 shapes). Their VRChat components sit in .unitypackage files; #47 decides how to read them.
- 2026-10-28 (4): M0.4 nearly done: new crate `civbb-anim` (#13 binding and sampling, #14 state machine, #15 blend trees, #16 Write Defaults and golden harness, #17 built-ins and behaviours), format v3 eye look, first person and licence (#44). ADRs 0007 to 0012. Opened #48 (muscles), #49 (material curves), #50 (weighted tangents), #51 (audio behaviour). Next: #48.
- 2026-10-28 (5): #48 muscles verified against Unity recordings (new `tools/unity-truth`, ADRs 0013, 0014, format v4), M0.4 closed. M0.5: new crate `civbb-dynamics`, #18 chain solver (ADR 0015), #19 colliders and limits (ADR 0016). Opened #52, #53, #54. Next: #20.
