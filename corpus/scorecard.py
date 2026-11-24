"""Phase 0 scorecard: is each local Unity export cheaper after compiling, and does it still look like Unity?

For every export in corpus/local/unity-glb/exports.txt this builds the import without compiler stages and the
compiled package, measures both with `civbb-client measure` (release, one worker), and scores the compiled package
against Unity's reference renders in corpus/local/unity-ref/<name>/ (ADR 0048). Prints one Markdown table.

    cargo build --release -p civbb-pipeline -p civbb-client --bins
    python corpus/scorecard.py [name-filter] > scorecard.md

Run it alone: measuring while anything compiles gives meaningless numbers. Outputs go to corpus/local/scorecard/.
"""
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOCAL = ROOT / "corpus" / "local"
BIN = ROOT / "target" / "release"
OUT = LOCAL / "scorecard"
KEY = LOCAL / "unity-ref" / "key"
# `measure` times one worker. Unpinned on a hybrid CPU it lands on a performance or an efficiency core at random
# (Yumeka 196 against 364 µs on the i5-12600K), so pin it to logical CPU 2: a performance core there (0 to 11 are;
# 0 takes most interrupts).
PIN = ["cmd", "/c", "start", "/affinity", "4", "/wait", "/b"] if sys.platform == "win32" else ["taskset", "-c", "2"]


def run(*args, pinned=False):
    """Runs a release binary and returns its stdout; raises with its stderr on failure."""
    exe = str(BIN / args[0])
    cmd = [exe, *map(str, args[1:])]
    p = subprocess.run(PIN + cmd if pinned else cmd, capture_output=True, text=True, encoding="utf-8")
    if p.returncode != 0:
        raise RuntimeError(f"{' '.join(map(str, args))}: {p.stderr.strip()[-400:]}")
    return p.stdout


def measure(pkg):
    m = json.loads(run("civbb-client", "measure", pkg, "--key", KEY, pinned=True))
    gpu = m["gpu"]["gpu_ms_per_avatar"] * 1000 if m["gpu"] else float("nan")
    return m["cpu"]["ms_per_avatar"] * 1000, gpu


def views_mean(state):
    return sum(v["mean"] for v in state["views"]) / len(state["views"])


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    flt = re.compile(sys.argv[1] if len(sys.argv) > 1 else ".")
    rows = []
    for line in (LOCAL / "unity-glb" / "exports.txt").read_text(encoding="utf-8").splitlines():
        name = line.split("|")[0].strip()
        if not name or not flt.search(name):
            continue
        d = OUT / name
        d.mkdir(parents=True, exist_ok=True)
        glb = LOCAL / "unity-glb" / f"{name}.glb"
        source, compiled, report = d / "source.civpkg", d / "compiled.civpkg", d / "report.json"
        run("civbb", "build", glb, "--key", KEY, "--unstaged", "-o", source)
        run("civbb", "build", glb, "--key", KEY, "-o", compiled, "--report", report)
        r = json.loads(report.read_text(encoding="utf-8"))
        first, last = r["stages"][0], r["stages"][-1]
        cpu0, gpu0 = measure(source)
        cpu1, gpu1 = measure(compiled)
        ref = LOCAL / "unity-ref" / name
        unity_rest = unity_worst = float("nan")
        current = "no reference"
        if (ref / "rig.json").exists():
            rig = run("civbb-client", "rig", source, "--key", KEY)
            current = "yes" if json.loads(rig) == json.loads((ref / "rig.json").read_text(encoding="utf-8")) else "stale"
            p = json.loads(run("civbb-client", "parity", ref, compiled, "--key", KEY))
            (d / "unity-parity.json").write_text(json.dumps(p, indent=1), encoding="utf-8")
            unity_rest = views_mean(p["states"][0])
            unity_worst = max(views_mean(s) for s in p["states"])
        rows.append((name, first, last, cpu0, cpu1, gpu0, gpu1, unity_rest, unity_worst, current))
        print(f"{name}: done", file=sys.stderr)

    print("| export | draws | shapes | CPU µs/avatar | GPU µs/avatar | Unity FLIP rest | Unity FLIP worst | reference current |")
    print("| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |")
    for name, a, b, cpu0, cpu1, gpu0, gpu1, rest, worst, cur in rows:
        print(
            f"| {name} | {a['draws']} → {b['draws']} | {a['blend_shapes']} → {b['blend_shapes']} "
            f"| {cpu0:.1f} → {cpu1:.1f} | {gpu0:.1f} → {gpu1:.1f} | {rest:.3f} | {worst:.3f} | {cur} |"
        )


if __name__ == "__main__":
    main()
