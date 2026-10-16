#!/usr/bin/env python3
"""Judge, step 1 of 2: build. Runs the agent's build code, so it runs in a container
that has NO benchmark material (no oracle, solutions, golden or other ports): only the
workspace (read-only), the image's toolchains, this file and runner/ports.py. Review 2 I5.

    build_site.py --port PORT --task TASK --workspace /in/ws --out /built

Produces in /built:
  final/      the build of the agent's final workspace
  mutated/    the build after the data-mutation test (DESIGN.md §6.3)
  start/      the build of the workspace as handed to the agent (T10 only)
  build.json  logs, exit codes, the data used, and the data-integrity findings

The data the site is graded against is the REFERENCE data: the data file as it was in
the workspace's first commit (review 2 B2). The agent may add keys (for example a
`sectors` array) but may not remove or change what was there: an agent that "fixes" a
page by deleting its row fails requirement 8.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "runner"))
# The workspace belongs to the agent's user; this step may run as another user. git's
# ownership check is lifted for every git process, clone's helpers included.
os.environ.update({"GIT_CONFIG_COUNT": "1", "GIT_CONFIG_KEY_0": "safe.directory", "GIT_CONFIG_VALUE_0": "*"})
from ports import PORTS, copy_deps  # noqa: E402


def sh(cmd, cwd, timeout=900):
    try:
        r = subprocess.run(cmd, shell=True, cwd=cwd, capture_output=True, text=True, timeout=timeout)
        return r.returncode, (r.stdout[-4000:] + r.stderr[-4000:])
    except subprocess.TimeoutExpired:
        return 124, f"timed out after {timeout}s"


def subsumes(agent, ref, path="$"):
    """Problems where the agent's data no longer contains the reference data."""
    probs = []
    if isinstance(ref, dict):
        if not isinstance(agent, dict):
            return [f"{path}: was an object, now {type(agent).__name__}"]
        for k, v in ref.items():
            if k not in agent:
                probs.append(f"{path}.{k}: removed")
            else:
                probs += subsumes(agent[k], v, f"{path}.{k}")
    elif isinstance(ref, list):
        if not isinstance(agent, list) or len(agent) != len(ref):
            return [f"{path}: list changed length ({len(ref)} -> {len(agent) if isinstance(agent, list) else 'not a list'})"]
        for i, (a, r) in enumerate(zip(agent, ref)):
            probs += subsumes(a, r, f"{path}[{i}]")
    elif agent != ref:
        probs.append(f"{path}: changed from {ref!r} to {agent!r}")
    return probs


def find_data(ws, recipe_rel, ref):
    """Where the agent's site keeps its data: the port's usual file, else data.json at the
    root, else any JSON file (outside dependency and output folders) that still contains
    the reference data. Review 2 I10."""
    cands = [ws / recipe_rel, ws / "data.json"]
    skip = {"node_modules", ".git", "out", ".next", "dist", "_site", "public", ".judge-out", ".astro"}
    cands += sorted(p for p in ws.rglob("*.json") if not (set(p.relative_to(ws).parts) & skip))
    for p in cands:
        try:
            d = json.loads(p.read_text())
        except (OSError, ValueError):
            continue
        if isinstance(d, dict) and not subsumes(d, ref):
            return p, d
    return None, None


def mutate(data, ref):
    """The data-mutation test: a renamed stock (with & and <), a new cycle id, a new stock
    at the front with a digit in its ticker, and a new stock at the end in a new sector.
    New rows carry only the fields of the original data, as rows from the upstream source
    would: a field the agent added by hand (a slug, say) is not filled in for them."""
    d = json.loads(json.dumps(data))
    d["stocks"][0]["name"] = "Zeta <Z> & Co"
    if "cycle_id" in d:
        d["cycle_id"] = "2026-12-31T09:00:00Z-mutated"
    keys = set(ref["stocks"][-1])
    tmpl = {k: v for k, v in d["stocks"][-1].items() if k in keys}
    first = dict(tmpl, ticker="A1", name="Alpha One Inc", sector=d["stocks"][0]["sector"])
    last = dict(tmpl, ticker="ZZZ", name="Zed Mutation Inc", sector="Utilities")
    for s in (first, last):
        if "dividend_yield" in s:
            s["dividend_yield"] = "9.99%" if s is last else "1.01%"
    d["stocks"].insert(0, first)
    d["stocks"].append(last)
    return d


def build(src, recipe, scratch, data_file=None, data=None):
    ws = scratch / "ws"
    shutil.copytree(src, ws, symlinks=True, ignore=shutil.ignore_patterns("node_modules", ".git"))
    if recipe["node_deps"]:
        copy_deps(ws, recipe["node_deps"])
    if data is not None:
        (ws / data_file).write_text(json.dumps(data, indent=1, ensure_ascii=False))
    code, log = sh(recipe["build"], ws)
    return ws / recipe["out"], code == 0, log


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", required=True)
    ap.add_argument("--task", required=True)
    ap.add_argument("--workspace", required=True)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    recipe = PORTS[a.port]
    src, out = Path(a.workspace), Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    scratch = Path("/tmp/judge-build")
    shutil.rmtree(scratch, ignore_errors=True)
    scratch.mkdir()
    rec = {"port": a.port, "task": a.task}

    # the reference data and the starting workspace, from the first commit
    start = scratch / "start-src"
    # copy git's metadata out of the read-only, agent-owned workspace first (git refuses to
    # read a repository owned by another user), then extract the first commit from the copy
    gitdir = scratch / "git"
    shutil.copytree(src / ".git", gitdir)
    first = subprocess.run(["git", "--git-dir", str(gitdir), "rev-list", "--max-parents=0", "HEAD"],
                           capture_output=True, text=True, check=True).stdout.split()[-1]
    start.mkdir()
    subprocess.run(f"git --git-dir {gitdir} archive {first} | tar -x -C {start}", shell=True, check=True)
    ref_file = recipe["data"] if (start / recipe["data"]).exists() else "data.json"
    ref = json.loads((start / ref_file).read_text())
    rec["reference_data_file"] = ref_file
    rec["reference_data"] = ref

    data_path, agent_data = find_data(src, recipe["data"], ref)
    if data_path is None:
        rec["integrity"] = ["no data file in the workspace still contains the data the site was given "
                            "(rows removed or values changed)"]
        p = src / recipe["data"]
        if not p.exists():
            p = src / "data.json"
        try:
            agent_data = json.loads(p.read_text())
            rec["integrity"] += subsumes(agent_data, ref)[:20]
            data_path = p
        except (OSError, ValueError) as e:
            rec["integrity"].append(f"no readable data file: {e}")
    else:
        rec["integrity"] = []
    rec["data_file"] = data_path.relative_to(src).as_posix() if data_path else None

    built, ok, log = build(src, recipe, scratch / "final")
    rec["final"] = {"ok": ok, "log": log}
    if built.is_dir():
        shutil.copytree(built, out / "final", symlinks=False)
    if agent_data is not None and data_path is not None:
        m = mutate(agent_data, ref)
        rec["mutated_data"] = m
        built, ok, log = build(src, recipe, scratch / "mutated", rec["data_file"], m)
        rec["mutated"] = {"ok": ok, "log": log}
        if built.is_dir():
            shutil.copytree(built, out / "mutated", symlinks=False)
    if a.task == "T10":
        built, ok, log = build(start, recipe, scratch / "start")
        rec["start"] = {"ok": ok, "log": log}
        if built.is_dir():
            shutil.copytree(built, out / "start", symlinks=False)
    (out / "build.json").write_text(json.dumps(rec, indent=1, ensure_ascii=False))
    print(json.dumps({"final_ok": rec["final"]["ok"], "mutated_ok": rec.get("mutated", {}).get("ok"),
                      "integrity": len(rec["integrity"])}))


if __name__ == "__main__":
    main()
