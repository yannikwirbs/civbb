# 0092: Sessions end at a chain's end or 250 k tokens of context; the handover is STATUS.md's Next and `/continue`

Status: accepted
Date: 2026-11-26

## Context

`docs/iteration-baseline.md` ("Session cost baseline") measured the 20 sessions after the iteration tuning: $9.2 per
closed issue, 61 % of it cache reads. Every API call re-reads the whole context, so a session's cost grows with the
square of its length: the mean call carried 254 k tokens, sessions peaked at 359 k on average (572 k at most), and
calls above 200 k context cost $147 of the $180 spent on cache reads. Sessions ran until the owner stopped them,
often three or four issues. Ending one meant a hand-written `NEXT_PROMPT.md` and 13 calls of upkeep ($1.21, 9 min),
and 36 minutes over the 20 sessions went to watching GitHub CI after pushing. A session cannot see its own context
size.

## Decision

This supersedes the session-start, end and handover parts of ADR 0044; its split of `docs/ai` stands.

- A session works on one issue or one chain of dependent issues. It ends when the chain is done or when the context
  passes 250 k tokens, whichever comes first, at the next clean point (gate green, committed). `tools/context.py`
  tells it: a PostToolUse hook adds a note once the context passes 250 k (and again per further 100 k), and the
  status line shows the size to the owner. Simulated on the measured sessions, a fresh session at 250 k saves 8 to
  11 % of the cost with today's start cost ($1.34), more as the start gets cheaper.
- The handover is `STATUS.md`'s **Next**: the issue(s) to take and anything the next session must know that is not
  in the issue. Work in progress is described in an issue comment. Nothing lives only in chat history.
- `NEXT_PROMPT.md` is gone. The owner starts every session with the project command `/continue`
  (`.claude/commands/continue.md`), a fixed prompt that works from STATUS.md.
- The end protocol is: code map and gate; STATUS (Now, Next, blockers, newest three sessions), one line in
  SESSIONS.md, the STATE.md sections of changed areas, new lessons; commit and push. Sessions do not wait for GitHub
  CI: the next session checks the previous push with `tools/ci.sh` first and fixes a red run before anything else.
- Long jobs (parity, Unity, release builds, studies) run in the background while the session does other work.
- No subagent definitions for now. In the measured sessions subagents did almost nothing (1 call); the candidates
  (CI triage, code search, studies, doc upkeep) either rarely occur, would pass exact numbers through a summary, or
  need the main session's knowledge of what it did. Revisit with `tools/session-stats.py` data.

## Consequences

More, shorter sessions; each pays the start cost again, so the start must stay small (STATUS.md and LESSONS.md are
read whole every time). A chain cut by the limit continues from its issue comment. Compare after about five
sessions with `python tools/session-stats.py --since 2026-11-26`: dollars and tokens per closed issue against the
baseline. If the limit is often hit mid-issue, raise it; if the start cost dominates, cut the start reads further.
