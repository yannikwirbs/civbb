# CIVBB

*Chatting in VR but Better* (codename). An open-source social VR platform on a custom Rust + Vulkan engine where every per-avatar system runs as parallel jobs, and existing avatar projects are compiled into an optimised native format instead of emulated.

Status: **Phase 0, pre-alpha.** Nothing to play yet. Phase 0 proves the core idea: popular, heavy avatars can be compiled into a much cheaper form that looks and behaves the same.

- Progress: [milestones](https://github.com/yannikwirbs/civbb/milestones) · [current status](docs/ai/STATUS.md)
- Spec: [design doc](docs/design-doc.md) · [architecture](docs/ARCHITECTURE.md) · [decisions (ADRs)](docs/adr/)
- How it's built: [CONTRIBUTING.md](CONTRIBUTING.md) (this project is written almost entirely by AI agents)

## Build

Needs Rust stable (`rustup`). On Windows, the MSVC build tools.

```sh
cargo xtask ci      # the full gate: fmt, clippy, tests, architecture rules, code map
cargo test          # tests only
```

## Licence

Engine, client, SDKs and formats: MIT OR Apache-2.0 ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)). The node (instance server, `crates/node`): AGPL-3.0 ([LICENSE-AGPL-3.0](LICENSE-AGPL-3.0)).

Not affiliated with VRChat Inc. Migration works only from source projects creators own; built bundles are never accepted.
