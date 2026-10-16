#!/usr/bin/env python3
"""Judge one workspace in a single process: build_site.py, then grade.py. For the
validation scripts (verify_solutions.py, validate_ports.sh), which judge known material
only. Real runs use the two steps in two containers (runner/run_one.py), so the agent's
build code never runs where the oracle is mounted (review 2 I5).

    judge.py --port PORT --task TASK --workspace WS --out OUT
"""
import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", required=True)
    ap.add_argument("--task", required=True)
    ap.add_argument("--workspace", required=True)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    with tempfile.TemporaryDirectory() as built:
        r = subprocess.run([sys.executable, str(HERE / "build_site.py"), "--port", a.port, "--task", a.task,
                            "--workspace", a.workspace, "--out", built], capture_output=True, text=True)
        if r.returncode:
            sys.exit(f"build step failed: {r.stdout}{r.stderr}")
        r = subprocess.run([sys.executable, str(HERE / "grade.py"), "--built", built, "--task", a.task,
                            "--out", a.out], capture_output=True, text=True)
        print(r.stdout.strip() or r.stderr.strip())
        sys.exit(r.returncode)


if __name__ == "__main__":
    main()
