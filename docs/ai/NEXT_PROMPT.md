# Next prompt

Paste the block below into a new Claude Code session opened in `D:\Documents\Repos\CIVBB`.

```
Continue CIVBB: Phase 1, milestone M1.1 (Native content model). #100 is designed (ADRs 0085 to 0090, design doc §9) and split into #113 to #125; #113 and #114 are done. Follow the session protocol in CLAUDE.md: read docs/ai/STATUS.md and docs/ai/LESSONS.md, then ADRs 0085 and 0088, issue #115 with its comment (the planned layout and the two-defaults pitfall), and the STATE.md sections for the compiler and the runtime. Work on main.

Do #115: one state table (`Avatar::state`) replaces the animator's parameter table; conditions, blend inputs, drivers, menu controls and contact receivers reference entries by index; the Unity dialect and VRM importers and the `collapse` stage lower into it. It moves data, not evaluation: before changing anything, settle the two defaults (`Controller::start` uses the expression default, `bind.rs` the animator default for parameter slots) and keep both behaviours. The animator goldens, contact recordings, `report-lean.json` and corpus builds must not change; re-bless `tests/golden/unity-dialect-1*.json` and check its diff only moves parameters into `state`. Bump `FORMAT_VERSION` (packages in corpus/local then need `civbb build` again). Then continue with #116 and onwards in number order.

Rules: one logical change per commit, `cargo fmt --all && cargo xtask ci` green before each (start Bash calls that pipe it with `set -o pipefail`), decisions the docs don't make as ADRs, found work as issues in the matching M1.x milestone. The parity table in ADR 0084 must not regress (Yumeka's rest worst view 0.1197 has no margin). Never commit corpus/local contents or Unity packages, never stop Unity processes you did not start, and check that GitHub CI passes after pushing (Linux too).
```
