# Contributing

## AI usage policy

CIVBB is built mostly by AI coding agents (design doc §21, D0). The rules are the same for AI-written and human-written code:

- It passes `cargo xtask ci` and the conformance, parity and benchmark suites.
- It follows the crate layering in `docs/ARCHITECTURE.md`; tooling enforces it.
- Code at a trust boundary (Wasm sandbox, network protocol, file parsers, upload pipeline) gets human review before any public server runs it. Issues touching these carry the `trust-boundary` label.
- Every decision not in the design doc gets an ADR in `docs/adr/`.

Agent workflow and session rules: [CLAUDE.md](CLAUDE.md).

## Sign-off

Contributions come in under the repository's licences with a DCO sign-off (`git commit -s`), no CLA.

## Assets

Only CC0 or self-made assets are committed. Third-party avatars for local testing go in `corpus/local/`, which is gitignored.
