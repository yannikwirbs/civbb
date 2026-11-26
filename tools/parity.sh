#!/usr/bin/env bash
# Parity rerun against Unity's renders (ADR 0083, 0084) for every local export, or those matching a regex. Builds
# the release binaries, then per export: compile the package, run `civbb-client parity`, keep the full report.
# Prints one line per export with numbers copied from the report unrounded: the overall light, the number of red
# states, and the rest state's per-view means (and interior means when present). With --against <old out dir>,
# each line also shows that run's rest view means. Takes ~25 min for all 13 exports: run it in the background.
# Usage: tools/parity.sh [--against <old out dir>] <out dir> [name regex]
set -u
against=
if [ "${1:-}" = --against ]; then against=$2; shift 2; fi
[ $# -ge 1 ] || { sed -n '2,7p' "$0" >&2; exit 2; }
out=$1
filter=${2:-.}
root=$(cd "$(dirname "$0")/.." && pwd)
ref=$root/corpus/local/unity-ref
cd "$root" || exit 2
cargo build --release -q -p civbb-pipeline -p civbb-client --bins 2>&1 | grep -E '^error' -A5 | head -30
[ "${PIPESTATUS[0]}" -eq 0 ] || { echo "parity: release build failed"; exit 1; }
mkdir -p "$out"
summary='
import json, sys
def rest(path):
    try:
        d = json.load(open(path, encoding="utf-8"))
    except OSError:
        return None, None
    return d, d["states"][0]
d, r = rest(sys.argv[1])
if d is None:
    print(f"{sys.argv[3]:16} no report"); sys.exit()
states, light, rest_light = d["states"], d["light"], r["light"]
reds = sum(1 for s in states if s["light"] == "red")
line = f"{sys.argv[3]:16} {light:6} red states {reds}/{len(states)}  rest {rest_light} mean "
line += " ".join(repr(v["mean"]) for v in r["views"])
if all("interior" in v for v in r["views"]):
    line += "  interior " + " ".join(repr(v["interior"]["mean"]) for v in r["views"])
if sys.argv[2]:
    _, o = rest(sys.argv[2])
    line += "  | before " + (" ".join(repr(v["mean"]) for v in o["views"]) if o else "none")
print(line)
'
while IFS='|' read -r name _; do
  echo "$name" | grep -qE "$filter" || continue
  dir=$out/$name
  mkdir -p "$dir"
  if ! ./target/release/civbb build "corpus/local/unity-glb/$name.glb" --key "$ref/key" -o "$dir/pkg.civpkg" \
    > "$dir/build.log" 2>&1 < /dev/null; then
    echo "$name: build failed ($dir/build.log)"
    continue
  fi
  rm -rf "$dir/flip"
  if ! ./target/release/civbb-client parity "$ref/$name" "$dir/pkg.civpkg" --key "$ref/key" --images "$dir/flip" \
    > "$dir/parity.json" 2> "$dir/parity.log" < /dev/null; then
    echo "$name: parity failed ($dir/parity.log)"
    continue
  fi
  python -c "$summary" "$dir/parity.json" "${against:+$against/$name/parity.json}" "$name"
done < corpus/local/unity-glb/exports.txt
echo "parity: reports in $out/<name>/parity.json"
