# 0094: Sessions navigate Rust through the LSP tool

Status: accepted
Date: 2026-11-26

## Context

Reading code was the largest tool output in the 20 sessions measured in `docs/iteration-baseline.md`: `sed -n`
slices 1.5 MB, `cat` of whole files 0.8 MB, `grep` 0.7 MB. The owner's Claude Code has the `rust-analyzer-lsp`
plugin enabled, but `rust-analyzer` was not installed for the pinned toolchain, so the LSP tool never worked.

## Decision

The `rust-analyzer` component is installed for the pinned toolchain on the reference PC (not in
`rust-toolchain.toml`: CI does not use it, and every workflow would download it). Sessions use the LSP tool for
definitions, references, implementations and call hierarchies in Rust, and `grep` for text.

Measured: references of `Machine::start` (`crates/anim/src/machine.rs:185`) are 2 locations in 123 B through LSP;
`grep -rn "Controller::start\|\.start("` printed 503 B, five unrelated hits, and missed the real caller. The server
starts at the first LSP call and needs about 20 s to load and index this workspace (`rust-analyzer analysis-stats`:
11.6 s load, 7.1 s item collection); calls before that answer "no references".

## Consequences

After a toolchain bump, run `rustup component add rust-analyzer` (noted in `rust-toolchain.toml`). If sessions keep
reading whole files, the gain stays small; `tools/session-stats.py` shows the read volume per session.
