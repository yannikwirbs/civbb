#!/usr/bin/env bash
# GitHub Actions result of a pushed commit: one line per workflow run; for a failed job, only its error lines.
# Never blocks unless --wait is given (then run it in the background). Default commit: HEAD.
# Usage: tools/ci.sh [--wait] [commit]
set -u
repo=yannikwirbs/civbb
wait=
if [ "${1:-}" = --wait ]; then wait=1; shift; fi
sha=$(git rev-parse "${1:-HEAD}") || exit 2
runs=$(gh run list --repo $repo --commit "$sha" --json databaseId,workflowName,status,conclusion \
  --jq '.[] | "\(.databaseId) \(.workflowName) \(.status) \(.conclusion)"')
if [ -z "$runs" ]; then
  echo "ci: no runs for ${sha:0:7} (not pushed, or GitHub has not listed them yet)"
  exit 0
fi
if [ -n "$wait" ]; then
  for id in $(echo "$runs" | awk '$3 != "completed" {print $1}'); do
    gh run watch "$id" --repo $repo --exit-status > /dev/null 2>&1
  done
  runs=$(gh run list --repo $repo --commit "$sha" --json databaseId,workflowName,status,conclusion \
    --jq '.[] | "\(.databaseId) \(.workflowName) \(.status) \(.conclusion)"')
fi
rc=0
while read -r id name status conclusion; do
  if [ "$status" != completed ]; then
    echo "ci: ${sha:0:7} $name $status"
    continue
  fi
  echo "ci: ${sha:0:7} $name $conclusion"
  [ "$conclusion" = success ] || [ "$conclusion" = skipped ] && continue
  rc=1
  for job in $(gh run view "$id" --repo $repo --json jobs --jq '.jobs[] | select(.conclusion == "failure") | "\(.databaseId)"'); do
    echo "  job $job (full log: gh api repos/$repo/actions/jobs/$job/logs):"
    gh api "repos/$repo/actions/jobs/$job/logs" 2>/dev/null | sed 's/^[0-9TZ:.-]* //' \
      | grep -aE 'error(\[|:)|warning: unused|panicked|FAILED|failed|^test .* \.\.\. FAILED|xtask:|##\[error\]' \
      | grep -av '^\s*$\|\[36;1m' | head -40 | sed 's/^/    /'
  done
done <<< "$runs"
exit $rc
