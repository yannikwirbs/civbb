# 0097: An opt-in cache of encoded textures, keyed by everything the bytes depend on

Status: accepted
Date: 2026-11-26

## Context

A full parity rerun against Unity (`tools/parity.sh`, ADR 0084) took about 25 min, about 93 % of it `civbb build`
(#128). Per-stage timings show `compress` (ADRs 0029, 0035) is nearly all of a build: every texture is encoded up
to three times along the codec ladder, single-threaded for reproducible bytes, at most four at once. Most reruns
follow runtime-only changes or format version bumps that leave every texture's encoding unchanged, so the same
encodes repeat on every run.

## Decision

1. `civbb build --texture-cache <dir>` (and `civbb_pipeline::compress_cached`, `compile`'s `texture_cache`) reads
   and writes one file per encoded texture in `<dir>`. Without the flag nothing is read or written: release builds
   and tests stay pure. `tools/parity.sh` passes `corpus/local/texture-cache` (gitignored).
2. The file name is a BLAKE3 hash of: the source blob's hash, its image format, its colour space, the platform's
   side cap, the hash of `pipeline/src/texture.rs` and of `core/src/mips.rs` (their source text: any edit of the
   encoder settings, the ladder or the mips misses the cache), and the versions of the crates that decide the
   bytes (`texture::ENCODING_DEPENDENCIES`: Basis Universal, `image`, `png`, `zune-jpeg`; a test checks them
   against `Cargo.lock`). The entry holds the chosen codec, whether the texture was capped, and the KTX2 bytes,
   written through a temporary file and a rename.
3. A package built with the cache is byte-identical to one built without it (tested).

## Consequences

- A rerun after a change that touches no texture skips every encode; the build log's `compress` line says how many
  textures came from the cache. Measured: all 13 exports 15 min 11 s on an empty cache, 1 min 12 s on a full one
  (Yumeka's `compress` 265 s to 0.03 s), every package byte-identical and every report equal to the uncached run.
- Not covered by the key: the Rust toolchain and the C++ compiler that builds Basis Universal. A toolchain bump
  (ADR 0045) should start with an empty cache (`rm -rf corpus/local/texture-cache`); so should any doubt.
- The cache grows without bound; it is a local directory to delete when large.
