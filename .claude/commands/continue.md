---
description: Continue CIVBB from docs/ai/STATUS.md's Next (start every session with this)
---

Continue CIVBB. Follow CLAUDE.md: its start-of-session steps (STATUS.md, LESSONS.md, `tools/ci.sh` on the last
push, then the issue STATUS.md's **Next** names, with the STATE.md sections and topic lessons of its area), then
work on Next on `main`.

Rules: one logical change per commit with `cargo fmt --all && cargo xtask ci` green before it; decisions the docs
don't make as ADRs; found work as issues in the matching milestone (`tools/issue.sh new`). The parity table in ADR
0084 must not regress. Never commit `corpus/local` contents or Unity packages; never stop Unity processes you did not
start. End the session per ADR 0092 (chain done or context past 250 k tokens) with CLAUDE.md's end-of-session steps.

$ARGUMENTS
