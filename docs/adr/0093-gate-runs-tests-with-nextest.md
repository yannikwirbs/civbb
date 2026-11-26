# 0093: The gate runs tests with cargo-nextest when it is installed

Status: accepted
Date: 2026-11-26

## Context

`cargo xtask ci` ran `cargo test --workspace`, which runs the 53 test binaries one after another. Sessions ran the
gate 142 times in the 20 sessions measured in `docs/iteration-baseline.md` (79 minutes). Warm, the test step took
18.7 s of a 19 to 20 s gate; the slowest single test takes 7 s.

## Decision

`cargo xtask ci` and `cargo xtask test` run tests with `cargo nextest run` (binaries in parallel, `--no-fail-fast`,
only failures printed) plus `cargo test --doc` (nextest does not run doctests) when `cargo nextest` is installed,
and fall back to `cargo test` otherwise. GitHub CI installs it with `taiki-e/install-action`. Install locally with
`cargo install cargo-nextest --locked`.

Measured warm on the reference PC: test step 18.7 s -> 9.5 s (+1.4 s doctests); whole gate 19 to 20 s -> 12 s.
The same 287 tests pass and 2 are ignored; a failing test prints 806 B (644 B with `cargo test`).

## Consequences

Each test runs in its own process, so tests cannot share process state; none does today. A machine without nextest
still gets the full gate, only slower.
