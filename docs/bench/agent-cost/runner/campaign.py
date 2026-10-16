#!/usr/bin/env python3
"""Run a set of benchmark runs, one at a time, safely against a subscription's limits.

    python3 runner/campaign.py plan  --set pilot --arms A,B --tasks T3,T4 --reps 3 --seed 20261016
    python3 runner/campaign.py plan  --set campaign --seed 20261016 --spec "A,B|T1,T2|P1|5" --spec ...
    python3 runner/campaign.py run   --set pilot [--max-runs N] [--dry-run] [run_one options]
    python3 runner/campaign.py status --set pilot

`plan` writes results/<set>/plan.json: every (arm, task, prompt, rep), in an order shuffled
by the seed, so arms are interleaved in time (drift in the service hits every arm alike).
The plan is written once; `plan` refuses to overwrite it.

`run` works through the plan. It is safe to stop and start again at any time:

- **Ledger.** results/<set>/ledger.jsonl gets one line per finished run (key, run_id,
  outcome). A key with a non-infrastructure outcome is done and never run again. A key
  whose runs were all infrastructure failures is retried, at most --max-infra-retries times.
- **Usage limits.** Before each run, the last `rate_limit_last` seen (Claude Code's own
  report of the subscription's five-hour and seven-day windows) is checked. Above
  --five-hour-max the runner sleeps until that window resets; above --seven-day-max it
  stops and says when the window resets. A run that ended on a usage limit (metrics
  `limit_hit`) makes it wait for the reset too. Nothing is fired into a known limit.
- **Circuit breaker.** --max-consecutive-infra infrastructure failures in a row stop the
  campaign: something is wrong with the setup, and more runs would only spend quota.
- **Leftovers.** At start, containers and volumes left by a killed run (molde-bench-agent-*,
  molde-bench-ws-*, molde-bench-built-*) are removed.
"""
import argparse
import datetime as dt
import json
import random
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
RESULTS = HERE.parent / "results"


def now():
    return dt.datetime.now(dt.timezone.utc)


def log(msg):
    print(f"{now():%Y-%m-%d %H:%M:%SZ} {msg}", flush=True)


def key_of(r):
    return f"{r['arm']}|{r['task']}|{r['prompt']}|r{r['rep']}"


def cmd_plan(a):
    d = RESULTS / a.set
    d.mkdir(parents=True, exist_ok=True)
    p = d / "plan.json"
    if p.exists():
        sys.exit(f"{p} exists; a plan is written once (delete it by hand only if no run has used it)")
    sys.path.insert(0, str(HERE))
    from arms import ARMS
    # groups: each "arms|tasks|prompts|reps"; or the single group given by --arms/--tasks/--reps
    groups = [g.split("|") for g in (a.spec or [])]
    if a.arms:
        groups.append([a.arms, a.tasks, a.prompts, str(a.reps)])
    runs, seen = [], set()
    tasks_dir = HERE.parent / "tasks"
    for arms_s, tasks_s, prompts_s, reps_s in groups:
        arms, tasks, prompts = arms_s.split(","), tasks_s.split(","), prompts_s.split(",")
        unknown = [x for x in arms if x not in ARMS]
        if unknown:
            sys.exit(f"unknown arms: {unknown}")
        missing = [t for t in tasks if t != "factory" and not (tasks_dir / f"{t}.md").exists()]
        if missing:
            sys.exit(f"no task file for: {missing}")
        for x in arms:
            for t in tasks:
                for pr in prompts:
                    for n in range(1, int(reps_s) + 1):
                        r = {"arm": x, "task": t, "prompt": pr, "rep": n}
                        if key_of(r) in seen:
                            sys.exit(f"run listed twice: {key_of(r)}")
                        seen.add(key_of(r))
                        runs.append(r)
    random.Random(a.seed).shuffle(runs)
    p.write_text(json.dumps({"set": a.set, "seed": a.seed, "created_utc": now().isoformat(),
                             "runs": runs}, indent=1))
    log(f"wrote {p}: {len(runs)} runs")


def read_ledger(d):
    rows = []
    lp = d / "ledger.jsonl"
    if lp.exists():
        for line in lp.read_text().splitlines():
            if line.strip():
                rows.append(json.loads(line))
    return rows


def last_rate_limit(d):
    """The newest rate_limit_last among this set's runs (and the smoke/dev sets as a fallback)."""
    best = None
    for s in (d, RESULTS / "smoke", RESULTS / "dev", RESULTS / "factory"):
        for m in s.glob("*/metrics.json"):
            try:
                j = json.loads(m.read_text())
            except ValueError:
                continue
            rl = j.get("rate_limit_last")
            if rl and (best is None or m.stat().st_mtime > best[0]):
                best = (m.stat().st_mtime, rl)
    return best[1] if best else None


def windows(rl):
    w = (rl or {}).get("unifiedWindows") or {}
    return {k: (v.get("utilization"), v.get("resetsAt")) for k, v in w.items()}


def cleanup():
    """Remove what a killed run left behind: stopped containers and volumes no container
    uses. Refuses while any benchmark container is running (another run is in progress),
    and never removes a running container or a volume in use."""
    running = subprocess.run(["docker", "ps", "--format", "{{.Names}}"], capture_output=True, text=True).stdout.split()
    busy = [n for n in running if n.startswith(("molde-bench-agent-", "molde-bench-built-"))]
    if busy:
        sys.exit(f"another benchmark run is in progress ({', '.join(busy)}); not starting")
    stopped = subprocess.run(["docker", "ps", "-a", "--filter", "status=exited", "--filter", "status=created",
                              "--filter", "status=dead", "--format", "{{.Names}}"],
                             capture_output=True, text=True).stdout.split()
    for n in stopped:
        if n.startswith(("molde-bench-agent-", "molde-bench-built-")):
            if subprocess.run(["docker", "rm", n], capture_output=True).returncode == 0:
                log(f"removed leftover container {n}")
    vols = subprocess.run(["docker", "volume", "ls", "--format", "{{.Name}}"], capture_output=True, text=True).stdout.split()
    for n in vols:
        if n.startswith(("molde-bench-ws-", "molde-bench-built-")):
            # no -f: docker refuses to remove a volume a container still uses
            if subprocess.run(["docker", "volume", "rm", n], capture_output=True).returncode == 0:
                log(f"removed leftover volume {n}")


def wait_until(epoch, why):
    secs = max(0, epoch - time.time()) + 120
    log(f"{why}: sleeping {secs / 60:.0f} min, until {dt.datetime.fromtimestamp(epoch + 120, dt.timezone.utc):%Y-%m-%d %H:%M}Z")
    time.sleep(secs)


def cmd_status(a):
    d = RESULTS / a.set
    plan = json.loads((d / "plan.json").read_text())["runs"]
    led = read_ledger(d)
    done = {r["key"] for r in led if r["outcome"] != "infra_fail"}
    by = {}
    for r in led:
        by[r["outcome"]] = by.get(r["outcome"], 0) + 1
    print(f"{a.set}: {len(done)}/{len(plan)} done; ledger outcomes {by}")
    for k, (u, t) in windows(last_rate_limit(d)).items():
        when = dt.datetime.fromtimestamp(t, dt.timezone.utc).strftime("%a %H:%MZ") if t else "?"
        print(f"  window {k}: {u if u is not None else '?'} used, resets {when}")


def cmd_run(a, passthrough):
    d = RESULTS / a.set
    plan = json.loads((d / "plan.json").read_text())["runs"]
    cleanup()
    consecutive_infra, started = 0, 0
    for r in plan:
        k = key_of(r)
        led = [x for x in read_ledger(d) if x["key"] == k]
        if any(x["outcome"] != "infra_fail" for x in led):
            continue
        if len(led) > a.max_infra_retries:
            log(f"skip {k}: {len(led)} infrastructure failures already")
            continue
        if a.max_runs is not None and started >= a.max_runs:
            log(f"stopping: --max-runs {a.max_runs} reached")
            break
        w = windows(last_rate_limit(d))
        u7, t7 = w.get("seven_day", (None, None))
        if u7 is not None and u7 >= a.seven_day_max:
            when = dt.datetime.fromtimestamp(t7, dt.timezone.utc).strftime("%Y-%m-%d %H:%MZ") if t7 else "?"
            log(f"stopping: seven-day window at {u7:.0%} (max {a.seven_day_max:.0%}); it resets {when}")
            return 3
        u5, t5 = w.get("five_hour", (None, None))
        if u5 is not None and u5 >= a.five_hour_max and t5 and t5 > time.time():
            wait_until(t5, f"five-hour window at {u5:.0%}")
        started += 1
        log(f"run {started}: {k}")
        cmd = [sys.executable, str(HERE / "run_one.py"), "--arm", r["arm"], "--task", r["task"],
               "--prompt", r["prompt"], "--rep", str(r["rep"]), "--set", a.set, *passthrough]
        if a.dry_run:
            cmd.append("--dry-run")
        before = set(p.name for p in d.iterdir() if p.is_dir())
        p = subprocess.run(cmd, capture_output=True, text=True)
        new = [x for x in d.iterdir() if x.is_dir() and x.name not in before]
        run_dir = new[0] if new else None
        m = {}
        if run_dir and (run_dir / "metrics.json").exists():
            m = json.loads((run_dir / "metrics.json").read_text())
        outcome = m.get("outcome") or "infra_fail"
        row = {"key": k, "run_id": run_dir.name if run_dir else None, "outcome": outcome,
               "pass": m.get("pass"), "new_tokens": m.get("new_tokens"), "wall_s": m.get("wall_s"),
               "infra_reasons": m.get("infra_reasons") or ([p.stderr[-300:]] if not m else []),
               "finished_utc": now().isoformat()}
        with open(d / "ledger.jsonl", "a") as f:
            f.write(json.dumps(row) + "\n")
        log(f"  -> {outcome} pass={m.get('pass')} tokens={m.get('new_tokens')} wall={m.get('wall_s')}s")
        if m.get("problems"):
            log(f"  problems: {m['problems']}")
        if outcome == "infra_fail" and not a.dry_run:
            consecutive_infra += 1
            if m.get("limit_hit"):
                rl = m.get("rate_limit_last") or last_rate_limit(d)
                resets = [t for _, t in windows(rl).values() if t and t > time.time()]
                if resets:
                    wait_until(min(resets), "usage limit hit")
                    consecutive_infra = 0
                    continue
                log("stopping: usage limit hit and no reset time known")
                return 3
            if consecutive_infra >= a.max_consecutive_infra:
                log(f"stopping: {consecutive_infra} infrastructure failures in a row; last: {row['infra_reasons']}")
                return 2
        else:
            consecutive_infra = 0
    log("plan finished" if a.max_runs is None or started < a.max_runs else "paused")
    return 0


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("plan")
    p.add_argument("--set", required=True)
    p.add_argument("--spec", action="append", help='a group "arms|tasks|prompts|reps"; repeatable')
    p.add_argument("--arms")
    p.add_argument("--tasks")
    p.add_argument("--prompts", default="P1")
    p.add_argument("--reps", type=int)
    p.add_argument("--seed", type=int, required=True)
    s = sub.add_parser("status")
    s.add_argument("--set", required=True)
    r = sub.add_parser("run")
    r.add_argument("--set", required=True)
    r.add_argument("--max-runs", type=int, default=None)
    r.add_argument("--dry-run", action="store_true")
    r.add_argument("--five-hour-max", type=float, default=0.85)
    r.add_argument("--seven-day-max", type=float, default=0.90)
    r.add_argument("--max-consecutive-infra", type=int, default=3)
    r.add_argument("--max-infra-retries", type=int, default=2)
    a, rest = ap.parse_known_args()
    if a.cmd == "plan":
        cmd_plan(a)
    elif a.cmd == "status":
        cmd_status(a)
    else:
        sys.exit(cmd_run(a, rest))


if __name__ == "__main__":
    main()
