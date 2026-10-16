#!/usr/bin/env python3
"""Task validation, the positive half: a known-correct solution to every task must pass
the whole judge (rebuild, oracle, data-mutation test). Runs in the judge container:

    python3 /bench/agent-cost/runner/verify_solutions.py PORT [TASK ...]

Solutions live in agent-cost/solutions/<port-module>.py (judge-side only).
"""
import importlib.util
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

BENCH = Path("/bench")
sys.path.insert(0, str(BENCH / "agent-cost/runner"))
from ports import PORTS, copy_deps, stage  # noqa: E402
from setup import setup  # noqa: E402

DATA = BENCH / "cross-framework/harness/data.json"


def load(port):
    name = port.replace("-", "_")
    spec = importlib.util.spec_from_file_location(name, BENCH / f"agent-cost/solutions/{name}.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod.SOLUTIONS, getattr(mod, "EXPECTED_FAIL", {})


def slug(s):
    return re.sub(r"[^a-z0-9]+", "-", s.lower()).strip("-")


def apply(ws, edits, data_rel):
    for rel, old, new in edits:
        p = ws / rel
        if old == "@SECTORS":                       # molde T2: a sectors array in the data
            d = json.loads(p.read_text())
            names = list(dict.fromkeys(s["sector"] for s in d["stocks"]))
            d["sectors"] = [{"name": n, "slug": slug(n)} for n in names]
            p.write_text(json.dumps(d, indent=1, ensure_ascii=False))
            continue
        if new == "@MOVE style.css":
            p.parent.mkdir(parents=True, exist_ok=True)
            shutil.move(str(ws / "style.css"), p)
            continue
        if new == "@DELETE":                        # a file the solution removes (a page moved)
            p.unlink()
            continue
        if old is None:
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(new)
            continue
        s = p.read_text()
        if s.count(old) != 1:
            raise SystemExit(f"solution anchor matches {s.count(old)} times in {rel}: {old!r}")
        p.write_text(s.replace(old, new))


def main():
    port = sys.argv[1]
    sols, expected_fail = load(port)
    tasks = sys.argv[2:] or list(sols)
    recipe = PORTS[port]
    bad = 0
    for task in tasks:
        with tempfile.TemporaryDirectory() as t:
            ws = Path(t) / "ws"
            if task == "T0":
                ws.mkdir()
                shutil.copy(DATA, ws / "data.json")
                shutil.copy(BENCH / "cross-framework/harness/golden/style.css", ws / "style.css")
                if recipe["node_deps"]:
                    copy_deps(ws, recipe["node_deps"])
            else:
                stage(port, BENCH, ws, DATA)
                setup(port, task.split("-")[0], ws, recipe["data"])
            g = ["git", "-c", "user.name=bench", "-c", "user.email=bench@localhost"]
            subprocess.run(["git", "init", "-q"], cwd=ws, check=True)
            (ws / ".gitignore").write_text("node_modules/\nout/\n.next/\n.judge-out/\n")
            subprocess.run(g + ["add", "-A"], cwd=ws, check=True)
            subprocess.run(g + ["commit", "-qm", "start"], cwd=ws, check=True)
            apply(ws, sols[task], recipe["data"])
            jtask = task.split("-")[0]       # "T10-overedit" is judged as T10
            r = subprocess.run([sys.executable, str(BENCH / "agent-cost/judge/judge.py"), "--port", port, "--task", jtask,
                                "--workspace", str(ws), "--out", f"{t}/v"], capture_output=True, text=True)
            v = json.loads(Path(f"{t}/v/verdict.json").read_text()) if Path(f"{t}/v/verdict.json").exists() else None
            passed = bool(v and v["pass"])
            ok = passed != (task in expected_fail)
            bad += not ok
            label = "PASS" if passed else "FAIL"
            if task in expected_fail:
                label += f" (expected to fail: {expected_fail[task][:90]}...)" if not passed else " (UNEXPECTED: was expected to fail)"
            print(f"{port} {task}: {label}")
            if not ok:
                print(r.stdout[-500:], r.stderr[-1500:])
                if v:
                    subprocess.run([sys.executable, str(BENCH / "agent-cost/judge/summarize.py"), f"{t}/v/verdict.json"])
    print(f"{len(tasks) - bad}/{len(tasks)} as expected")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
