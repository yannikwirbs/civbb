# 0013: Unity ground truth is recorded in batch mode by `tools/unity-truth`

Status: accepted
Date: 2026-10-28

## Context

ADR 0002 says conformance against Unity waits for the exporter, so animator tests use hand-written golden files
until then. Unity 6.3 (6000.3.27f1, Personal licence) is now installed on the dev machine and sessions may run it in
batch mode (CLAUDE.md). The humanoid muscle model (#48) cannot be derived from documentation at all: Unity
publishes the muscle names and default ranges but not how a muscle value becomes a bone rotation.

## Decision

This supersedes the Consequences paragraph of ADR 0002; its Decision stands (input stays glTF/VRM and generated
avatars until the exporter).

1. `tools/unity-truth/` is a minimal Unity project (editor scripts only, no assets, no packages beyond the
   built-in animation module) that records Unity behaviour as JSON. It is not a crate (excluded from the
   workspace). Only `Assets/`, `Packages/manifest.json` and `ProjectSettings/ProjectVersion.txt` are committed;
   Unity regenerates the rest.
2. Sessions run it headless:
   `Unity.exe -batchmode -nographics -quit -projectPath tools/unity-truth -executeMethod <Class>.<Method> -logFile <log> <args>`.
   Recordings go to the test directory of the crate that checks them (first: `crates/anim/tests/unity/`) and are
   committed, so `cargo xtask ci` never needs Unity.
3. Recordings come from inputs the script builds itself (skeletons, controllers, clips), never from third-party
   avatars. Local corpus avatars may be recorded for local checks, but those recordings stay in `corpus/local/`.
4. Where the public API does not expose what we need, the script may read Unity's internal accessors by
   reflection (for example `Avatar.GetPreRotation`). That is reading Unity's own state on our own inputs, as the
   design doc's exporter does with serialised fields; the exporter may use the same accessors.
5. A recording that settles an "unverified" choice in an ADR either confirms it (note it in the next ADR that
   touches the area) or supersedes that ADR.

## Consequences

- Muscle space is verified against Unity now (ADR 0014) instead of guessed.
- The hand-derived golden animator cases and the unverified choices of ADRs 0007 to 0011 can be checked before
  the exporter exists (#54).
- Recordings depend on a Unity version; each records it. Re-record when the pinned editor changes and treat
  differences as findings.
- The exporter's reference renders (design doc §10) still need creator projects and stay with M0.10.
