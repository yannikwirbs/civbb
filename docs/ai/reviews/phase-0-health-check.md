# Phase 0 health check

Date: 2026-11-24. Branch `health/phase-0`. Scope: the whole repository against the design doc, ADRs 0001 to 0077
and the Phase 0 milestones, before Phase 1 starts. Results and metrics are at the end; this first part is the
findings and the plan, written before any change.

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
| `format::cost::CostProfile` and the manifest's `cost` field | Early (§8 governor, Phase 1): never written by the compiler, never read; field names guess a cost model nobody designed | Remove, issue for Phase 1 |
| `dynamics` grab and pose | In scope (PhysBones convert one to one, §10) but unverified against the SDK | Keep, noted |
| `Island` IK stub and `System::Ik` | Placeholder for Phase 1 IK (ADR 0020 fixes its slot), two lines | Keep |
| `format::package::ContentKind::World`, `Platform::Quest` | One variant each; Quest caps are used by `compress` | Keep |
| `render/examples/clear.rs` | Manual swapchain smoke test, documented in the crate README | Keep |
| Locomotion extras #70, audio #51, MSAA #60, pipeline cache #61, lights #62, LOD switching #63 | Already Phase 1 issues, no code yet | Nothing to do |

No crate, tool or dependency without a design-doc or ADR basis was found. All 20 direct dependencies are used and
each has a recorded reason.

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
3. **Scope removal**
   1. Remove `CostProfile` and `Header::cost`; issue for the Phase 1 cost model.
4. **Structure**
   1. Math helpers onto `civbb_core::math` (vrm, cull, Mat4 multiply).
   2. Split `validate::V::avatar` into one method per avatar part.
   3. Split `vrm.rs` and `gltf_import.rs` along their seams.
   Behaviour pinned first: a scratch harness records the import result (validation errors, or a hash of the
   avatar's CBOR) for the local corpus and 3,000 mutated inputs, before and after each refactor.
5. **Cosmetics and docs**: the raw newline; dynamics description and README; README, STATUS, ARCHITECTURE.
6. **Guardrails**: `clippy::undocumented_unsafe_blocks` as a workspace lint (after adding the 14 comments);
   `xtask ci` fails on `.unwrap()` in non-test library and binary code and on any `.rs` file over 1,000 lines.
