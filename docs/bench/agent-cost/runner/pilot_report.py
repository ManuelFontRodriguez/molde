#!/usr/bin/env python3
"""Summarise a set's runs for calibration (the pilot is discarded from every analysis):

    python3 runner/pilot_report.py [--set pilot] [--campaign-runs 1060]

Per (arm, task): attempts, outcomes, pass rate, median and p95 of new tokens, wall time
and turns. Then what the set cost the subscription (the change in Claude Code's reported
five-hour and seven-day utilization from the first run to the last), and what a campaign
of N runs would cost at the same rate. The rate is a rough guide: utilization is reported
in whole percent, and the pilot's tasks are not the campaign's mix.
"""
import argparse
import json
import statistics
from pathlib import Path

RESULTS = Path(__file__).resolve().parent.parent / "results"


def pct(xs, q):
    xs = sorted(x for x in xs if x is not None)
    if not xs:
        return None
    return xs[min(len(xs) - 1, int(round(q * (len(xs) - 1))))]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--set", default="pilot")
    ap.add_argument("--campaign-runs", type=int, default=1060)
    a = ap.parse_args()
    runs = []
    for m in sorted((RESULTS / a.set).glob("*/metrics.json")):
        j = json.loads(m.read_text())
        meta = json.loads((m.parent / "meta.json").read_text())
        j["_started"] = meta.get("started_utc")
        runs.append(j)
    runs.sort(key=lambda j: j["_started"] or "")
    if not runs:
        print("no runs")
        return
    cells = {}
    for j in runs:
        cells.setdefault((j["arm"], j["task"]), []).append(j)
    print(f"{'arm':16} {'task':5} {'n':>2} {'pass':>4} {'fail':>4} {'cens':>4} {'infra':>5} "
          f"{'tok med':>8} {'tok p95':>8} {'wall med':>8} {'turns med':>9}")
    for (arm, task), js in sorted(cells.items()):
        out = [j["outcome"] for j in js]
        ok = [j for j in js if j["outcome"] != "infra_fail"]
        tok = [j.get("new_tokens") for j in ok]
        wall = [j.get("wall_s") for j in ok]
        turns = [(j.get("result") or {}).get("num_turns") for j in ok]
        med = lambda xs: statistics.median([x for x in xs if x is not None]) if any(x is not None for x in xs) else None
        print(f"{arm:16} {task:5} {len(js):>2} {out.count('pass'):>4} {out.count('fail'):>4} {out.count('censored'):>4} "
              f"{out.count('infra_fail'):>5} {med(tok) or 0:>8.0f} {pct(tok, .95) or 0:>8.0f} "
              f"{med(wall) or 0:>8.0f} {med(turns) or 0:>9.0f}")
    fd = [j for j in runs if j.get("false_done")]
    print(f"\nruns {len(runs)}; false-done {len(fd)}: " + ", ".join(f"{j['arm']}/{j['task']} ({j.get('false_done_kind')})" for j in fd))
    print(f"total new tokens {sum(j.get('new_tokens') or 0 for j in runs):,}; "
          f"total wall {sum(j.get('wall_s') or 0 for j in runs) / 60:.0f} min (agent only)")
    infra = [j for j in runs if j["outcome"] == "infra_fail"]
    for j in infra:
        print(f"  infra: {j['run_id']}: {j.get('infra_reasons')}")
    probs = [j for j in runs if j.get("problems")]
    for j in probs:
        print(f"  problems: {j['run_id']}: {j['problems']}")

    def util(j):
        w = ((j.get("rate_limit_last") or {}).get("unifiedWindows")) or {}
        return {k: (v.get("utilization"), v.get("resetsAt")) for k, v in w.items()}
    first, last = util(runs[0]), util(runs[-1])
    print()
    for k in ("five_hour", "seven_day"):
        if k in first and k in last:
            u0, r0 = first[k]
            u1, r1 = last[k]
            note = " (the window reset during the set; delta not meaningful)" if r0 != r1 else ""
            print(f"{k}: {u0} -> {u1}{note}")
            if r0 == r1 and u0 is not None and u1 is not None and u1 > u0:
                per = (u1 - u0) / max(1, len(runs) - 1)
                print(f"  ~{per:.4f} of the window per run -> a {a.campaign_runs}-run campaign ~ "
                      f"{per * a.campaign_runs:.1f} windows of {k.replace('_', '-')}")


if __name__ == "__main__":
    main()
