#!/usr/bin/env bash
# GitHub issues in one call each, compact: an issue with its comments, the open queue of a milestone, a new issue.
# Usage:
#   tools/issue.sh show <N>                                   # title, labels, milestone, body, every comment
#   tools/issue.sh queue [milestone title]                    # open issues, lowest first (default: STATUS.md's)
#   tools/issue.sh new <milestone title> <labels,comma> <title> < body.md   # prints "#N title"
set -u
repo=yannikwirbs/civbb
case "${1:-}" in
show)
  gh issue view "$2" --repo $repo --json number,title,state,labels,milestone,body,comments --jq '
    "#\(.number) \(.title) [\(.state); \([.labels[].name] | join(", ")); \(.milestone.title // "no milestone")]\n\n"
    + .body
    + ([.comments[] | "\n\n--- \(.author.login) \(.createdAt[:10])\n\(.body)"] | join(""))'
  ;;
queue)
  m=${2:-$(sed -n 's/.*\*\*Milestone:\*\* \([^(]*[^ (]\) *(.*/\1/p' "$(dirname "$0")/../docs/ai/STATUS.md" | head -1)}
  [ -n "$m" ] || { echo "issue: no milestone given and none in STATUS.md (**Milestone:** line)" >&2; exit 2; }
  gh issue list --repo $repo --milestone "${m% }" --state open --limit 100 --json number,title,labels --jq '
    sort_by(.number)[] | "#\(.number) \(.title)\(if (.labels | length) > 0 then " [" + ([.labels[].name] | join(", ")) + "]" else "" end)"'
  ;;
new)
  [ $# -eq 4 ] || { echo "usage: tools/issue.sh new <milestone> <labels> <title> < body" >&2; exit 2; }
  url=$(gh issue create --repo $repo --milestone "$2" --label "$3" --title "$4" --body-file -) || exit 1
  echo "#${url##*/} $4"
  ;;
*)
  sed -n '2,6p' "$0" >&2
  exit 2
  ;;
esac
