#!/usr/bin/env python3
"""The pre-registered analysis (PREDICTIONS.md), from the runs' metrics.json alone:

    python3 analyze.py [--set campaign] [--out results/<set>/analysis.json]

Every number in the report comes from this script, and it needs nothing but the published
run folders: anyone can re-run it on the published data and get the same tables (the
bootstrap seed is fixed, 1016). Standard library only.

- H1 (cost): per task, median(new tokens, molde arm) / median(new tokens, other arm) over
  all attempts; pooled over the core tasks as the geometric mean of the per-task ratios;
  95% bootstrap interval (10,000 resamples within each arm x task cell); Holm correction
  over the six comparisons. Reported with and without docs tokens, and with and without T2.
- H2 (safety): false-done on T3, T4 and T4b pooled, molde-check against the three arms
  without a checker pooled; Fisher's exact test, one-sided; Clopper-Pearson intervals.
- H2b: molde-check's "checker passed (blind)" false-dones on T4b against T4.
- H3: T8 ratio band [0.7, 1.4]; T9 molde-check against nextjs; T10 over-editing.
- H4: molde-check against molde-build; each +checker arm against its framework.
Attempts are runs with outcome pass, fail or censored; infrastructure failures are excluded.
"""
import argparse
import json
import math
import random
import statistics
from pathlib import Path

HERE = Path(__file__).resolve().parent
CORE = ["T1", "T2", "T3", "T4", "T4b", "T5", "T6"]
INCUMBENTS = ["hugo", "astro", "nextjs", "hugo+checker", "astro+checker", "nextjs+checker"]
NO_CHECKER = ["hugo", "astro", "nextjs"]
B, SEED, ALPHA = 10_000, 1016, 0.05


def load(set_name, prompt):
    rows = []
    for m in sorted((HERE / "results" / set_name).glob("*/metrics.json")):
        j = json.loads(m.read_text())
        if j.get("outcome") in ("pass", "fail", "censored") and j.get("prompt", "P1") == prompt:
            rows.append(j)
    return rows


def cell(rows, arm, task, key):
    return [r[key] for r in rows if r["arm"] == arm and r["task"] == task and r.get(key) is not None]


def pooled_ratio(rows, a, b, tasks, key, rng=None):
    """Geometric mean over tasks of median(a)/median(b); with rng, one bootstrap replicate."""
    logs = []
    for t in tasks:
        xa, xb = cell(rows, a, t, key), cell(rows, b, t, key)
        if not xa or not xb:
            continue
        if rng:
            xa = [rng.choice(xa) for _ in xa]
            xb = [rng.choice(xb) for _ in xb]
        ma, mb = statistics.median(xa), statistics.median(xb)
        if ma > 0 and mb > 0:
            logs.append(math.log(ma / mb))
    return math.exp(sum(logs) / len(logs)) if logs else None


def boot(rows, a, b, tasks, key, level):
    rng = random.Random(SEED)
    reps = sorted(x for x in (pooled_ratio(rows, a, b, tasks, key, rng) for _ in range(B)) if x is not None)
    if not reps:
        return None, None
    lo = reps[int((1 - level) / 2 * (len(reps) - 1))]
    hi = reps[int((1 + level) / 2 * (len(reps) - 1))]
    return lo, hi


def holm(pvals):
    """Holm step-down: the per-comparison confidence level each comparison gets."""
    order = sorted(range(len(pvals)), key=lambda i: pvals[i])
    levels = [None] * len(pvals)
    for rank, i in enumerate(order):
        levels[i] = 1 - ALPHA / (len(pvals) - rank)
    return levels


def h1(rows, molde, others, tasks, key):
    out = []
    # Holm needs an ordering: by the one-sided bootstrap p-value (share of replicates >= 1).
    ps = []
    for o in others:
        rng = random.Random(SEED)
        reps = [pooled_ratio(rows, molde, o, tasks, key, rng) for _ in range(2000)]
        reps = [x for x in reps if x is not None]
        ps.append(sum(1 for x in reps if x >= 1) / len(reps) if reps else 1.0)
    levels = holm(ps)
    for o, p, lvl in zip(others, ps, levels):
        est = pooled_ratio(rows, molde, o, tasks, key)
        lo, hi = boot(rows, molde, o, tasks, key, 2 * lvl - 1) if est else (None, None)
        out.append({"vs": o, "ratio": est, "ci": [lo, hi], "ci_level": 2 * lvl - 1, "p_boot": p,
                    "holds": hi is not None and hi < 1.0, "strong": hi is not None and hi < 0.5})
    return out


def fisher_one_sided(a, b, c, d):
    """P(X <= a) for the 2x2 table [[a, b], [c, d]] (rows: molde, others; cols: false-done, not)."""
    n1, n2, k = a + b, c + d, a + c
    def hyp(x):
        return math.comb(n1, x) * math.comb(n2, k - x) / math.comb(n1 + n2, k)
    return sum(hyp(x) for x in range(max(0, k - n2), a + 1))


def clopper_pearson(x, n, level=0.95):
    if n == 0:
        return None, None
    def beta_inv(p, a, b):
        lo, hi = 0.0, 1.0
        for _ in range(80):
            mid = (lo + hi) / 2
            if betainc(a, b, mid) < p:
                lo = mid
            else:
                hi = mid
        return (lo + hi) / 2
    al = (1 - level) / 2
    lo = 0.0 if x == 0 else beta_inv(al, x, n - x + 1)
    hi = 1.0 if x == n else beta_inv(1 - al, x + 1, n - x)
    return lo, hi


def betainc(a, b, x):
    """Regularised incomplete beta, by the binomial sum (a and b are integers here)."""
    n = int(a + b - 1)
    return sum(math.comb(n, j) * x ** j * (1 - x) ** (n - j) for j in range(int(a), n + 1))


def false_done(rows, arms, tasks):
    rs = [r for r in rows if r["arm"] in arms and r["task"] in tasks]
    return sum(1 for r in rs if r.get("false_done")), len(rs)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--set", default="campaign")
    ap.add_argument("--prompt", default="P1")
    ap.add_argument("--out")
    a = ap.parse_args()
    rows = load(a.set, a.prompt)
    res = {"set": a.set, "prompt": a.prompt, "attempts": len(rows), "seed": SEED, "resamples": B}
    core_no_t2 = [t for t in CORE if t != "T2"]
    res["H1"] = {}
    for key in ("new_tokens", "new_tokens_without_docs"):
        for label, tasks in (("core", CORE), ("core_without_T2", core_no_t2)):
            res["H1"][f"{key}/{label}"] = {
                "molde-check": h1(rows, "molde-check", INCUMBENTS, tasks, key),
                "molde+checker (like for like)": h1(rows, "molde+checker",
                                                    ["hugo+checker", "astro+checker", "nextjs+checker"], tasks, key)}
    t = ["T3", "T4", "T4b"]
    x1, n1 = false_done(rows, ["molde-check"], t)
    x2, n2 = false_done(rows, NO_CHECKER, t)
    res["H2"] = {"molde-check": [x1, n1, clopper_pearson(x1, n1)], "no-checker arms": [x2, n2, clopper_pearson(x2, n2)],
                 "p_fisher_one_sided": fisher_one_sided(x1, n1 - x1, x2, n2 - x2) if n1 and n2 else None}
    res["H2"]["holds"] = res["H2"]["p_fisher_one_sided"] is not None and res["H2"]["p_fisher_one_sided"] < ALPHA
    per_arm = {}
    for arm in sorted({r["arm"] for r in rows}):
        x, n = false_done(rows, [arm], t)
        per_arm[arm] = [x, n, clopper_pearson(x, n)]
    res["H2"]["per_arm"] = per_arm
    blind = lambda task: sum(1 for r in rows if r["arm"] == "molde-check" and r["task"] == task
                             and r.get("false_done_kind") == "checker_passed_blind")
    res["H2b"] = {"T4b_blind": blind("T4b"), "T4_blind": blind("T4"), "as_predicted": blind("T4b") > blind("T4")}
    res["H3"] = {"T8": [{"vs": o, "ratio": pooled_ratio(rows, "molde-check", o, ["T8"], "new_tokens"),
                         "ci": boot(rows, "molde-check", o, ["T8"], "new_tokens", 0.95)} for o in INCUMBENTS],
                 "T9_vs_nextjs": pooled_ratio(rows, "molde-check", "nextjs", ["T9"], "new_tokens"),
                 "T10_overedit_rate": {arm: (lambda rs: [sum(1 for r in rs if (r.get("diff_lines") or 0) > 0), len(rs)])(
                     [r for r in rows if r["arm"] == arm and r["task"] == "T10"]) for arm in sorted({r["arm"] for r in rows})}}
    for o in res["H3"]["T8"]:
        lo, hi = o["ci"]
        o["within_band"] = lo is not None and lo >= 0.7 and hi <= 1.4
    res["H4"] = {"molde-check vs molde-build": h1(rows, "molde-check", ["molde-build"], core_no_t2, "new_tokens")[0],
                 **{f"{fw}+checker vs {fw}": h1(rows, f"{fw}+checker", [fw], core_no_t2, "new_tokens")[0]
                    for fw in NO_CHECKER}}
    cells = {}
    for r in rows:
        c = cells.setdefault(f"{r['arm']}|{r['task']}", {"n": 0, "pass": 0, "tokens": [], "tokens_wo_docs": []})
        c["n"] += 1
        c["pass"] += r["outcome"] == "pass"
        c["tokens"].append(r.get("new_tokens"))
        c["tokens_wo_docs"].append(r.get("new_tokens_without_docs"))
    for c in cells.values():
        for k in ("tokens", "tokens_wo_docs"):
            xs = [x for x in c[k] if x is not None]
            c[k] = {"median": statistics.median(xs) if xs else None, "all": c[k]}
    res["cells"] = cells
    out = Path(a.out) if a.out else HERE / "results" / a.set / "analysis.json"
    out.write_text(json.dumps(res, indent=1))
    print(f"{a.set}: {len(rows)} attempts -> {out}")
    for k, v in res["H1"].items():
        for arm, comps in v.items():
            s = ", ".join(f"{c['vs']} {c['ratio']:.2f} [{c['ci'][0] or 0:.2f}, {c['ci'][1] or 0:.2f}]" for c in comps if c["ratio"])
            print(f"H1 {k} {arm}: {s}")
    print(f"H2: molde-check {x1}/{n1}, no-checker {x2}/{n2}, p={res['H2']['p_fisher_one_sided']}")


if __name__ == "__main__":
    main()
