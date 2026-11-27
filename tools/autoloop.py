"""Back-to-back headless `/continue` sessions until the 5-hour usage window passes a threshold.

Usage is checked only between sessions, so a running session always finishes first.
Usage: python tools/autoloop.py [threshold percent, default 80]
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
                used = int(w["utilization"] * 100)
        elif t == "result" and label:
            mins = e.get("duration_ms", 0) // 60000
            print(f"\n--- [session {label}] ended: {e.get('subtype')}, {e.get('num_turns')} turns, {mins} min ---")
    return proc.wait(), used


def main():
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    limit = int(sys.argv[1]) if len(sys.argv) > 1 else 80
    n = 0
    while True:
        n += 1
        print(f"\n==================== SESSION {n}  ({datetime.now():%Y-%m-%d %H:%M:%S}) ====================", flush=True)
        code, used = run(["/continue", "--permission-mode", "auto"], n, REPO)
        if code != 0:
            sys.exit(f"autoloop: session {n} exited with code {code}, stopping.")
        if used is None:  # no usage event in the stream: ask with a tiny Haiku call
            _, used = run(["reply ok", "--model", "haiku"], None, tempfile.gettempdir())
        print(f"autoloop: 5-hour window at {used if used is not None else 'unknown'}% (limit {limit}%)")
        if used is None or used >= limit:
            print(f"autoloop: stopping after {n} session(s).")
            return


if __name__ == "__main__":
    main()
