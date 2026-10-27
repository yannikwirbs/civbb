# Lessons

Environment gotchas and mistakes not to repeat. Add a line whenever something costs more than one attempt.

- Git Bash may lack `~/.cargo/bin` on PATH: prefix with `export PATH="$HOME/.cargo/bin:$PATH" &&`.
- Git Bash rewrites leading-slash args into Windows paths: use `gh api repos/...`, not `gh api /repos/...`.
- A `cd` in a Bash call persists into later calls; use absolute paths.
- Never `canonicalize()` repo paths on Windows: it adds a verbatim `\\?\` prefix and breaks comparisons with cargo metadata paths.
