# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues, what
exists in `STATE.md`, the full session log in `SESSIONS.md`.

## Now

- **Phase 0 health check done** (merged from `health/phase-0`; report `docs/ai/reviews/phase-0-health-check.md`,
  "Result" at the end). Guardrails now in the gate (ADR 0079, CLAUDE.md).
- **Phase 0's gate is half passed.** CPU half passed (ADR 0082). Visual half: thresholds for a Unity reference
  decided (ADR 0083: green mean <= 0.06 and p99 <= 0.25, red mean > 0.12 or p99 > 0.9, from the translation floor);
  #101's mapping fixes took rest means to 0.027 to 0.206, five exports green. #101 closes when no rest state is red:
  **#105** (Cokoro_Quest, VRChat Toon Standard's ramp), **#106** (KohzieBoi, Poiyomi's second matcap and UV1 masks)
  and **#108** (Yumeka, silhouette offsets, likely PhysBone rest) are next, in M0.11. Then the ADR closing the gate,
  M0.11 closed, epic #39 split. Red states after rest are itemised in ADR 0083 with issues in Phase 1 (#107
  Blackbird's wings, #109 pose states, #110 Cokoro's material toggles). #104 (Av3Emulator check, cannot change ADR
  0082) when time allows. Evidence: per export `civbb build <glb> --key corpus/local/unity-ref/key -o <pkg>` then
  `civbb-client parity corpus/local/unity-ref/<name> <pkg> --key ... --images <dir>` (13 exports about 15 min,
  Yumeka's build alone about 10); `python corpus/scorecard.py` (about 45 min) for the full table with CPU and GPU.
  Phase 1 leftovers already filed: #60 MSAA and multiview, #61 pipeline cache, #62 clustered lights, #63 LOD
  switching, #70 locomotion extras, #77, #92, #93, #94; #51 (audio behaviour) waits for an audio crate.
- The local branch `wip/52-body-pose` is merged into main (542bea6) and can be deleted.
- Toolchain: Rust 1.100.0, pinned by `rust-toolchain.toml` for this machine and CI (ADR 0045); bump it on purpose.
  `.githooks/pre-push` runs `cargo xtask ci` (enable per clone: `git config core.hooksPath .githooks`).
- Progress for the human: https://github.com/yannikwirbs/civbb/milestones

## Blockers and needs-user

- None. #38's Unity baselines: sessions can likely run the AV3 emulator and AvatarOptimizer themselves (MIT
  packages, local installs only).

## Decisions in flight

None. ADRs 0017 to 0083 are accepted (0035 supersedes the codec, mip and gutter parts of 0029; 0036 the rig
bounds of 0033; 0046 the capture frame and rebaking of 0040; 0047 refines 0017's shapes and proximity; 0049
supersedes 0015 and the solver points of 0016; 0055 the frame count of 0033; 0062 the HDR conversion and reference
time of 0052; 0065 ADR 0010's WD-on empty states and 0028's `Start` state; 0066 the interruption points of 0008; 0067
the material default of 0025; 0068 the version 1.0 gravity of 0049; 0070 the collider velocity of 0049; 0074 the
partial output step and in-frame interpolation of 0049; 0072 corrects 0064's thumb account; 0077 defers ADR 0023
point 7's GPU culling to Phase 2, #59).

## Last sessions

The newest three; `SESSIONS.md` has them all.

- 2026-11-24 (2): Phase 0 health check finished and merged. Scorecard over all 13 Unity exports (#101): none green
  against Unity; GPU drops with compiling, CPU does not; one-worker timings were P/E-core noise (pinned now, #103);
  Blackbird slower compiled (#102). Pinned refactors (19,252-case harness, identical): `validate` per avatar part,
  `vrm.rs` and `gltf_import.rs` split, math onto `civbb_core::math`. Collider shapes now validated finite.
  Guardrails in the gate (ADR 0079). Next: M0.11, #103 then #38.
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
