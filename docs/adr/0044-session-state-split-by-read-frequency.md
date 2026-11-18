# 0044: Session state is split by how often it is read

Status: accepted
Date: 2026-11-18

## Context

ADR 0005 put session state in `docs/ai/STATUS.md` (focus, blockers, session log) and `docs/ai/LESSONS.md`, read
whole with `docs/ai/CODEMAP.md` at the start of every session. By M0.10 the three files were 64 KB (~16,000
tokens): STATUS.md had grown a 13 KB narrative of every area built so far plus the full session log, and LESSONS.md
held 77 lessons on every area, most of them about one tool or crate (`docs/iteration-baseline.md`).

## Decision

This supersedes the session-state and orientation bullets of ADR 0005; the rest of 0005 stands.

- Read every session: `STATUS.md` (milestone, focus, blockers, decisions in flight, the newest three sessions) and
  `LESSONS.md` (general gotchas: shell, Python, git, gh, tests, long runs).
- Read before working in an area: its section of `STATE.md` (what exists, by area) and its
  `LESSONS-<topic>.md` (unity, render, textures, parsers, anim, data).
- Appended, rarely read: `SESSIONS.md`, one line per session.
- `CODEMAP.md` stays generated and checked; sessions grep it instead of reading it whole.

## Consequences

Session start drops from ~64 KB to ~10 KB. A session must pick the right topic file before working in an area;
LESSONS.md lists them. A lesson that fits no topic goes in LESSONS.md, which should stay short.
