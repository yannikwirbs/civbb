# civbb-fuzz

cargo-fuzz targets for the parsers of untrusted input (design doc §16): `package_decode`, `avatar_validate`,
`gltf_import` (which also covers VRM). Kept out of the main workspace because it needs nightly and libFuzzer.

CI (`.github/workflows/fuzz.yml`) runs each target for a minute on every push to `main` and for longer every night.
Locally (Linux or macOS; Windows needs MSVC ASan):

```sh
rustup toolchain install nightly && cargo install cargo-fuzz
cargo +nightly run --manifest-path fuzz/Cargo.toml --bin seeds
cargo +nightly fuzz run gltf_import
```

A crash leaves its input in `fuzz/artifacts/<target>/`; turn it into a regression test next to the parser.
