# 0079: Source guardrails in the gate

Status: accepted
Date: 2026-11-24

## Context

Sessions build this project with nobody reviewing their code (CLAUDE.md), so a convention holds only as long as
every session remembers it. The Phase 0 health check (`docs/ai/reviews/phase-0-health-check.md`) found three
conventions kept so far by habit alone: every non-test `unsafe` block had a `// SAFETY:` comment (13 test blocks did
not); no `unwrap()` in library or binary code; and files small enough to read in one go, which two files had
stopped being (`gltf_import/vrm.rs` 1,265 lines, `gltf_import.rs` 1,062), as had one 406-line function
(`validate::V::avatar`). `unsafe` is the Vulkan code's whole risk surface (design doc §6), and a panic on untrusted
input is a crash class (§1).

## Decision

`cargo xtask ci` enforces all three:

1. Clippy `undocumented_unsafe_blocks` in `[workspace.lints.clippy]` (ci denies warnings), tests included.
2. `cargo xtask rules`: no `.rs` file in a workspace crate over 1,000 lines. Past it, split along a real seam (the
   health check split `vrm.rs` into springs, MToon, expressions and meta; `gltf_import.rs` into materials, meshes
   and animations; validation's animator checks into their own file).
3. `cargo xtask rules`: no `unwrap()` in a crate's `src/` before its first `#[cfg(test)]`. Use `?`, or `expect`
   with the invariant that makes it unreachable. Test modules stay last in a file.

No function-length limit: the long functions left (the test-data generator, `unity_material`, the solver's
`advance`) each walk one sequential algorithm, and a cap would cut them at arbitrary lines.

## Consequences

- A new file that grows past 1,000 lines fails the gate in the session that grows it, while its seams are still
  fresh; the limit can rise for a file only by superseding this ADR.
- The `unwrap` rule is textual: code after a file's first `#[cfg(test)]` is not checked, and a string containing
  the call is flagged. Both are cheap to live with; revisit if either bites.
- Refactors to meet the limit are behaviour-preserving and pinned first (the health check's scratch harness over
  the local corpus and mutated inputs).
