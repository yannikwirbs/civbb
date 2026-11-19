#!/usr/bin/env bash
# Runs a Unity batch method with the Editor that the project's ProjectVersion.txt pins, without -quit: the method
# must exit by itself (EditorApplication.Exit), which also lets play-mode and async methods finish. One line on
# success; on failure the log's error lines. The full log is <project>/Logs/batch-<Method>.log.
# --graphics keeps a graphics device (no -nographics), for methods that render.
# Usage: tools/unity-batch.sh [--graphics] <project dir> <Class.Method> [method args, absolute paths...]
set -u
graphics=-nographics
if [ "${1:-}" = --graphics ]; then graphics=; shift; fi
[ $# -ge 2 ] || { echo "usage: tools/unity-batch.sh [--graphics] <project dir> <Class.Method> [args...]" >&2; exit 2; }
project=$(cd "$1" && pwd -W) || exit 2
method=$2
shift 2
version=$(sed -n 's/^m_EditorVersion: //p' "$project/ProjectSettings/ProjectVersion.txt" | tr -d '\r')
editor="$(unity editors path "$version" | tr -d '\r')/Editor/Unity.exe"
log="$project/Logs/batch-${method##*.}.log"
mkdir -p "$project/Logs"
start=$SECONDS
timeout 900 "$editor" -batchmode $graphics -projectPath "$project" -logFile "$log" -executeMethod "$method" "$@" \
  > /dev/null 2>&1
rc=$?
if [ $rc -eq 0 ]; then
  echo "unity: $method ok in $((SECONDS - start)) s ($(grep -ac '^CIVBB' "$log") CIVBB lines in $log)"
  exit 0
fi
echo "unity: $method failed (exit $rc, 124 = timeout) in $((SECONDS - start)) s; log $log"
grep -aE 'error CS|Exception:|could not be found|^CIVBB failed|Aborting batchmode|No valid Unity Editor license' "$log" | head -40
exit $rc
