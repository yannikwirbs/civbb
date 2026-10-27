# Architecture

A short map that stays current. Detailed reasoning lives in `design-doc.md` and `adr/`; a file-level index lives in `ai/CODEMAP.md` (generated).

## Paths through the system

- **Build path:** creator source package → `pipeline` (validate, normalise, compile, optimise, measure, sign) → runtime package (signed CBOR manifest + BLAKE3-addressed blobs) → storage/CDN.
- **Runtime paths:** client ⇄ node over QUIC (shared instance state), client → hub API (accounts, join tokens), client ← CDN (content only).

## Crates and layers

| Layer | Crates | May depend on |
| --- | --- | --- |
| Contracts | `format`, `protocol`, `wit` | nothing internal |
| Engine | `core`, `render`, `anim`, `dynamics`, `audio`, `xr`, `ui`, `script-host` | `core`, `format` |
| Glue | `client`, `node`, `pipeline` | anything except other glue; `node` never `render` |

`cargo xtask arch` enforces this. Crates are created when their first issue starts, not ahead of time.

## Phase 0 shape

`pipeline` imports glTF/VRM, compiles to `format` types and writes runtime packages. `anim` and `dynamics` are pure step functions over per-avatar data (ADR 0003), run in parallel by `rayon`. `client` in Phase 0 is a desktop viewer with a headless benchmark mode.
