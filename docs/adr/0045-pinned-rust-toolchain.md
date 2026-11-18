# 0045: One pinned Rust toolchain for this machine and CI

Status: accepted
Date: 2026-11-19

## Context

CI installed the newest stable on every run; this machine kept whatever rustup last installed. Rust 1.100.0
reached the runners on 2026-11-09 while this machine had 1.99.1, and its clippy rejected unchanged code in
`crates/pipeline/src/atlas.rs` under `-D warnings`: every push to main was red while the local gate was green
(#76). A gate is only a gate if both sides run the same compiler and lints.

## Decision

- `rust-toolchain.toml` pins an exact release (`1.100.0`) with `clippy` and `rustfmt`. rustup uses it for every
  cargo command in the repo, locally and in CI.
- The `ci` and `docs` workflows install it with `rustup toolchain install` instead of a stable action.
- The fuzz workflow sets `RUSTUP_TOOLCHAIN: nightly` (cargo-fuzz needs nightly; the variable beats the file);
  locally `cargo +nightly ...` does the same (`fuzz/README.md`).
- Bumping is a deliberate commit: change the version, run `cargo fmt --all && cargo xtask ci`, fix what the new
  lints report, commit them together.
- The gate runs before every push through `.githooks/pre-push` (`git config core.hooksPath .githooks` once per
  clone); it is `cargo xtask ci`, the full workspace test suite, not `cargo xtask test`.

## Consequences

A new stable release no longer changes CI by itself. New lints and compiler fixes arrive only when a session bumps
the pin; revisit the pin at the start of each milestone.
