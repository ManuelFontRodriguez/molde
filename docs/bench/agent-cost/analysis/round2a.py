#!/usr/bin/env python3
"""The registered analysis of round 2A (ROUND-2A.md), from the runs' metrics.json alone.

    python3 analysis/round2a.py [--set round2a]

A1: molde-check-guide / molde-check, new tokens, core tasks, 95% bootstrap.
A2: whole-reference reads by molde-check-guide on T0, T4, T4b, T10 (< 20%).
A3: molde-check-guide / hugo, astro, nextjs, new tokens, Holm over the three.
A4: A3 with each run's total_cost_usd.
Uses analyze.py's pooled ratio and bootstrap (seed 1016, 10,000 resamples) so the
numbers are computed exactly as round 1's.
"""
import argparse
import json
import sys
from pathlib import Path

AC = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(AC))
sys.path.insert(0, str(AC / "analysis"))
import analyze as A  # noqa: E402
import secondary as S  # noqa: E402

CORE = A.CORE
GUIDE, PLAIN = "molde-check-guide", "molde-check"
INCUMBENTS = ["hugo", "astro", "nextjs"]


def load(set_name):
    rows = A.load(set_name, "P1")
    for r in rows:
        r["cost_usd"] = (r.get("result") or {}).get("total_cost_usd")
        r["_dir"] = AC / "results" / set_name / r["run_id"]
    return rows


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--set", default="round2a")
    a = ap.parse_args()
    rows = load(a.set)
    print(f"{a.set}: {len(rows)} attempts")
    out = {"set": a.set, "attempts": len(rows)}

    est = A.pooled_ratio(rows, GUIDE, PLAIN, CORE, "new_tokens")
    lo, hi = A.boot(rows, GUIDE, PLAIN, CORE, "new_tokens", 0.95)
    out["A1"] = {"ratio": est, "ci95": [lo, hi], "holds": hi is not None and hi < 1.0}
    print(f"A1 {GUIDE} / {PLAIN}: {est:.2f} [{lo:.2f}, {hi:.2f}] holds={out['A1']['holds']}")

    whole = n = 0
    for r in rows:
        if r["arm"] == GUIDE and r["task"] in ("T0", "T4", "T4b", "T10"):
            n += 1
            whole += S.reads_whole_reference(r["_dir"])
    out["A2"] = {"whole_reads": whole, "runs": n, "holds": n > 0 and whole / n < 0.2}
    print(f"A2 whole-reference reads on T0/T4/T4b/T10: {whole}/{n} holds={out['A2']['holds']}")

    for label, key in (("A3", "new_tokens"), ("A4", "cost_usd")):
        res = A.h1(rows, GUIDE, INCUMBENTS, CORE, key)
        out[label] = res
        print(f"{label} ({key}): " + "  ".join(
            f"{x['vs']} {x['ratio']:.2f} [{x['ci'][0]:.2f}, {x['ci'][1]:.2f}] holds={x['holds']}" for x in res))

    out["reported"] = {
        "plain_vs_incumbents_now": {b: A.pooled_ratio(rows, PLAIN, b, CORE, "new_tokens") for b in INCUMBENTS},
        "docs_tokens_median": {arm: {t: S.med(S.cell(rows, arm, t, "docs_tokens_est"))
                                     for t in CORE + ["T0", "T10"]} for arm in (GUIDE, PLAIN)},
        "new_tokens_median": {arm: {t: S.med(S.cell(rows, arm, t, "new_tokens")) for t in CORE + ["T0", "T10"]}
                              for arm in [GUIDE, PLAIN] + INCUMBENTS},
        "pass": {arm: sum(r["outcome"] == "pass" for r in rows if r["arm"] == arm) for arm in [GUIDE, PLAIN] + INCUMBENTS},
    }
    print("drift check, molde-check / incumbents now:",
          {b: round(v, 2) for b, v in out["reported"]["plain_vs_incumbents_now"].items() if v})
    (AC / "results" / a.set / "analysis.json").write_text(json.dumps(out, indent=1, default=str))


if __name__ == "__main__":
    main()
