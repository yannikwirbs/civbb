"""Back-to-back headless `/continue` sessions while the 5-hour usage window has room for another.

Between sessions it predicts the next session's usage as the average of past sessions' usage
(percentage points of the window) and stops if that prediction would cross the limit or 100%.
Usage is checked only between sessions, so a running session always finishes first.
Usage: python tools/autoloop.py [limit percent, default 90]
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CLAUDE = shutil.which("claude") or sys.exit("autoloop: `claude` not found on PATH")
STREAM = ["--output-format", "stream-json", "--verbose"]


def run(args, label, cwd):
    """Runs one headless claude call; prints the main agent's text. Returns (exit code, 5h usage % or None)."""
    used = None
    proc = subprocess.Popen(
        [CLAUDE, "-p", *args, *STREAM],
        cwd=cwd,
        stdout=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    for line in proc.stdout:
        try:
            e = json.loads(line)
        except ValueError:
            print(line, end="", flush=True)
            continue
        t = e.get("type")
        if t == "assistant" and not e.get("parent_tool_use_id") and label:
            for b in e["message"].get("content", []):
                text = b.get("text", "")
                if b.get("type") == "text" and text.strip():
                    print(f"\n--- [session {label}] Claude ---\n{text}", flush=True)
        elif t == "rate_limit_event":
            w = e["rate_limit_info"].get("unifiedWindows", {}).get("five_hour")
            if w:
                used = w["utilization"] * 100
        elif t == "result" and label:
            mins = e.get("duration_ms", 0) // 60000
            print(f"\n--- [session {label}] ended: {e.get('subtype')}, {e.get('num_turns')} turns, {mins} min ---")
    return proc.wait(), used


def usage():
    """Current 5h usage % via a tiny Haiku call, or None if unknown."""
    return run(["reply ok", "--model", "haiku"], None, tempfile.gettempdir())[1]


def main():
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    limit = float(sys.argv[1]) if len(sys.argv) > 1 else 90
    before = usage()
    costs = []  # usage (percentage points) of each past session
    n = 0
    while True:
        avg = sum(costs) / len(costs) if costs else 0
        if before is None:
            print("autoloop: 5-hour usage unknown, stopping.")
            return
        predicted = before + avg
        print(f"autoloop: window at {before:.1f}%, avg session {avg:.1f}%, next predicted {predicted:.1f}% (limit {limit:g}%)")
        if predicted > limit or predicted > 100:
            print(f"autoloop: stopping after {n} session(s).")
            return
        n += 1
        print(f"\n==================== SESSION {n}  ({datetime.now():%Y-%m-%d %H:%M:%S}) ====================", flush=True)
        code, after = run(["/continue", "--permission-mode", "auto"], n, REPO)
        if code != 0:
            sys.exit(f"autoloop: session {n} exited with code {code}, stopping.")
        if after is None:
            after = usage()
        if after is not None and after >= before:  # a drop means the window reset mid-session: no clean sample
            costs.append(after - before)
        before = after


if __name__ == "__main__":
    main()
