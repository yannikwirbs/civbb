# Lessons: parsers and fuzzing

Read before working on parsers of untrusted input, fuzz targets or native (`-sys`) dependencies. Add a line whenever something costs more than one attempt.

- The `gltf` crate (1.4) is not safe on hostile input by itself: its accessor reader uses unchecked arithmetic, `image.source()` unwraps unvalidated fields, GLB parsing underflows on short header lengths, and gltf-json's validation indexes POSITION accessors before checking them. Use `gltf_import::parse` and `accessor.rs`, never `gltf::import` or `Reader`.
- Fuzz in CI: the prebuilt cargo-fuzz (taiki-e/install-action) is a musl binary and defaults to the musl target, which ASan rejects. Pass `--target x86_64-unknown-linux-gnu`.
- Test that hostile-input cases fail for the intended reason (assert the error text); a missing required field can make every case fail early and pass vacuously.
- Pure-Rust parsers still need hostile-input tests: `basisu` 0.1 allocates a KTX2 level's stated uncompressed
  length unchecked (a 1 EB abort from three flipped bits). It also adds the header's `sgdByteOffset` unchecked (an
  overflow panic the CI fuzzer found): check every header range (DFD, KVD, SGD, levels) before calling it, and that the KVD offset and length are 4-byte aligned (its padded cursor underflows otherwise, #67). A test that truncates and bit-flips a small valid file
  a few thousand times finds such things in seconds, on Windows, without cargo-fuzz.
- `cargo fuzz run` on Windows exits `0xc0000135` (STATUS_DLL_NOT_FOUND): put the MSVC ASan runtime on `PATH`
  (`.../VC/Tools/MSVC/<ver>/bin/Hostx64/x64`, holds `clang_rt.asan_dynamic-x86_64.dll`). See `fuzz/README.md`. On the dev machine it is on the user PATH since
  2026-11-06 (`D:/zProgramm/Visual Studio/2026/...`); a shell started before that, or after a toolset update, lacks it.
- `cargo fuzz build` on Windows fails with LNK2001 `main` when a dependency is also a `cdylib` (cargo-fuzz adds `/include:main`
  to every link; `--no-include-main-msvc` breaks the targets instead). Keep such crates out of the fuzz build: `flip-rs` sits
  behind `civbb-client`'s default `parity` feature, which `fuzz/Cargo.toml` turns off.
- C/C++ `-sys` crates may bundle the same third-party code with external linkage: `nv-flip-sys` and
  `basisu_c_sys` both compile tinyexr, so any binary linking both fails with LNK2005. `cargo clippy` does not
  link; build a test binary that pulls in both before settling on a native dependency.
