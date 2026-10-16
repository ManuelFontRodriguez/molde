#!/usr/bin/env python3
"""Show that every planted defect is real (runs in the judge container, bench mounted
at /bench):

    python3 /bench/agent-cost/runner/verify_plants.py PORT [T4 T4b]

For each task: stage the port, apply the plants, build with the judge, and report
 - which oracle rules / task assertions fail (must be exactly the planted categories:
   "task_*" categories count as task assertions);
 - for the molde port, what `molde check` itself reports, i.e. whether the agent of the
   molde-check arm would be shown each defect (T4: all, by design; T4b: none, by design);
 - for every port, what site-check (the +checker arms' tool) reports.
"""
import json
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, "/bench/agent-cost/runner")
sys.path.insert(0, "/bench/agent-cost/oracle")
import oracle  # noqa: E402
from ports import PORTS, stage  # noqa: E402
from setup import setup  # noqa: E402

BENCH = Path("/bench")
DATA = BENCH / "cross-framework/harness/data.json"


def main():
    port = sys.argv[1]
    tasks = sys.argv[2:] or ["T4", "T4b"]
    recipe = PORTS[port]
    ok_all = True
    for task in tasks:
        with tempfile.TemporaryDirectory() as t:
            ws = Path(t) / "ws"
            stage(port, BENCH, ws, DATA)
            cats = setup(port, task, ws, recipe["data"])
            r = subprocess.run(recipe["build"], shell=True, cwd=ws, capture_output=True, text=True)
            data = json.loads((ws / recipe["data"]).read_text())
            v = oracle.judge(ws / recipe["out"], task, data, r.returncode == 0)
            failing = {k for k, x in v["stated"].items() if not x["ok"]}
            if not v["task"]["ok"]:
                failing.add("task")
            planted = {("task" if c.startswith("task") else c) for c in cats}
            # Every planted rule must fail, and nothing else may, except task assertions
            # that fail as a consequence (a broken link also breaks "the index links every
            # stock"; a double escape also breaks "the page shows its name").
            exact = planted <= failing <= planted | {"task"}
            ok_all &= exact
            print(f"{port} {task}: planted {sorted(planted)}; oracle fails {sorted(failing)} -> {'OK' if exact else 'MISMATCH'}")
            for k, x in v["stated"].items():
                for p in x["problems"][:3]:
                    print(f"    {k}: {p}")
            for p in v["task"]["problems"][:6]:
                print(f"    task: {p}")
            if port == "molde":
                c = subprocess.run("/opt/molde-real/molde check --site . --out /tmp/mc-out", shell=True, cwd=ws,
                                   capture_output=True, text=True)
                try:
                    rep = json.loads(c.stdout)
                    ids = sorted({e.get("check") for e in rep.get("errors", [])})
                    verdict = rep.get("verdict")
                except ValueError:
                    ids = ["<no JSON report>", c.stdout[:200], c.stderr[:200]]
                    verdict = "?"
                print(f"    molde check -> verdict {verdict}; check ids {ids}")
            sc = subprocess.run(f"python3 /bench/agent-cost/checker/site-check {ws / recipe['out']}", shell=True,
                                capture_output=True, text=True)
            try:
                srep = json.loads(sc.stdout)
                sids = sorted({e.get("check") for e in srep.get("errors", [])})
                print(f"    site-check -> verdict {srep.get('verdict')}; check ids {sids}")
            except ValueError:
                print(f"    site-check -> no JSON: {sc.stdout[:200]} {sc.stderr[:200]}")
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
