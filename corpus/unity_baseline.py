"""Unity CPU baselines (#38, ADR 0081): every local Unity export stepped in the Editor at 0, 1 and 40 copies by
`Civbb.Exporter.Baseline.Measure`, on the logical CPU `civbb-client measure` pins to (ADR 0080).

    cargo build --release -p civbb-client --bins
    python corpus/unity_baseline.py [--project corpus/local/unity-export-vrc] [--variant unity] [name-filter]

Writes corpus/local/unity-baseline/<variant>/<name>.json; `corpus/scorecard.py` reads them. Run it alone, nothing else
compiling. `--optimize` runs AvatarOptimizer's Trace and Optimize through NDMF on each prefab first (needs a host
project with both installed).
"""
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOCAL = ROOT / "corpus" / "local"


def pinned_cpu():
    """The logical CPU `measure` pins its worker to, from a one-frame run of any scorecard package."""
    pkg = next((LOCAL / "scorecard").glob("*/compiled.civpkg"))
    exe = ROOT / "target" / "release" / "civbb-client"
    out = subprocess.run([str(exe), "measure", str(pkg), "--key", str(LOCAL / "unity-ref" / "key"), "--avatars", "1",
                          "--frames", "1", "--warmup", "0"], capture_output=True, text=True, check=True).stdout
    pinned = json.loads(out)["cpu"]["pinned"]
    if not pinned:
        sys.exit("measure ran unpinned: no performance core to match")
    return pinned["logical_cpu"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("filter", nargs="?", default=".")
    ap.add_argument("--project", default=str(LOCAL / "unity-export-vrc"))
    ap.add_argument("--variant", default="unity")
    ap.add_argument("--optimize", action="store_true")
    ap.add_argument("--copies", default="0,1,40")
    a = ap.parse_args()
    cpu = pinned_cpu()
    out = LOCAL / "unity-baseline" / a.variant
    out.mkdir(parents=True, exist_ok=True)
    for line in (LOCAL / "unity-glb" / "exports.txt").read_text(encoding="utf-8").splitlines():
        if "|" not in line:
            continue
        name, prefab = (s.strip() for s in line.split("|", 1))
        if not re.search(a.filter, name):
            continue
        # A graphics device for both variants: AvatarOptimizer refuses -nographics; nothing renders without a camera.
        args = ["bash", str(ROOT / "tools" / "unity-batch.sh"), "--graphics", a.project, "Civbb.Exporter.Baseline.Measure",
                "-civbbAvatar", prefab, "-civbbOut", str((out / f"{name}.json").resolve()), "-civbbCpu", str(cpu),
                "-civbbCopies", a.copies]
        if a.optimize:
            args.append("-civbbOptimize")
        p = subprocess.run(args, capture_output=True, text=True, encoding="utf-8")
        print(f"{name}: {p.stdout.strip() or p.stderr.strip()}", flush=True)
        if p.returncode != 0:
            print(p.stdout[-2000:], file=sys.stderr)


if __name__ == "__main__":
    main()
