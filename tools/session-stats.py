"""Measure what Claude Code sessions on this repo cost, from their transcripts (tokens, time, tool output).

Reads every `*.jsonl` under the project's transcript folder (subagent transcripts included, attributed to their
parent session) and git history for issues closed. Never prints transcript content beyond short command and error
prefixes.

    python tools/session-stats.py                      # summary of all sessions
    python tools/session-stats.py --since 2026-11-20   # only sessions started on or after that day
    python tools/session-stats.py --json out.json      # also write every per-session and per-call number

Prices per token are fitted from the transcripts' own `cost-state` records (least squares), so the dollar figures
match what Claude Code reported; tokens are the API's usage counts, not estimates.
"""
import argparse
import collections
import glob
import json
import os
import re
import subprocess
import sys
from datetime import datetime
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOCS_AI = "docs/ai/"
# Bash command classes, first match wins. Order: specific before generic.
CLASSES = [
    ("xtask ci", r"cargo xtask ci\b"),
    ("xtask test", r"cargo xtask test\b"),
    ("xtask codemap", r"cargo xtask codemap\b"),
    ("xtask other", r"cargo xtask\b"),
    ("cargo fmt", r"^cargo fmt\b(?!.*xtask)"),
    ("parity", r"\bparity\b"),
    ("unity", r"unity-batch|Unity\.exe|unity\.exe|\bunity (run|editors|project)"),
    ("civbb run", r"civbb-client|civbb(\.exe)? build|cargo run\b"),
    ("cargo test", r"cargo test\b"),
    ("cargo build", r"cargo (build|check|clippy)\b"),
    ("gh run", r"\bgh run\b"),
    ("gh issue", r"\bgh issue\b"),
    ("gh other", r"\bgh (api|pr|release|label|repo)\b"),
    ("git commit/push", r"\bgit (commit|push)\b"),
    ("git read", r"\bgit (log|diff|show|status|blame|ls-files|grep)\b"),
    ("git other", r"\bgit\b"),
    ("python", r"\bpython3?\b|\bpy\b"),
    ("read/search", r"^(grep|rg|sed|cat|head|tail|wc|ls|find|awk|sort|tr|cut|diff|cmp|xxd|od|file|du)\b"),
]


def ts(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00")).timestamp()


def classify(cmd):
    c = cmd.strip()
    c = re.sub(r"^(set -o pipefail\s*(;|&&)\s*)", "", c)
    c = re.sub(r"^(cd\s+\S+\s*(&&|;)\s*)", "", c)
    c = re.sub(r"^(export [^;&]+(&&|;)\s*)", "", c)
    for name, pat in CLASSES:
        if re.search(pat, c, re.M):
            return name
    return "other"


def subkind(cmd):
    """Finer split of read/search and python commands: what was read, and whether python edited files."""
    c = re.sub(r"^(set -o pipefail\s*(;|&&)\s*|cd \S+\s*&&\s*|S=\S+;?\s*(&&)?\s*)+", "", cmd.strip())
    if re.match(r"python", c):
        return "python edit" if re.search(r"\.write_text|\.write\(|open\([^)]*['\"]w", c) else "python other"
    w = c.split()[0] if c.split() else ""
    if w == "sed" and "-n" in c.split()[1:3]:
        return "sed -n (slice)"
    if w == "sed":
        return "sed edit"
    if w == "cat":
        return "cat scratch" if "Temp/claude" in c or "$S" in c else "cat file"
    return w or "?"


def result_text(content):
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "".join(b.get("text", "") for b in content if isinstance(b, dict) and b.get("type") == "text")
    return ""


def load(path):
    out = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            try:
                out.append(json.loads(line))
            except json.JSONDecodeError:
                pass
    return out


def git_commits():
    log = subprocess.run(
        ["git", "log", "--all", "--format=%ct%x00%s%x00%b%x1e"], cwd=ROOT, capture_output=True, text=True,
        encoding="utf-8").stdout
    commits = []
    for rec in log.split("\x1e"):
        parts = rec.strip("\n").split("\x00")
        if len(parts) < 3:
            continue
        t, subj, body = int(parts[0]), parts[1], parts[2]
        closes = set(int(n) for n in re.findall(r"(?i)\b(?:closes|fixes)\s+#(\d+)", subj + "\n" + body))
        commits.append((t, subj, closes))
    return commits


def fit_prices(sessions):
    """Least-squares $/token per usage kind from cost-state model usage (per session and model)."""
    rows = []
    for s in sessions:
        for model, u in (s.get("model_usage") or {}).items():
            rows.append(([u.get("inputTokens", 0), u.get("outputTokens", 0), u.get("cacheReadInputTokens", 0),
                          u.get("cacheCreationEphemeral5mInputTokens", 0),
                          u.get("cacheCreationEphemeral1hInputTokens", 0)], u.get("costUSD", 0.0), model))
    prices = {}
    for model in sorted(set(r[2] for r in rows)):
        xs = [r[0] for r in rows if r[2] == model]
        ys = [r[1] for r in rows if r[2] == model]
        n = 5
        # Drop columns that are all zero so the normal equations stay solvable.
        cols = [j for j in range(n) if any(x[j] for x in xs)]
        a = [[sum(x[i] * x[j] for x in xs) for j in cols] for i in cols]
        b = [sum(x[i] * y for x, y in zip(xs, ys)) for i in cols]
        sol = solve(a, b)
        p = [0.0] * n
        for k, j in enumerate(cols):
            p[j] = sol[k] if sol else 0.0
        prices[model] = p
    return prices


def solve(a, b):
    n = len(b)
    m = [row[:] + [b[i]] for i, row in enumerate(a)]
    for c in range(n):
        piv = max(range(c, n), key=lambda r: abs(m[r][c]))
        if abs(m[piv][c]) < 1e-30:
            return None
        m[c], m[piv] = m[piv], m[c]
        for r in range(n):
            if r != c:
                f = m[r][c] / m[c][c]
                m[r] = [x - f * y for x, y in zip(m[r], m[c])]
    return [m[i][n] / m[i][i] for i in range(n)]


def analyse_file(recs):
    """API calls (deduplicated by message id), tool calls and prompts of one transcript file."""
    calls, tools, prompts, compactions = {}, {}, [], 0
    cost = None
    first_ts = last_ts = None
    for d in recs:
        t = d.get("timestamp")
        if t:
            tt = ts(t)
            first_ts = tt if first_ts is None else min(first_ts, tt)
            last_ts = tt if last_ts is None else max(last_ts, tt)
        typ = d.get("type")
        if typ == "cost-state":
            cost = d
        m = d.get("message")
        if typ == "assistant" and isinstance(m, dict):
            mid = m.get("id") or d.get("uuid")
            u = m.get("usage") or {}
            if mid not in calls:
                cc = u.get("cache_creation") or {}
                calls[mid] = dict(t=ts(t), model=m.get("model"), side=bool(d.get("isSidechain")),
                                  think=(u.get("output_tokens_details") or {}).get("thinking_tokens") or 0,
                                  inp=u.get("input_tokens", 0), out=u.get("output_tokens", 0),
                                  cr=u.get("cache_read_input_tokens", 0),
                                  cw=u.get("cache_creation_input_tokens", 0),
                                  cw5=cc.get("ephemeral_5m_input_tokens", 0),
                                  cw1h=cc.get("ephemeral_1h_input_tokens", 0), stop=m.get("stop_reason"))
            else:
                calls[mid]["stop"] = m.get("stop_reason") or calls[mid]["stop"]
                calls[mid]["out"] = max(calls[mid]["out"], u.get("output_tokens", 0))
            for b in m.get("content") or []:
                if isinstance(b, dict) and b.get("type") == "tool_use":
                    inp = b.get("input") or {}
                    tools[b["id"]] = dict(t=ts(t), name=b.get("name"), input=inp, call=mid,
                                          side=bool(d.get("isSidechain")))
        elif typ == "user" and isinstance(m, dict):
            c = m.get("content")
            if isinstance(c, list):
                for b in c:
                    if isinstance(b, dict) and b.get("type") == "tool_result" and b.get("tool_use_id") in tools:
                        tc = tools[b["tool_use_id"]]
                        txt = result_text(b.get("content"))
                        tc["out_chars"] = len(txt)
                        tc["err"] = bool(b.get("is_error"))
                        tc["dt"] = ts(t) - tc["t"]
                        tc["err_line"] = txt.strip().splitlines()[0][:120] if tc["err"] and txt.strip() else ""
                        tur = d.get("toolUseResult")
                        tc["bg"] = isinstance(tur, dict) and bool(tur.get("backgroundTaskId"))
                    elif isinstance(b, dict) and b.get("type") == "text" and not d.get("isMeta"):
                        if b.get("text", "").startswith("This session is being continued"):
                            compactions += 1
                        elif not d.get("isSidechain"):
                            prompts.append(ts(t))
            elif isinstance(c, str) and not d.get("isMeta") and not d.get("isSidechain"):
                if c.startswith("This session is being continued"):
                    compactions += 1
                elif not c.startswith("<"):
                    prompts.append(ts(t))
    return dict(calls=calls, tools=tools, prompts=prompts, compactions=compactions, cost=cost,
                first=first_ts, last=last_ts)


def tool_target(tc):
    i = tc["input"]
    if tc["name"] == "Bash" or tc["name"] == "PowerShell":
        return i.get("command", "")
    return i.get("file_path") or i.get("path") or i.get("pattern") or i.get("description") or ""


def norm_path(p):
    p = p.replace("\\", "/")
    m = re.search(r"(?i)repos/civbb/(.*)$", p)
    return m.group(1) if m else p


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", default=None)
    ap.add_argument("--since", default=None, help="YYYY-MM-DD")
    ap.add_argument("--exclude", action="append", default=[], help="session id prefix to skip (e.g. the current)")
    ap.add_argument("--json", default=None)
    ap.add_argument("--top", type=int, default=12)
    args = ap.parse_args()
    d = args.dir or os.path.join(os.path.expanduser("~"), ".claude", "projects", "d--Documents-Repos-CIVBB")

    commits = git_commits()
    sessions = []
    for main_file in sorted(glob.glob(os.path.join(d, "*.jsonl"))):
        sid = Path(main_file).stem
        if any(sid.startswith(x) for x in args.exclude):
            continue
        a = analyse_file(load(main_file))
        if not a["calls"]:
            continue
        subs = [analyse_file(load(f)) for f in glob.glob(os.path.join(d, sid, "subagents", "*.jsonl"))]
        s = dict(id=sid, start=a["first"], end=a["last"], main=a, subs=subs,
                 model_usage=(a["cost"] or {}).get("modelUsage"), cost=a["cost"])
        if args.since and datetime.fromtimestamp(s["start"]).strftime("%Y-%m-%d") < args.since:
            continue
        sessions.append(s)
    sessions.sort(key=lambda s: s["start"])
    prices = fit_prices(sessions)

    def price(c):
        p = prices.get(c["model"]) or prices.get("claude-opus-5-5") or [0] * 5
        cw5 = c["cw5"] if (c["cw5"] or c["cw1h"]) else c["cw"]
        return p[0] * c["inp"] + p[1] * c["out"] + p[2] * c["cr"] + p[3] * cw5 + p[4] * c["cw1h"]

    per_session, all_tools = [], []
    reads_by_path = collections.defaultdict(lambda: [0, 0, set()])  # count, chars, sessions
    for s in sessions:
        a = s["main"]
        calls = sorted(a["calls"].values(), key=lambda c: c["t"])
        main_calls = [c for c in calls if not c["side"]]
        sub_calls = [c for sub in s["subs"] for c in sub["calls"].values()] + [c for c in calls if c["side"]]
        ctx = [c["inp"] + c["cr"] + c["cw"] for c in main_calls]
        # Context drop of more than half between consecutive calls: a compaction or a /clear.
        drops = sum(1 for x, y in zip(ctx, ctx[1:]) if y < x * 0.5 and x > 50000)
        tools = sorted(a["tools"].values(), key=lambda t: t["t"])
        for sub in s["subs"]:
            for tc in sub["tools"].values():
                tc["side"] = True
            tools += sorted(sub["tools"].values(), key=lambda t: t["t"])
        # Human waiting: from an end_turn to the next prompt. Agent time = wall minus that.
        idle = 0.0
        ends = [c["t"] for c in main_calls if c["stop"] == "end_turn"]
        for p in a["prompts"]:
            prev = [e for e in ends if e < p]
            if prev:
                idle += p - max(prev)
        wall = s["end"] - s["start"]
        closed = sorted(set().union(*[c[2] for c in commits if s["start"] <= c[0] <= s["end"] + 120] or [set()]))
        ncommits = sum(1 for c in commits if s["start"] <= c[0] <= s["end"] + 120)
        tot = lambda k, cs: sum(c[k] for c in cs)
        usd_main = sum(price(c) for c in main_calls)
        usd_sub = sum(price(c) for c in sub_calls)

        # Start phase: until the first edit of a file outside docs/ (the first product change).
        def is_code_edit(tc):
            if tc["name"] not in ("Edit", "Write", "NotebookEdit") or tc["side"]:
                return False
            return not norm_path(tool_target(tc)).startswith("docs/")
        main_tools = [t for t in tools if not t["side"]]
        first_edit = next((t["t"] for t in main_tools if is_code_edit(t)), None)
        # End phase: from the first docs/ai edit or codemap run after the last code edit.
        last_edit = max((t["t"] for t in main_tools if is_code_edit(t)), default=s["start"])
        end_marks = [t["t"] for t in main_tools if t["t"] > last_edit and (
            (t["name"] in ("Edit", "Write") and norm_path(tool_target(t)).startswith(DOCS_AI))
            or (t["name"] == "Bash" and "xtask codemap" in tool_target(t)))]
        end_start = min(end_marks) if end_marks else None

        def phase(t0, t1):
            cs = [c for c in main_calls if t0 <= c["t"] <= t1]
            ts_ = [t for t in main_tools if t0 <= t["t"] <= t1]
            return dict(calls=len(cs), usd=round(sum(price(c) for c in cs), 2), secs=round(t1 - t0),
                        out_chars=sum(t.get("out_chars", 0) for t in ts_),
                        ctx_added=(cs[-1]["inp"] + cs[-1]["cr"] + cs[-1]["cw"] - (cs[0]["inp"] + cs[0]["cr"]
                                                                                  + cs[0]["cw"])) if cs else 0)
        start_phase = phase(s["start"], first_edit) if first_edit else None
        end_phase = phase(end_start, s["end"]) if end_start else None

        for t in tools:
            t["session"] = s["id"][:8]
            t["usd_share"] = 0.0
            if t["name"] == "Read":
                rp = reads_by_path[norm_path(t["input"].get("file_path", ""))]
                rp[0] += 1
                rp[1] += t.get("out_chars", 0)
                rp[2].add(s["id"][:8])
        # Carry cost of a tool output: its tokens are re-read (cache) by every later main-thread call
        # until the context drops (compaction/clear). chars/4 approximates tokens.
        p_cr = (prices.get("claude-opus-5-5") or [0, 0, 0, 0, 0])[2]
        p_cw = (prices.get("claude-opus-5-5") or [0, 0, 0, 0, 0])[4] or (prices.get("claude-opus-5-5") or [0] * 4)[3]
        call_t = [c["t"] for c in main_calls]
        drop_t = [main_calls[i + 1]["t"] for i, (x, y) in enumerate(zip(ctx, ctx[1:])) if y < x * 0.5 and x > 50000]
        for t in main_tools:
            if "out_chars" not in t:
                continue
            horizon = min([x for x in drop_t if x > t["t"]] or [float("inf")])
            later = sum(1 for x in call_t if t["t"] < x < horizon)
            tok = t["out_chars"] / 4
            t["carry_usd"] = tok * (p_cw + p_cr * later)
        all_tools += tools

        per_session.append(dict(
            id=s["id"][:8], start=datetime.fromtimestamp(s["start"]).strftime("%m-%d %H:%M"),
            wall_min=round(wall / 60), agent_min=round((wall - idle) / 60), prompts=len(a["prompts"]),
            api_calls=len(main_calls), sub_calls=len(sub_calls), subagents=len(s["subs"]),
            inp=tot("inp", main_calls), cr=tot("cr", main_calls), cw=tot("cw", main_calls),
            out=tot("out", main_calls), think=tot("think", main_calls), ctx_series=ctx, sub_tokens=sum(c["inp"] + c["cr"] + c["cw"] + c["out"] for c in sub_calls),
            peak_ctx=max(ctx) if ctx else 0, first_ctx=ctx[0] if ctx else 0, ctx_drops=drops,
            compactions=a["compactions"], usd=round(usd_main + usd_sub, 2), usd_sub=round(usd_sub, 2),
            usd_reported=round((s["cost"] or {}).get("totalCostUSD", 0), 2),
            api_s=round((s["cost"] or {}).get("totalAPIDuration", 0) / 1000),
            tool_s=round((s["cost"] or {}).get("totalToolDuration", 0) / 1000),
            tool_calls=len(main_tools), tool_chars=sum(t.get("out_chars", 0) for t in main_tools),
            errors=sum(1 for t in main_tools if t.get("err")), commits=ncommits, closed=closed,
            start_phase=start_phase, end_phase=end_phase))

    # ---- Report ----
    W = sys.stdout.write
    W(f"sessions: {len(per_session)}  prices $/Mtok (in, out, cache read, cache write 5m, 1h):\n")
    for m, p in prices.items():
        W(f"  {m}: " + ", ".join(f"{x * 1e6:.2f}" for x in p) + "\n")
    W("\nper session (usd = fitted; rep = Claude Code's own total; ctx in k tokens):\n")
    W(f"{'id':8} {'start':11} {'wall':>5} {'agent':>5} {'pr':>3} {'calls':>5} {'sub':>4} {'out_k':>6} "
      f"{'cr_M':>6} {'cw_k':>6} {'peak':>5} {'drop':>4} {'usd':>7} {'rep':>7} {'tools':>5} {'outKB':>6} "
      f"{'err':>4} {'cmt':>3} closed\n")
    for r in per_session:
        W(f"{r['id']:8} {r['start']:11} {r['wall_min']:>5} {r['agent_min']:>5} {r['prompts']:>3} "
          f"{r['api_calls']:>5} {r['sub_calls']:>4} {r['out'] / 1e3:>6.0f} {r['cr'] / 1e6:>6.1f} "
          f"{r['cw'] / 1e3:>6.0f} {r['peak_ctx'] / 1e3:>5.0f} {r['ctx_drops'] + r['compactions']:>4} "
          f"{r['usd']:>7.2f} {r['usd_reported']:>7.2f} {r['tool_calls']:>5} {r['tool_chars'] / 1e3:>6.0f} "
          f"{r['errors']:>4} {r['commits']:>3} {','.join(map(str, r['closed']))}\n")
    tusd = sum(r["usd"] for r in per_session)
    tclosed = sum(len(r["closed"]) for r in per_session)
    W(f"\ntotal fitted ${tusd:.0f} (reported ${sum(r['usd_reported'] for r in per_session):.0f}), "
      f"issues closed {tclosed}, $ per closed issue {tusd / max(tclosed, 1):.1f}, "
      f"agent hours {sum(r['agent_min'] for r in per_session) / 60:.1f}\n")
    cr_usd = sum(r["cr"] for r in per_session) * prices.get("claude-opus-5-5", [0] * 5)[2]
    out_usd = sum(r["out"] for r in per_session) * prices.get("claude-opus-5-5", [0] * 5)[1]
    cw_usd = sum(r["cw"] for r in per_session) * max(prices.get("claude-opus-5-5", [0] * 5)[3:5])
    W(f"main-thread cost split (approx): cache read ${cr_usd:.0f}, cache write ${cw_usd:.0f}, "
      f"output ${out_usd:.0f}; subagents ${sum(r['usd_sub'] for r in per_session):.0f}\n")

    p_opus = prices.get("claude-opus-5-5", [0] * 5)
    W(f"thinking tokens {sum(r['think'] for r in per_session) / 1e6:.2f} M of "
      f"{sum(r['out'] for r in per_session) / 1e6:.2f} M output; base context at the first call "
      f"{sum(r['first_ctx'] for r in per_session) / max(len(per_session), 1) / 1e3:.0f} k tokens (mean)\n")
    W("main-thread cache-read $ by context size at the call:\n")
    bands = [(0, 100e3), (100e3, 200e3), (200e3, 300e3), (300e3, 400e3), (400e3, 1e9)]
    for lo, hi in bands:
        xs = [x for r in per_session for x in r["ctx_series"] if lo <= x < hi]
        W(f"  {lo / 1e3:>4.0f}-{min(hi, 1e6) / 1e3:<5.0f}k calls {len(xs):>5}  ${sum(xs) * p_opus[2]:>6.0f}\n")
    # What-if: start a fresh session whenever the context passes `limit`. The fresh session starts at the base
    # context plus `reload` tokens (protocol reads and re-reading the files in hand) and costs `restart_usd` once.
    start = [r["start_phase"] for r in per_session if r["start_phase"]]
    reload = sum(p["ctx_added"] for p in start) / max(len(start), 1)
    restart_usd = sum(p["usd"] for p in start) / max(len(start), 1)
    base = sum(r["first_ctx"] for r in per_session) / max(len(per_session), 1)
    W(f"what-if, fresh session at a context limit (restart reloads {reload / 1e3:.0f} k tokens and costs "
      f"${restart_usd:.2f}, the mean measured start phase):\n")
    actual = sum(sum(r["ctx_series"]) for r in per_session) * p_opus[2]
    for limit in (150e3, 200e3, 250e3, 300e3):
        cost, restarts = 0.0, 0
        for r in per_session:
            off = 0.0
            for x in r["ctx_series"]:
                if x - off > limit:
                    off = x - (base + reload)
                    restarts += 1
                cost += (x - off) * p_opus[2]
        new = cost + restarts * restart_usd
        W(f"  limit {limit / 1e3:.0f} k: restarts {restarts}, cache-read ${actual:.0f} -> ${cost:.0f}, "
          f"plus restarts ${restarts * restart_usd:.0f}: saves ${actual - new:.0f} "
          f"({(actual - new) / max(tusd, 1) * 100:.0f} % of all)\n")

    main_tools = [t for t in all_tools if not t["side"] and "out_chars" in t]
    W("\nbash read/search and python by sub-kind: calls, output KB, carry $, wall min, errors\n")
    sk = collections.defaultdict(lambda: [0, 0, 0.0, 0.0, 0])
    for t in main_tools:
        if t["name"] == "Bash" and classify(t["input"].get("command", "")) in ("read/search", "python"):
            v = sk[subkind(t["input"].get("command", ""))]
            v[0] += 1
            v[1] += t["out_chars"]
            v[2] += t.get("carry_usd", 0)
            v[3] += max(t.get("dt", 0), 0)
            v[4] += t.get("err", False)
    for k, v in sorted(sk.items(), key=lambda kv: -kv[1][2])[:args.top]:
        W(f"  {k:22} {v[0]:>5} {v[1] / 1e3:>7.0f} {v[2]:>8.1f} {v[3] / 60:>7.1f} {v[4]:>4}\n")
    W("\ntool calls by kind (main thread): calls, output KB, carry $ (output re-read until session end), "
      "wall min, errors\n")
    kinds = collections.defaultdict(lambda: [0, 0, 0.0, 0.0, 0])
    for t in main_tools:
        k = ("Bash:" + classify(t["input"].get("command", ""))) if t["name"] == "Bash" else t["name"]
        t["kind"] = k
        v = kinds[k]
        v[0] += 1
        v[1] += t["out_chars"]
        v[2] += t.get("carry_usd", 0)
        v[3] += max(t.get("dt", 0), 0)
        v[4] += t.get("err", False)
    for k, v in sorted(kinds.items(), key=lambda kv: -kv[1][2]):
        W(f"  {k:22} {v[0]:>5} {v[1] / 1e3:>7.0f} {v[2]:>8.1f} {v[3] / 60:>7.1f} {v[4]:>4}\n")

    W(f"\nlargest single outputs (KB, carry $, kind, target):\n")
    for t in sorted(main_tools, key=lambda t: -t["out_chars"])[:args.top]:
        W(f"  {t['out_chars'] / 1e3:>6.1f} {t.get('carry_usd', 0):>6.2f} {t['session']} {t['kind']:18} "
          f"{tool_target(t)[:90]!r}\n")

    W("\nslowest calls (min, kind, target):\n")
    for t in sorted(main_tools, key=lambda t: -t.get("dt", 0))[:args.top]:
        W(f"  {t.get('dt', 0) / 60:>6.1f} {t['session']} {t['kind']:18} {tool_target(t)[:90]!r}\n")

    W("\nfiles read with Read (count, KB, sessions), re-read most:\n")
    for p, (n, ch, ss) in sorted(reads_by_path.items(), key=lambda kv: -kv[1][1])[:args.top]:
        W(f"  {n:>4} {ch / 1e3:>7.0f} {len(ss):>3}  {p}\n")

    W("\nbash command prefixes (first 3 words), most frequent:\n")
    pref = collections.Counter()
    for t in main_tools:
        if t["name"] == "Bash":
            c = re.sub(r"^(set -o pipefail\s*(;|&&)\s*|cd \S+\s*&&\s*)+", "", t["input"].get("command", ""))
            pref[" ".join(c.split()[:3])[:60]] += 1
    for p, n in pref.most_common(args.top * 2):
        W(f"  {n:>4} {p}\n")

    W("\nrepeated sequences of bash kinds (3 consecutive calls):\n")
    seq = collections.Counter()
    for sid in set(t["session"] for t in main_tools):
        ks = [t["kind"] for t in main_tools if t["session"] == sid]
        for i in range(len(ks) - 2):
            if all(k.startswith("Bash:") for k in ks[i:i + 3]):
                seq[" > ".join(k[5:] for k in ks[i:i + 3])] += 1
    for p, n in seq.most_common(args.top):
        W(f"  {n:>4} {p}\n")

    W("\nerrors by kind, and an error followed by the same kind (retry):\n")
    errk, retry, errl = collections.Counter(), collections.Counter(), collections.Counter()
    for i, t in enumerate(main_tools):
        if t.get("err"):
            errk[t["kind"]] += 1
            errl[re.sub(r"\d+", "N", t["err_line"])[:90]] += 1
            if i + 1 < len(main_tools) and main_tools[i + 1]["kind"] == t["kind"]:
                retry[t["kind"]] += 1
    for k, n in errk.most_common(args.top):
        W(f"  {n:>4} (retried {retry[k]:>3}) {k}\n")
    W("  most common first lines:\n")
    for k, n in errl.most_common(args.top):
        W(f"    {n:>4} {k!r}\n")

    W("\nsession start (until first non-docs edit) and end (docs/ai upkeep after last code edit):\n")
    for name in ("start_phase", "end_phase"):
        ph = [r[name] for r in per_session if r[name]]
        if ph:
            W(f"  {name}: sessions {len(ph)}, mean calls {sum(p['calls'] for p in ph) / len(ph):.0f}, "
              f"mean ${sum(p['usd'] for p in ph) / len(ph):.2f}, mean min {sum(p['secs'] for p in ph) / len(ph) / 60:.1f}, "
              f"mean output KB {sum(p['out_chars'] for p in ph) / len(ph) / 1e3:.0f}, "
              f"mean context added k {sum(p['ctx_added'] for p in ph) / len(ph) / 1e3:.0f}, "
              f"total ${sum(p['usd'] for p in ph):.0f}\n")

    if args.json:
        for t in all_tools:
            t["target"] = tool_target(t)[:200]
            t.pop("input", None)
        Path(args.json).write_text(json.dumps(dict(prices=prices, sessions=per_session, tools=all_tools),
                                              default=str), encoding="utf-8")


if __name__ == "__main__":
    main()
