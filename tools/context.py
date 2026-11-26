"""Context size of the running Claude Code session: a status line, and a hook that says when to wrap up (ADR 0092).

Reads the newest main-thread API usage from the end of the session transcript (input + cache read + cache write
tokens: what the next call re-reads).

    python tools/context.py statusline   # statusLine command: prints "ctx 254k/250k"
    python tools/context.py hook         # PostToolUse hook: one note per 100 k band once the context passes the limit

The limit is 250 k tokens (CIVBB_CONTEXT_LIMIT overrides it). Both read Claude Code's JSON on stdin.
"""
import json
import os
import sys


LIMIT = int(os.environ.get("CIVBB_CONTEXT_LIMIT", "250000"))
BAND = 100_000


def context_tokens(transcript):
    try:
        with open(transcript, "rb") as f:
            f.seek(0, os.SEEK_END)
            f.seek(max(0, f.tell() - 1_000_000))
            lines = f.read().split(b"\n")
    except OSError:
        return None
    for line in reversed(lines):
        if b'"usage"' not in line or b'"assistant"' not in line:
            continue
        try:
            d = json.loads(line)
        except json.JSONDecodeError:
            continue
        if d.get("type") != "assistant" or d.get("isSidechain"):
            continue
        u = (d.get("message") or {}).get("usage") or {}
        return u.get("input_tokens", 0) + u.get("cache_read_input_tokens", 0) + u.get("cache_creation_input_tokens", 0)
    return None


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else ""
    try:
        data = json.load(sys.stdin)
    except json.JSONDecodeError:
        data = {}
    n = context_tokens(data.get("transcript_path", ""))
    if mode == "statusline":
        print(f"ctx {n // 1000 if n else 0}k/{LIMIT // 1000}k")
        return
    if mode != "hook" or n is None or n < LIMIT:
        return
    band = (n - LIMIT) // BAND
    tmp = os.environ.get("TEMP") or os.environ.get("TMPDIR") or "/tmp"  # tempfile's import costs 40 ms per call
    mark = os.path.join(tmp, f"civbb-context-{data.get('session_id', 'x')}")
    try:
        with open(mark, encoding="utf-8") as f:
            if int(f.read() or -1) >= band:
                return
    except (OSError, ValueError):
        pass
    with open(mark, "w", encoding="utf-8") as f:
        f.write(str(band))
    note = (f"Context is {n // 1000}k tokens (limit {LIMIT // 1000}k, ADR 0092). Every call re-reads all of it: "
            "finish the current issue (or the chain of dependent ones), run the end-of-session protocol, and leave "
            "the rest to a new session.")
    print(json.dumps({"hookSpecificOutput": {"hookEventName": "PostToolUse", "additionalContext": note}}))


if __name__ == "__main__":
    main()
