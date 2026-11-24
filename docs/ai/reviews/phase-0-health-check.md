# Phase 0 health check

Date: 2026-11-24 (two sessions). Branch `health/phase-0`, merged into `main`. Scope: the whole repository against
the design doc, ADRs 0001 to 0077 and the Phase 0 milestones, before Phase 1 starts. Results and metrics are at
the end ("Result"); the first part is the findings and the plan, written before any change.

## Baseline (measured)

- `cargo xtask ci`: green, 19 s warm, 273 tests passed, 1 ignored. CI on `main` green (ci, docs, fuzz).
- 36,307 lines of Rust. By crate: pipeline 9,923; anim 8,216; render 5,533; format 3,923; client 3,389; dynamics
  3,002; corpus 1,135; core 548; xtask 508; fuzz 130.
- Largest files: `pipeline/src/gltf_import/vrm.rs` 1,265; `pipeline/src/gltf_import.rs` 1,062;
  `format/src/validate.rs` 981; `pipeline/tests/gltf_import.rs` 896; `client/src/view.rs` 860; `corpus/src/lib.rs`
  834; `dynamics/src/solver.rs` 753.
- Longest non-test functions: `format::validate::V::avatar` 406; `corpus::generate` 389 (test data generator);
  `pipeline::unity_material::unity_material` 200; `dynamics::solver::advance` 177; `pipeline::atlas::atlas` 170;
  `pipeline::merge::merge` 167; `pipeline::atlas::eligible` 163; `vrm::expression_parts` 152;
  `render::material::pack` 148; `pipeline::report::markdown` 141.
- Dependency graph (normal deps): format → none; core → format; anim, dynamics, render → core, format; pipeline →
  anim, core, format; client → anim, core, corpus, dynamics, format, render. Matches ARCHITECTURE.md; `xtask arch`
  enforces it. 20 direct external dependencies, 288 packages in `Cargo.lock`.
- `cargo machete`, `cargo deny`, `cargo audit`: not installed (not added). `clippy::pedantic` on lib and bin
  targets: mostly casts (`usize as u32` 105, `u32 as f32` 48), `must_use` (141) and doc sections; nothing that
  points at a bug on its own.
- `unsafe`: 101 sites, all in `render` and `client::view`/`parity` (Vulkan); none in the pipeline or any parser.
  Every `unsafe` block in non-test code carries a
  `// SAFETY:` comment; 13 blocks in `render/tests` do not, nor does the `unsafe extern fn on_message` callback.
- `unwrap()`: none in library or binary code. 64 `expect`/`unreachable!` in non-test code, each stating an
  invariant (validated indices, packed bins, lock poisoning).

## 1. Goal alignment: is Phase 0's core claim proven?

The claim (design doc §21, ADR 0004 gate): *compiled avatars stay visually identical (FLIP within threshold in
every state) and cost clearly less CPU per avatar than the baselines (the same avatar in Unity with an Avatars 3.0
emulator, and AvatarOptimizer / d4rk's optimiser)*.

**Verdict: half proven.** M0.10 was closed and STATUS says Phase 0 is done, but the gate was never passed.

What is demonstrated, with tests or recorded measurements:

- *Behaves the same* (strong). Animator goldens recorded in Unity (ADR 0065), muscle space, body pose and root
  motion (516 cases exact), Unity and VRChat constraints (0.0001 to 0.0002 degrees), contacts (39 SDK cases exact),
  PhysBones (84 of 88 cases within 0.5 mm at 60 Hz, all 18 at 90 Hz within 0.2 mm). All replayed by committed
  tests in CI.
- *The compiler's stages don't change appearance or behaviour* (strong). Each stage has before/after island-step
  equivalence tests; FLIP of the compiled package against its own import is green for lean and bloated in CI
  (`pipeline/tests/parity.rs`) and for 13 of 16 textured local files.
- *Our runtime is fast and scales* (measured, not gated). 100 bloated avatars 0.43 ms on 16 threads; corpus
  avatars 2 to 30 µs CPU per avatar on one worker (#34).

What is asserted but not demonstrated:

- *Cheaper than the baselines*: never measured. #38 (Unity AV3 emulator and optimiser baselines) was moved to
  Phase 1 when M0.10 closed. No Unity CPU number exists anywhere in the repo or the issues.
- *Visually identical to the source*: against Unity's own renders (ADR 0048, the real reference), rest means run
  0.06 to 0.23 and worst states 0.12 to 0.37 (STATE.md). ADR 0033's lights call a mean above 0.1 red and only
  ≤ 0.02 green: **no export is green against Unity, most are red.** The remaining errors (silhouettes, Blackbird's
  wing pose at 0.35) have no open issue.
- *The before/after cost of the full exports*: `civbb build --measure` times only the compiled package, and the
  #34 table measured Booth FBX conversions that have no animator or PhysBones. The 13 Unity exports (animators,
  PhysBones, contacts, constraints) were never measured before against after.
- *Reproducibility*: the Unity-parity scoring scripts (`rescore.sh`, `rerender.sh`, `summ.py`) live only in the
  gitignored `corpus/local/unity-ref/`, so the evidence for the claim cannot be regenerated from the repository.

Milestones marked done but only partly done:

- M0.9 *Reports and parity*: the report has no Unity parity (`unity_parity`, #98, moved to Phase 3) and no
  before-compile timings.
- M0.10 *Unity exporter*: deliverable 1 of §21 ("including the non-destructive build step and the reference
  renders") is two-thirds deferred (#97, #98 to Phase 3, ADR 0075). The deferral is reasoned (no local package uses
  NDMF/MA/VRCFury) and recorded; it is not hidden.
- §21 deliverable 3 lists PhysBone merging: dropped by ADR 0030 with a measurement. Fine.
- Milestones M0.1, M0.2, M0.3 and M0.8 are still open on GitHub with zero open issues.
- Epic #39 says "split into milestones when Phase 0 passes its gate". NEXT_PROMPT.md tells the next session to
  split it now.

Does the architecture support Phase 1? Yes, with one known substitution: ADR 0003 runs islands on rayon over plain
data and defers `bevy_ecs` and the scheduler to the client; Phase 1 adds them around `Model`/`Island`, which are
plain per-avatar data and port as components. The `IK` stub keeps its slot in `Island::step` (ADR 0020). Nothing
in Phase 0 blocks OpenXR, networking or voice; `node` and `protocol` don't exist yet, as ARCHITECTURE.md says they
shouldn't until their first issue.

## 2. Correctness and safety

- **Compiler stages are not fuzzed.** The four fuzz targets cover package decoding, CBOR plus validation, the glTF
  importer and KTX2 transcoding. The stages that run on validated untrusted input afterwards (prune, atlas, merge,
  lod, collapse; compress is C++ Basis Universal on decoded images) are reached by no fuzzer. A throwaway probe
  (digit mutations of the lean and bloated source GLBs, import then every stage but compress) ran 2,000 cases, 802
  of them through the stages: no panic. Still, a crash there takes down an upload worker (§1 metric: zero crash
  classes), and the stages hold 9 `expect`s that rely on validation.
- `unsafe`: correct `SAFETY:` comments in all non-test code (sampled `memory.rs`, `context.rs`, `renderer.rs`); 13
  test blocks and one callback lack them. Nothing enforces the convention.
- No `unwrap()` outside tests, by convention only.

## 3. Scope creep

| Item | Class | Action |
| --- | --- | --- |
| Crates `format`, `core`, `anim`, `dynamics`, `render`, `client`, `pipeline`; `tools/corpus`, `xtask`, `unity-exporter`, `unity-truth`; `fuzz` | In scope (§21 deliverables 1 to 5, ADRs 0013, 0038) | Keep |
| `client` viewer, `bench`, `measure`, `parity`, `rig` | In scope (deliverables 4, 5) | Keep |
| `client::cull` (ADR 0077) | Early (Phase 1 performance) but small, isolated, tested | Keep |
| `format::cost::CostProfile` and the manifest's `cost` field | Early (§8 governor, Phase 1): never written, never read. ADR 0032 keeps it empty on purpose until the governor (#63) exists; 60 lines, isolated | Keep (ADR-backed); #63 defines it |
| `dynamics` grab and pose | In scope (PhysBones convert one to one, §10) but unverified against the SDK | Keep, noted |
| `Island` IK stub and `System::Ik` | Placeholder for Phase 1 IK (ADR 0020 fixes its slot), two lines | Keep |
| `format::package::ContentKind::World`, `Platform::Quest` | One variant each; Quest caps are used by `compress` | Keep |
| `render/examples/clear.rs` | Manual swapchain smoke test, documented in the crate README | Keep |
| Locomotion extras #70, audio #51, MSAA #60, pipeline cache #61, lights #62, LOD switching #63 | Already Phase 1 issues, no code yet | Nothing to do |

No crate, tool or dependency without a design-doc or ADR basis was found. All 20 direct dependencies are used and
each has a recorded reason. In the small: the workspace defines no traits at all, has no `dead_code` allows and no
configuration options without a caller; the only placeholders (the IK stub, `CostProfile`) are fixed by ADRs 0020
and 0032. Nothing is removed.

## 4. Code quality

- **Architecture**: layering matches ARCHITECTURE.md and is enforced. Fine.
- **Size**: `vrm.rs` (1,265) and `gltf_import.rs` (1,062) exceed 1,000 lines; both have clear seams (VRM springs,
  MToon, expressions, meta; glTF meshes and animations). `validate::V::avatar` is one 406-line function over ten
  independent parts of the avatar.
- **DRY**: `gltf_import/vrm.rs` carries its own vector and quaternion library (`add`, `dot`, `cross`, `qmul`,
  `conj`, `rotate`, `from_to`) duplicating `civbb_core::math`, which `pipeline` already depends on;
  `client::cull` duplicates `sub` and `length`; column-major `Mat4` multiply is copied in `render::forward` and
  `client::view`. (`tools/corpus` repeats a few too, but it depends on `format` only; left alone.)
- **Readability**: `pipeline/src/main.rs` writes the report with a string literal holding a raw newline (the
  heredoc gotcha in LESSONS.md).
- **Error handling**: consistent: `thiserror` in libraries, `String` errors in binaries and stages; no swallowed
  errors found in the import path.
- **Concurrency and hot paths**: `jobs::islands` is lock-free per island; `Island::step` reuses its buffers;
  `texture` encodes under a capped pool with a `Mutex` only around results. Fine.
- **Tests**: meaningful (goldens from Unity, mutation-checked per LESSONS.md). Gap: compile stages under hostile
  input (above).
- **Docs vs code**: `civbb-dynamics`' Cargo description says "XPBD chain solver" and its README points at ADR 0015
  for the model, both superseded by ADR 0049; README says "Status: Phase 0"; STATUS.md and NEXT_PROMPT.md treat
  Phase 0 as passed.
- **Guardrails**: `xtask ci` enforces fmt, clippy `-D warnings` (so `missing_docs` too), tests, layering, crate
  description and README, module docs, code-map and shader freshness. It does not enforce `SAFETY:` comments, the
  absence of `unwrap()`, or any file size.

## Plan, by priority

1. **Goal alignment**
   1. Commit `corpus/scorecard.py`: per Unity export, the import without stages against the compiled package (draws,
      shapes, CPU and GPU µs per avatar from `civbb-client measure`) and the compiled package's FLIP against
      Unity's reference renders. Run it and put the table in this report.
   2. ADR 0078: Phase 0's gate is not passed; the two missing proofs (Unity CPU baseline, Unity-reference parity)
      become milestone *M0.11 Phase 0 gate*, worked before epic #39 is split.
   3. Issues: Unity-reference parity to within threshold (with the thresholds for a Unity reference as a decision
      in it); #38 moved to M0.11 with a concrete plan. Close the empty M0.1, M0.2, M0.3, M0.8.
2. **Correctness and safety**
   1. Fuzz target `compile`: import, then prune, atlas, merge, lod and collapse; seeded, in the CI matrix.
3. **Scope removal**: nothing qualifies (section 3).
4. **Structure**
   1. Math helpers onto `civbb_core::math` (vrm, cull, Mat4 multiply).
   2. Split `validate::V::avatar` into one method per avatar part.
   3. Split `vrm.rs` and `gltf_import.rs` along their seams.
   Behaviour pinned first: a scratch harness records the import result (validation errors, or a hash of the
   avatar's CBOR) for the local corpus and 3,000 mutated inputs, before and after each refactor.
5. **Cosmetics and docs**: the raw newline; dynamics description and README; README, STATUS, ARCHITECTURE.
6. **Guardrails**: `clippy::undocumented_unsafe_blocks` as a workspace lint (after adding the 14 comments);
   `xtask ci` fails on `.unwrap()` in non-test library and binary code and on any `.rs` file over 1,000 lines.

## Progress (session 1, stopped early at the owner's request)

Done on `health/phase-0` (each commit gated by `cargo xtask ci`):

- `4301fdd`, `c5a8e41`: three bugs in `lod` reachable from hostile input, found by a mutation probe and the new
  fuzz target. (1) A level's target could exceed its input (the level above overshot): meshoptimizer's C++
  assertion aborted debug builds, and release builds broke its precondition. (2) An all-degenerate mesh repeated
  its empty level. (3) A few vertices near 2e37 made the error infinite. (2) and (3) produced packages that
  validation, and so every client, rejects. Regression tests in `pipeline/tests/lod.rs`.
- `848ff7d`: fuzz target `compile` (import, then prune, atlas, merge, lod, collapse; the result must validate), in
  the CI matrix. Locally 78,362 runs clean after the fixes. The digit-mutation probe: 10,000 cases, 4,029 through
  the stages, no failure left.
- `ce31bda`: `civbb-client parity` could not score a *compiled* package against Unity at all: collapsing adds
  `civbb/One`, and the states came from the test avatar. Every Unity number so far was for the uncompiled import.
  Parity now skips `COMPILER_PARAMETER_PREFIX` parameters.
- ADR 0078, milestone M0.11 (#38 with a plan, new #101), epic #39 commented; empty milestones M0.1, M0.2, M0.3 and
  M0.8 closed.
- `corpus/scorecard.py`: before/after CPU and GPU per export and compiled-vs-Unity FLIP. First run, before the
  parity fix (it stopped at Cokoro):

| export | draws | shapes | CPU µs/avatar | GPU µs/avatar | Unity FLIP rest | Unity FLIP worst |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Tessakuchan | 1 → 1 | 114 → 23 | 16.5 → 16.4 | 1.7 → 1.7 | 0.109 | 0.320 |

  Tessakuchan's CPU cost barely moves with compiling: the cost of an avatar is mostly its animator and PhysBones,
  which the stages keep. The answer to "cheaper" therefore rests on the Unity comparison (#38).

## Progress (session 2)

Done on `health/phase-0`, one logical change per commit, each gated by `cargo xtask ci`:

- `0507e40`: the scorecard ran in full (40 minutes, nothing else compiling). Its CPU column turned out to be noise:
  one `measure` worker lands on a performance or an efficiency core at random on the hybrid i5-12600K (Yumeka's
  import 196 µs pinned to a P-core, 364 µs on an E-core; unpinned runs 185, 316, 358). The script now pins
  `measure` to logical CPU 2; pinned runs repeat within about 5 %. The same holds for `civbb-client measure` itself
  and every `--measure` report: #103.
- `f6f92cc`: cosmetics (the raw newline; civbb-dynamics' description).
- `6b3c1f1`, `6786981`: `validate::V::avatar` (406 lines) became one method per avatar part, called in the old
  order; the animator checks moved to `validate/animator.rs` when the file passed 1,000 lines.
- `76ce31a`, `4f4d0ad`: `mat4_mul` in `civbb_core::math` replaces the copies in `render::forward` and
  `client::view`; `client::cull` and `vrm.rs` take `sub`, `length`, `add`, `dot`, `cross`, `conj` from core. vrm's
  `qmul`, `rotate` and `from_to` stay (commented): core's round differently (439 of the pin's results changed in the
  last bit) and core's `from_to` maps a non-finite input to the identity where vrm's yields NaN and validation
  rejects the collider.
- `33df448`, `bc0f88a`: `vrm.rs` split into `vrm/{springs,mtoon,expressions,meta}.rs`, `gltf_import.rs` into
  `gltf_import/{material,mesh,animation}.rs`, code moved unchanged.
- `7a69245`: found while pinning: validation checked contact volumes' sizes but not PhysBone collider shapes, so a
  hostile package could hand the solver a NaN or infinite radius or height. Now rejected (test added).
- `d4a870d`, `00c5eda`: guardrails (below), ADR 0079.

Behaviour pin, a scratch harness kept out of the repository: the import result (CBOR hash of avatar and blobs,
warnings, validation) of all 52 files in `corpus/local/{vrm,glb,unity-glb,khronos}`; 3,200 digit-mutated JSON
chunks of the lean and bloated source GLBs and the four VRMs (half aimed at the extension JSON); 4,000
byte-mutated and 12,000 structurally mutated avatars (indices to edge values, floats to huge or infinite, arrays
cut, emptied or grown, strings too long) through validate: 19,252 results. Every refactor commit was identical to
the baseline built from a worktree at the session's start, the collider fix differed in exactly the 4 mutated
avatars it now rejects. Mutation check: a wrong argument to the new skin check changed 6,318 results. Limit: no
case has errors in two avatar parts at once, so the pin cannot see a reordering of parts; that order was checked
in the diff (moved code shown as moved, only plumbing changed).

## Scorecard: Phase 0's claim against Unity

All 13 local Unity exports (`python corpus/scorecard.py`; CPU and GPU from re-measuring the same packages pinned
to a performance core, mean of two runs; FLIP of the compiled package against Unity's reference renders; every
reference current). Posted on #101.

| export | draws | shapes | CPU µs/avatar | GPU µs/avatar | Unity FLIP rest | Unity FLIP worst |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Blackbird | 4 → 4 | 6 → 5 | 22.1 → 29.4 | 3.6 → 3.7 | 0.108 | 0.348 |
| Blackbird_Quest | 3 → 3 | 6 → 5 | 7.2 → 7.2 | 1.7 → 1.7 | 0.086 | 0.164 |
| Cokoro | 13 → 13 | 313 → 227 | 43.6 → 43.4 | 16.1 → 14.3 | 0.132 | 0.472 |
| Cokoro_Quest | 2 → 2 | 311 → 236 | 40.5 → 38.6 | 9.0 → 8.8 | 0.194 | 0.487 |
| HAOLAN_PC | 15 → 14 | 127 → 94 | 88.8 → 80.4 | 43.7 → 36.5 | 0.057 | 0.076 |
| HAOLAN_Quest | 13 → 10 | 114 → 86 | 34.6 → 35.1 | 12.6 → 6.6 | 0.150 | 0.179 |
| Inuinu | 1 → 1 | 53 → 25 | 26.2 → 26.3 | 2.6 → 2.6 | 0.119 | 0.122 |
| KohzieBoi | 1 → 1 | 4 → 1 | 4.7 → 4.8 | 4.2 → 4.2 | 0.140 | 0.140 |
| Megumi | 23 → 16 | 1 → 0 | 5.8 → 3.8 | 15.9 → 10.8 | 0.160 | 0.160 |
| Ostgaroa | 6 → 6 | 2 → 2 | 42.5 → 43.1 | 24.1 → 24.9 | 0.099 | 0.099 |
| Taro | 9 → 9 | 43 → 27 | 24.9 → 23.9 | 10.8 → 9.5 | 0.165 | 0.166 |
| Tessakuchan | 1 → 1 | 114 → 23 | 16.8 → 16.9 | 1.7 → 1.7 | 0.109 | 0.320 |
| Yumeka | 23 → 19 | 563 → 282 | 190.8 → 186.4 | 59.9 → 33.6 | 0.146 | 0.431 |

- **Visually identical to Unity: not shown.** No export reaches ADR 0033's green (mean ≤ 0.02), at rest or in any
  state; rest means 0.057 to 0.194, worst states up to 0.487. #101.
- **Cheaper:** compiling cuts GPU cost where it removes draws or shapes (Yumeka 44 %, HAOLAN_Quest 48 %, Megumi
  32 %) and leaves CPU cost where it was, because an avatar's CPU cost is its animator and PhysBones, which the
  stages keep. Blackbird's compiled package costs 33 % more CPU (#102). Whether our runtime is cheaper than Unity's
  is the #38 baseline, still unmeasured.

## Result

**Goal alignment: Phase 0's claim is half proven, unchanged.** Behaviour parity with Unity and the VRChat SDK is
strong and replayed in CI; visual parity with Unity and lower cost than the Unity baselines are not shown. Both are
milestone M0.11 (ADR 0078): #38 (Unity CPU baselines), #101 (parity within a threshold), and from this session #102
(Blackbird) and #103 (pinned measuring). Phase 1 should not start before M0.11 closes; epic #39 stays unsplit.
The architecture supports Phase 1 (section 1).

**Scope:** nothing out of scope found or removed (table in section 3).

**Fixed** (sessions 1 and 2): three `lod` bugs on hostile input; compiled packages could not be scored against
Unity; PhysBone collider shapes unvalidated; scorecard CPU numbers were core-class noise; the compile stages are
fuzzed (`compile` target in CI).

**Deferred, with issues:** #101 Unity-reference parity, #38 Unity CPU baselines, #102 Blackbird's CPU after
compiling, #103 pinned `measure`; Blackbird's 0.35 wing pose and the silhouette errors belong to #101. vrm's own
`qmul`/`rotate`/`from_to` stay by decision (above), no issue.

**ADRs:** 0078 (Phase 0 gate not passed, M0.11), 0079 (source guardrails in the gate).

**Guardrails** (CLAUDE.md, ADR 0079): clippy `undocumented_unsafe_blocks`; `cargo xtask rules` in ci: no Rust file
over 1,000 lines, no `unwrap()` in `src/` before the first `#[cfg(test)]`. Each was mutation-checked (a missing
SAFETY comment and an `unwrap()` fail the gate).

| metric | before (baseline) | after |
| --- | --- | --- |
| `cargo xtask ci` | green, 19 s warm, 273 passed, 1 ignored | green, 21 s warm, 276 passed, 1 ignored; adds `rules` and the unsafe lint |
| CI on GitHub | ci, docs, fuzz (4 targets) | ci, docs, fuzz (5 targets: + `compile`) |
| Lines of Rust | 36,307 | 36,617 |
| Files over 1,000 lines | 2 (`vrm.rs` 1,265, `gltf_import.rs` 1,062) | 0 (largest: `format/src/validate.rs` 900, `pipeline/tests/gltf_import.rs` 896, `client/src/view.rs` 852) |
| Longest non-test function | `validate::V::avatar` 406 | `corpus::generate` 389 (test data); then `unity_material` 200, `solver::advance` 177; longest in validation 70 (`animator`) |
| Duplicated math | vrm (7 functions), cull (2), `Mat4` multiply (2 copies) | vrm's `qmul`/`rotate`/`from_to` by decision |
| Unsafe blocks without `SAFETY:` | 13 (tests) | 0, enforced |
| `unwrap()` outside tests | 0 by convention | 0, enforced |
| Direct external dependencies / `Cargo.lock` packages | 20 / 288 | 20 / 288 |

## Update (M0.11, 2026-11-24)

`measure` pins itself to a performance core (ADR 0080, #103). Unity CPU baselines measured (ADR 0081, #38): the
compiled avatars' animator is 3.0 to 42 times cheaper than Unity's with AvatarOptimizer at 40 copies; the SDK's
PhysBones are managed code in Unity and are not compared. Table and verdict in ADR 0082: the CPU half of the gate
passes; the visual half (#101) is still open. #102 did not reproduce and is closed.

## Update (#101, 2026-11-24)

The visual half's causes, itemised from the parity images (ADR 0083). `parity` now splits each view into interior
and silhouette pixels; at rest the interior carried nearly all the error, so the gaps were shading. Fixed from each
shader's source: VRChat Mobile Toon Lit and MatCap Lit unshaded; lilToon's outline colour times `_OutlineTex`
(format v23); UTS2's shade ramp and second shade; Poiyomi without a ramp texture unshaded; TSF as an unlit matcap;
Toon Standard's emission strength. Rest mean against Unity, compiled packages:

| export | before | after | light (ADR 0083) |
| --- | ---: | ---: | --- |
| Inuinu | 0.119 | 0.027 | green |
| Blackbird | 0.108 | 0.040 | green |
| HAOLAN_Quest | 0.150 | 0.046 | green |
| Blackbird_Quest | 0.086 | 0.052 | green |
| Megumi | 0.160 | 0.054 | green |
| HAOLAN_PC | 0.057 | 0.057 | yellow |
| Taro | 0.165 | 0.059 | yellow |
| Cokoro | 0.132 | 0.070 | yellow |
| Tessakuchan | 0.109 | 0.075 | yellow |
| Ostgaroa | 0.099 | 0.099 | yellow |
| Yumeka | 0.146 | 0.108 | red (#108) |
| KohzieBoi | 0.140 | 0.140 | red (#106) |
| Cokoro_Quest | 0.194 | 0.206 | red (#105) |

Thresholds for a Unity reference (ADR 0083): green mean ≤ 0.06 and p99 ≤ 0.25 (the floor of the five exports left
with only texture filtering and edge pixels), red mean > 0.12 or p99 > 0.9. Compiling costs 0.001 to 0.003 against
Unity. Red states after rest are itemised in ADR 0083 (poses #109, Blackbird's wings #107, Cokoro's material
toggles #110). The visual half stays open on #105, #106 and #108 (M0.11).
