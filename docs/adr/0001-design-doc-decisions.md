# 0001: Adopt the design doc's decisions D0 to D10

Status: accepted
Date: 2026-10-28

## Context

`docs/design-doc.md` §23 records decisions D0 to D10 made before any code existed.

## Decision

They are binding until superseded by a later ADR:

- D0: open-source hobby project, built with heavy AI assistance, maintainable without it.
- D1: custom engine in Rust on Vulkan (`ash`) and OpenXR.
- D2: scripting through WebAssembly with several languages on one WIT API (C# subset, Luau, strict TypeScript, Rust, graphs).
- D3: MIT OR Apache-2.0 everywhere, except the node (instance server), which is AGPL-3.0.
- D4: Quest 2 supported with the strictest content variant.
- D5: marketplace later, as the official store.
- D6: age verification after the core works; hooks (age status, instance age requirement) built in from the start.
- D7: voice evidence later, as a local rolling buffer sent only with a report.
- D8: free by default, funded by donations and a small store fee.
- D9: codename CIVBB; final name open.
- D10: hub-and-nodes with official, community and peer-to-peer nodes.

## Consequences

The design doc stays the product spec. Implementation-level choices and deviations get their own ADRs.
