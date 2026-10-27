# 0004: Roadmap phases and milestones

Status: accepted
Date: 2026-10-28

## Context

The design doc's roadmap figure ("5 phases, 4 gates") did not survive the export to Markdown. Only Phase 0 is described in detail (§21); Quest is placed in Phase 3 (§18).

## Decision

Phases, reconstructed from the rest of the doc:

| Phase | Goal | Gate to pass |
| --- | --- | --- |
| 0 | Prove the avatar compiler: convert, optimise, run N avatars on all cores, report parity and cost | Compiled avatars visually identical and clearly cheaper than baselines |
| 1 | PC vertical slice: client with OpenXR and desktop mode, node, protocol, voice, minimal hub; friends meet in one instance with converted avatars | 40 avatars at 90 Hz on the reference PC, join under 10 s |
| 2 | Worlds and scripting: Wasm script host, WIT API and SDKs, world conversion, Udon translation, OSC | 70 % of a world corpus runs unchanged |
| 3 | Quest standalone and the content pipeline at scale: Android build, Quest variants, upload pipeline, moderation tools, community nodes | Quest 3 at 72 Hz in the benchmark scene |
| 4 | Public beta: safety and legal readiness, operations, store later | Design doc §1 success metrics |

Phase 0 is split into GitHub milestones `M0.1` to `M0.10`; later phases are one milestone each until they start, then get split the same way.

## Consequences

The human owner can correct the phase contents at any time; a correction supersedes this ADR.
