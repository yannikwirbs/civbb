# civbb-fuzz

cargo-fuzz targets for the parsers of untrusted input (design doc §16): `package_decode`, `avatar_validate`,
`gltf_import` (which also covers VRM), `ktx2_transcode` (the client's KTX2 to BC7 transcoding). Kept out of the main workspace because it needs nightly and libFuzzer.

CI (`.github/workflows/fuzz.yml`) runs each target for a minute on every push to `main` and for longer every night.
Locally:

```sh
rustup toolchain install nightly && cargo install cargo-fuzz
cargo +nightly run --manifest-path fuzz/Cargo.toml --bin seeds
cargo +nightly fuzz run gltf_import
```

On Windows the target exits with `STATUS_DLL_NOT_FOUND` unless the MSVC ASan runtime
(`clang_rt.asan_dynamic-x86_64.dll`) is on `PATH`, for example (Git Bash):

```sh
export PATH="/c/Program Files/Microsoft Visual Studio/2022/Community/VC/Tools/MSVC/14.44.35207/bin/Hostx64/x64:$PATH"
```

A crash leaves its input in `fuzz/artifacts/<target>/`; turn it into a regression test next to the parser.
