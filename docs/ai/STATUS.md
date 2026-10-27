# Status

Read at the start of every session; update at the end. Keep it short: the queue lives in GitHub issues.

## Now

- Phase 0 (prove the avatar compiler). Current milestone: **M0.3 Importer and corpus**.
- Next issue: #9 glTF importer (also export the generated corpus's mesh/skin/morph subset to glTF as importer test data, per the note on #8), then #10 VRM, #12 CLI, #43 fuzz.
- After M0.3: M0.4 animation runtime. Test avatars come from `civbb-corpus` (`lean()`, `bloated()`).
- Progress for the human: https://github.com/yannikwirbs/civbb/milestones

## Blockers and needs-user

- #11 Booth/VRM avatars for `corpus/local/` (not blocking; synthetic corpus covers tests until M0.9).
- #37, #38 Unity tasks (M0.10, later).

## Decisions in flight

None.

## Session log

One line per session, newest last: date, what moved, what's next.

- 2026-10-28: repo, CI, xtask (ci/arch/codemap), ADRs 0001-0005, labels, 14 milestones, 43 issues. M0.1 and M0.2 done: `format` (avatar model, animator/dynamics/contacts/constraints/cost types, signed package, validation). #8 `civbb-corpus` generator done.
