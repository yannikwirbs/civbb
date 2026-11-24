# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`.

## Now

- **Phase 0's gate is passed** (ADR 0084: CPU half ADR 0082, visual half ADR 0083's thresholds; table of all 13
  Unity exports in ADR 0084, no rest state red, every red state after rest itemised with a Phase 1 issue). M0.11
  and epic #39 are closed.
- **Phase 1 is split into milestones M1.1 to M1.9** (https://github.com/yannikwirbs/civbb/milestones): M1.1 Native
  content model (#100) first, then ECS client runtime, desktop mode/locomotion/IK, OpenXR, protocol/instance
  server/nodes, voice, minimal hub and join flow, avatar fidelity (#109, #107, #110, #92, #93, #94, #77), Phase 1
  gate (#61, #62, #63, #104). Milestones without issues get them filed from their design doc sections by the
  session that starts them.
- Next: M1.1, #100 (the content model redesign). It is a design question first: read #100 and design doc §9, write
  the ADR(s) before code.
- Parity rerun (any change to materials, dynamics or the renderer): per export `civbb build
  corpus/local/unity-glb/<name>.glb --key corpus/local/unity-ref/key -o <pkg>` then `civbb-client parity
  corpus/local/unity-ref/<name> <pkg> --key corpus/local/unity-ref/key --images <dir>` (13 exports about 25 min,
  Yumeka's build about 10). Yumeka's rest worst view is 0.1197, 0.0003 under red: check it after such changes.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` for this machine and CI (ADR 0045); bump it on purpose.
  `.githooks/pre-push` runs `cargo xtask ci` (enable per clone: `git config core.hooksPath .githooks`).

## Blockers and needs-user

- None.

## Decisions in flight

None. ADRs 0017 to 0084 are accepted (0035 supersedes the codec, mip and gutter parts of 0029; 0036 the rig
bounds of 0033; 0046 the capture frame and rebaking of 0040; 0047 refines 0017's shapes and proximity; 0049
supersedes 0015 and the solver points of 0016; 0055 the frame count of 0033; 0062 the HDR conversion and reference
time of 0052; 0065 ADR 0010's WD-on empty states and 0028's `Start` state; 0066 the interruption points of 0008; 0067
the material default of 0025; 0068 the version 1.0 gravity of 0049; 0070 the collider velocity of 0049; 0074 the
partial output step and in-frame interpolation of 0049; 0072 corrects 0064's thumb account; 0077 defers ADR 0023
point 7's GPU culling to Phase 2, #59).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-24 (3): #103 closed (ADR 0080): `measure` pins its worker to the fastest core class (Windows,
  `windows-sys`), reports `cpu.pinned`; scorecard no longer pins. #38 closed (ADR 0081, 0082): Unity baselines in
  Editor play mode on the same core (`Baseline.Measure`, `corpus/unity_baseline.py`), with AvatarOptimizer in a
  host copy; the SDK's PhysBones are managed jobs in Unity; animator 3.0 to 42x cheaper compiled, CPU half of the
  gate passed. #102 closed (not reproduced). #104 filed. Next: #101.
- 2026-11-24 (4): #101. `parity` splits views into interior and silhouette pixels (rest error was interior:
  shading). Fixed from shader sources: Mobile Toon Lit/MatCap Lit unshaded, lilToon outline times `_OutlineTex`
  (format v23), UTS2 shade ramp and second shade, Poiyomi without a ramp unshaded, TSF mapped, Toon Standard
  emission strength. Rest means 0.057-0.194 to 0.027-0.206. ADR 0083: Unity thresholds from the translation floor;
  red rests #105, #106, #108 in M0.11; red states itemised (#107, #109, #110). Next: #105, #106, #108.
- 2026-11-24 (5): M0.11 closed: Phase 0's gate passed (ADR 0084). #105 Toon Standard ramp and shadow; #106 Poiyomi
  matcap replace, emission, second matcap, UV1 slots (format v25); #108 PhysBone limit rotations `qz qy qx`
  (recorded), Yumeka's skirt pushed by its hands (#92). No rest state red (5 green, 8 yellow). Epic #39 split into
  M1.1 to M1.9. Next: M1.1, #100.
