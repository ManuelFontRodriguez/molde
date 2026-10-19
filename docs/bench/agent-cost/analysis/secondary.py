#!/usr/bin/env python3
"""Secondary results for REPORT.md: not registered as tests, computed after the campaign.

    python3 analysis/secondary.py [--set campaign-all]

Everything here reads the published run folders only (metrics.json, verdict.json,
transcript.jsonl, diff-stat.txt). Standard library only; bootstrap seed 1016.
"""
import argparse
import json
import math
import random
import re
import statistics as st
import sys
from collections import Counter, defaultdict
from pathlib import Path

AC = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(AC / "runner"))
import metrics  # noqa: E402

CORE = ["T1", "T2", "T3", "T4", "T4b", "T5", "T6"]
INCUMBENTS = ["hugo", "astro", "nextjs", "hugo+checker", "astro+checker", "nextjs+checker"]
# Price weights relative to base input, as Claude Code bills them (they reproduce each run's
# own `total_cost_usd` exactly): cache writes are 1-hour writes, 2x; cache reads 0.05x;
# output 5x. The price-weighted ratios below use `total_cost_usd` itself.
WEIGHTS = {"input_tokens": 1.0, "cache_creation_input_tokens": 2.0,
           "cache_read_input_tokens": 0.05, "output_tokens": 5.0}
CHECK_BY_HAND = re.compile(r"(?i)(doesn't|does not|don't|do not) (check|verify|test|cover|catch)|by hand|manually|"
                           r"myself|isn't checked|not checked by")


def load(set_name):
    rows = []
    for m in sorted((AC / "results" / set_name).glob("*/metrics.json")):
        j = json.loads(m.read_text())
        if j.get("outcome") not in ("pass", "fail", "censored"):
            continue
        u = j.get("usage") or {}
        j["weighted"] = (j.get("result") or {}).get("total_cost_usd")
        j["output_tokens"] = u.get("output_tokens") or 0
        j["_dir"] = m.parent
        rows.append(j)
    return rows


def cell(rows, arm, task, key):
    return [r[key] for r in rows if r["arm"] == arm and r["task"] == task and r.get(key) is not None]


def med(xs):
    return st.median(xs) if xs else None


def pooled(rows, a, b, key, tasks=CORE, rng=None):
    logs = []
    for t in tasks:
        xa, xb = cell(rows, a, t, key), cell(rows, b, t, key)
        if not xa or not xb:
            continue
        if rng:
            xa = [rng.choice(xa) for _ in xa]
            xb = [rng.choice(xb) for _ in xb]
        logs.append(math.log(med(xa) / med(xb)))
    return math.exp(sum(logs) / len(logs)) if logs else None


def boot(rows, a, b, key, n=2000):
    rng = random.Random(1016)
    v = sorted(pooled(rows, a, b, key, rng=rng) for _ in range(n))
    return v[int(0.025 * n)], v[int(0.975 * n) - 1]


def tool_uses(run_dir):
    for line in (run_dir / "transcript.jsonl").read_text().splitlines():
        try:
            e = json.loads(line)
        except ValueError:
            continue
        if e.get("type") != "assistant":
            continue
        for b in (e.get("message") or {}).get("content") or []:
            if isinstance(b, dict) and b.get("type") == "tool_use":
                yield b


def assistant_text(run_dir):
    for line in (run_dir / "transcript.jsonl").read_text().splitlines():
        try:
            e = json.loads(line)
        except ValueError:
            continue
        if e.get("type") == "assistant":
            for b in (e.get("message") or {}).get("content") or []:
                if isinstance(b, dict) and b.get("type") == "text":
                    yield b["text"]


def reads_whole_reference(run_dir):
    for b in tool_uses(run_dir):
        inp = b.get("input") or {}
        if b["name"] == "Read" and "molde-reference" in inp.get("file_path", "") and inp.get("offset") is None:
            return True
        if b["name"] == "Bash" and re.search(r"\bcat\s+\S*molde-reference\.md\s*($|[;&|])", inp.get("command", "")):
            return True
    return False


def docs_before_first_run(run_dir, build_cmd):
    """'separate': the reference was read in an earlier call than the first molde run;
    'same call': both in one Bash command (the agent ran molde before seeing the docs);
    'run first': molde ran first; 'neither'."""
    for b in tool_uses(run_dir):
        inp = b.get("input") or {}
        docs = "molde-reference" in json.dumps(inp)
        run = b["name"].startswith("mcp__") or bool(
            re.search(r"(^|[\s;&|(])(molde\s+(check|build)|site-check)\b", inp.get("command", "")))
        if docs and run:
            return "same call"
        if docs:
            return "separate"
        if run:
            return "run first"
    return "neither"


def failure_problems(run_dir):
    v = json.loads((run_dir / "verdict.json").read_text())
    out = []

    def walk(o):
        if isinstance(o, dict):
            if o.get("ok") is False or o.get("pass") is False:
                out.extend(str(p) for p in o.get("problems") or [])
            for x in o.values():
                walk(x)
        elif isinstance(o, list):
            for x in o:
                walk(x)
    walk(v)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--set", default="campaign-all")
    a = ap.parse_args()
    rows = load(a.set)
    p1 = [r for r in rows if r["prompt"] == "P1"]
    print(f"{a.set}: {len(rows)} attempts ({len(p1)} P1)\n")

    print("Price-weighted cost (each run's total_cost_usd), pooled over the core tasks (95% bootstrap, not Holm-corrected)")
    for arm, others in (("molde-check", INCUMBENTS), ("molde+checker", INCUMBENTS[3:])):
        for b in others:
            lo, hi = boot(p1, arm, b, "weighted")
            print(f"  {arm} vs {b}: {pooled(p1, arm, b, 'weighted'):.2f} [{lo:.2f}, {hi:.2f}]")
    print("\nShare of weighted cost, core tasks")
    for arm in ("molde-check", "hugo", "astro", "nextjs"):
        rs = [r for r in p1 if r["arm"] == arm and r["task"] in CORE]
        tot = sum(sum(w * ((r.get("usage") or {}).get(k) or 0) for k, w in WEIGHTS.items()) for r in rs)
        share = {k: sum(w * ((r.get("usage") or {}).get(k) or 0) for r in rs) / tot for k, w in WEIGHTS.items()}
        print(f"  {arm}: " + ", ".join(f"{k.replace('_input_tokens', '').replace('_tokens', '')} {v:.0%}"
                                       for k, v in share.items()))

    print("\nmolde-check / other arm, pooled over the core tasks")
    for key in ("new_tokens", "new_tokens_without_docs", "output_tokens", "assistant_messages", "tool_calls", "wall_s"):
        print(f"  {key}: " + "  ".join(f"{b} {pooled(p1, 'molde-check', b, key):.2f}" for b in INCUMBENTS))

    print("\nSize of the change (diff lines + 1, without the generated next-env.d.ts), pooled over the core tasks")
    for r in rows:
        f = r["_dir"] / "diff-stat.txt"
        gen = 0
        for ln in (f.read_text().splitlines() if f.exists() else []):
            m = re.match(r"\s*next-env\.d\.ts\s*\|\s*(\d+)", ln)
            if m:
                gen = int(m.group(1))
        r["diff_lines_clean"] = max(0, (r.get("diff_lines") or 0) - gen) + 1
    for arm, others in (("molde-check", INCUMBENTS), ("molde+checker", INCUMBENTS[3:])):
        print(f"  {arm}: " + "  ".join(
            f"{b} {pooled(p1, arm, b, 'diff_lines_clean'):.2f} [{boot(p1, arm, b, 'diff_lines_clean')[0]:.2f}, "
            f"{boot(p1, arm, b, 'diff_lines_clean')[1]:.2f}]" for b in others))
    print("  files changed, molde-check: " + "  ".join(
        f"{b} {pooled(p1, 'molde-check', b, 'diff_files_changed'):.2f}" for b in INCUMBENTS))
    for arm in ("molde-check", "hugo", "astro", "nextjs"):
        print(f"  median lines, {arm}: " + " ".join(f"{t} {med(cell(p1, arm, t, 'diff_lines_clean')) - 1:.0f}" for t in CORE))

    print("\nProjection: docs read once every k tasks (molde-check vs hugo / astro / nextjs)")
    for k in (1, 2, 3, 5):
        out = []
        for b in ("hugo", "astro", "nextjs"):
            logs = [math.log((med(cell(p1, "molde-check", t, "new_tokens_without_docs"))
                              + med(cell(p1, "molde-check", t, "docs_tokens_est")) / k)
                             / med(cell(p1, b, t, "new_tokens"))) for t in CORE]
            out.append(f"{math.exp(sum(logs) / len(logs)):.2f}")
        print(f"  k={k}: " + " / ".join(out))

    print("\nmolde-mcp (T1-T6):", "  ".join(
        f"{b} {pooled(p1, 'molde-mcp', b, 'new_tokens', tasks=CORE[:4] + CORE[5:]):.2f}"
        for b in INCUMBENTS + ["molde-check"]))

    p2 = [r for r in rows if r["prompt"] == "P2"]
    t16 = ["T1", "T2", "T3", "T4", "T5", "T6"]
    print("P2 / P1 (T1-T6):", "  ".join(
        f"{arm} {math.exp(st.mean(math.log(med(cell(p2, arm, t, 'new_tokens')) / med(cell(p1, arm, t, 'new_tokens'))) for t in t16)):.2f}"
        for arm in ("molde-check", "hugo", "astro", "nextjs")))
    print("Mean coefficient of variation of new tokens per core cell:", "  ".join(
        f"{arm} {st.mean(st.pstdev(cell(p1, arm, t, 'new_tokens')) / st.mean(cell(p1, arm, t, 'new_tokens')) for t in CORE):.2f}"
        for arm in ("molde-check", "hugo", "astro", "nextjs")))

    print("\nWhole-reference reads, molde P1 runs, by task")
    whole, total = Counter(), Counter()
    for r in p1:
        if r["arm"].startswith("molde"):
            total[r["task"]] += 1
            whole[r["task"]] += reads_whole_reference(r["_dir"])
    print("  " + "  ".join(f"{t} {whole[t]}/{total[t]}" for t in sorted(total)), f"  all {sum(whole.values())}/{sum(total.values())}")
    first = Counter()
    for r in p1:
        if r["arm"] in ("molde-check", "molde+checker", "molde-build") and r["task"] in ("T0", "T4", "T4b", "T10"):
            first[docs_before_first_run(r["_dir"], "molde check --site . --out out")] += 1
    print(f"  first docs read vs first molde run (T0, T4, T4b, T10; three command-line arms): {dict(first)}")

    print("\nRuns where the agent says it checks something by hand")
    by_hand, n = Counter(), Counter()
    for r in p1:
        n[r["arm"]] += 1
        by_hand[r["arm"]] += any(CHECK_BY_HAND.search(t) for t in assistant_text(r["_dir"]))
    print("  " + "  ".join(f"{arm} {by_hand[arm]}/{n[arm]}" for arm in sorted(n)))

    print("\nFailures by cause")
    causes = defaultdict(list)
    for r in rows:
        if r["outcome"] != "fail":
            continue
        probs = failure_problems(r["_dir"])
        if probs and all("belongs on another page" in p for p in probs):
            c = "T6 vs requirement 10"
        elif any("badge color is rgb(255, 255, 255)" in p for p in probs):
            c = "T8 red background"
        elif any("404.html" in p for p in probs):
            c = "Next.js 404.html in the sitemap"
        else:
            c = "other: " + "; ".join(sorted(set(probs)))[:120]
        causes[c].append(f"{r['arm']}|{r['task']}|{r['prompt']}")
    for c, v in sorted(causes.items(), key=lambda x: -len(x[1])):
        print(f"  {len(v):3}  {c}")

    sonnet_dir = AC / "results" / "robust-sonnet"
    if sonnet_dir.is_dir():
        sr = load("robust-sonnet")
        t34 = ["T3", "T4"]
        print(f"\nSonnet robustness row ({len(sr)} runs, T3 and T4; models: "
              f"{sorted({m for r in sr for m in r.get('models_used') or []})}; outcomes {dict(Counter(r['outcome'] for r in sr))})")
        for arm, others in (("molde-check", INCUMBENTS), ("molde+checker", INCUMBENTS[3:])):
            for b in others:
                print(f"  {arm} vs {b}: new {pooled(sr, arm, b, 'new_tokens', tasks=t34):.2f}"
                      f"  without docs {pooled(sr, arm, b, 'new_tokens_without_docs', tasks=t34):.2f}"
                      f"  price-weighted {pooled(sr, arm, b, 'weighted', tasks=t34):.2f}")
        print("  the same comparison on Opus (campaign, T3 and T4): " + "  ".join(
            f"{b} {pooled(p1, 'molde-check', b, 'new_tokens', tasks=t34):.2f}" for b in INCUMBENTS[:3]))
        for model_rows, name in ((sr, "Sonnet"), (p1, "Opus")):
            c = Counter()
            n = Counter()
            for r in model_rows:
                if r["task"] in t34 and r["arm"] in ("molde-check", "molde+checker", "molde-build"):
                    n[(r["arm"], r["task"])] += 1
                    c[(r["arm"], r["task"])] += reads_whole_reference(r["_dir"])
            print(f"  whole-reference reads, {name}: " + "  ".join(f"{a}|{t} {c[(a, t)]}/{n[(a, t)]}" for a, t in sorted(n)))

    print("\nT10 edits without the generated next-env.d.ts")
    for r in p1:
        if r["task"] != "T10":
            continue
        f = r["_dir"] / "diff-stat.txt"
        files = [ln.split("|")[0].strip() for ln in (f.read_text().splitlines() if f.exists() else []) if "|" in ln]
        real = [x for x in files if x != "next-env.d.ts"]
        if files:
            print(f"  {r['arm']} r{r['rep']}: {', '.join(real) or '(only next-env.d.ts)'}")


if __name__ == "__main__":
    main()
