#!/usr/bin/env python3
"""Prepare one run (runs in the judge image, bench at /bench read-only; never in the
agent's container):

    prep.py --arm ARM --task TASK --prompt P1|P2 --ws /ws --ctx /ctx

/ws  (a fresh Docker volume) becomes the agent's workspace: the arm's port staged with
     the dataset, the task's setup applied, committed to git (so the judge can diff).
     T0 is the exception: the workspace holds only data.json, style.css and, for Node
     arms, package.json + node_modules.
/ctx (mounted read-only into the agent's container at /work/context):
     arm.md       the arm note + requirements (--append-system-prompt-file)
     prompt.txt   the task text (+ the P2 suffix)
     bin/         the only tools added to PATH for this arm
     molde-reference.md   the docs, for molde arms only
     mcp.json     the MCP config, for the molde-mcp arm only
     manifest.json        what was prepared, with hashes
"""
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

BENCH = Path("/bench")
AC = BENCH / "agent-cost"
sys.path.insert(0, str(AC / "runner"))
from arms import ARMS, SHIM_BUILD, build_only_reference, note_text  # noqa: E402
from ports import PORTS, copy_deps, stage  # noqa: E402
from setup import setup  # noqa: E402

DATA = BENCH / "cross-framework/harness/data.json"
STYLE = BENCH / "cross-framework/harness/golden/style.css"


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--arm", required=True)
    ap.add_argument("--task", required=True)
    ap.add_argument("--prompt", default="P1", choices=("P1", "P2"))
    ap.add_argument("--ws", required=True)
    ap.add_argument("--ctx", required=True)
    ap.add_argument("--meta", required=True, help="judge-side folder for manifest.json (never mounted into the agent)")
    a = ap.parse_args()
    arm = ARMS[a.arm]
    port = arm["port"]
    ws, ctx = Path(a.ws), Path(a.ctx)
    for p in list(ws.iterdir()):
        shutil.rmtree(p) if p.is_dir() and not p.is_symlink() else p.unlink()

    # --- the workspace
    if a.task in ("T0", "factory"):
        shutil.copy(DATA, ws / "data.json")
        shutil.copy(STYLE, ws / "style.css")
        deps = PORTS[port]["node_deps"] if port in PORTS else None
        if deps:
            shutil.copy(f"/opt/node-deps/{deps}/package.json", ws / "package.json")
            shutil.copy(f"/opt/node-deps/{deps}/package-lock.json", ws / "package-lock.json")
            copy_deps(ws, deps)
        planted = []
    else:
        tmp = Path("/tmp/stage")
        shutil.rmtree(tmp, ignore_errors=True)
        stage(port, BENCH, tmp, DATA)
        for p in tmp.iterdir():
            shutil.move(str(p), ws / p.name)
        planted = setup(port, a.task, ws, PORTS[port]["data"])
    (ws / ".gitignore").write_text("node_modules/\nout/\n.next/\n.judge-out/\n")
    g = ["git", "-c", "user.name=bench", "-c", "user.email=bench@localhost"]
    subprocess.run(["git", "init", "-q"], cwd=ws, check=True)
    subprocess.run(g + ["add", "-A"], cwd=ws, check=True)
    subprocess.run(g + ["commit", "-qm", "start"], cwd=ws, check=True)

    # --- the context
    ctx.mkdir(parents=True, exist_ok=True)
    (ctx / "bin").mkdir(exist_ok=True)
    req = (AC / "requirements.md").read_text()
    (ctx / "arm.md").write_text(note_text(a.arm, req))
    if a.task == "factory":
        from factory import prompt as factory_prompt
        prompt = factory_prompt(arm["factory"], (AC / "tasks/factory.md").read_text()).rstrip("\n")
    else:
        prompt = (AC / "tasks" / f"{a.task}.md").read_text().rstrip("\n")
    if a.prompt == "P2":
        prompt += (AC / "tasks/p2-suffix.md").read_text().rstrip("\n")
    (ctx / "prompt.txt").write_text(prompt + "\n")
    for name, kind in arm["bin"].items():
        dst = ctx / "bin" / name
        # molde reaches the agent only this way, for molde arms (the agent image has none)
        if kind == "molde-full":
            shutil.copy("/opt/molde-real/molde", dst)
        elif kind == "molde-shim-build":
            shutil.copy("/opt/molde-real/molde", ctx / "bin" / ".molde")
            (ctx / "bin" / ".molde").chmod(0o755)
            dst.write_text(SHIM_BUILD)
        elif kind == "site-check":
            shutil.copy(AC / "checker/site-check", dst)
        dst.chmod(0o755)
    if arm["docs"]:
        full = (BENCH.parent / "reference.md") if (BENCH.parent / "reference.md").exists() else Path("/docs/reference.md")
        text = full.read_text()
        if arm["docs"] == "build-only":
            text = build_only_reference(text)
        (ctx / "molde-reference.md").write_text(text)
    if arm.get("mcp"):
        (ctx / "mcp.json").write_text(json.dumps({"mcpServers": {"molde": {
            "command": "/work/context/bin/molde", "args": ["mcp", "--site", "/work/site", "--out", "/work/site/out"]}}}, indent=1))
    manifest = {
        "arm": a.arm, "port": port, "task": a.task, "prompt": a.prompt, "planted": planted,
        "files": {p.relative_to(ctx).as_posix(): sha(p) for p in sorted(ctx.rglob("*")) if p.is_file()},
        "workspace_commit": subprocess.run(["git", "rev-parse", "HEAD"], cwd=ws, capture_output=True, text=True).stdout.strip(),
    }
    # Review 2 B1: the context folder is mounted into the agent's container, so it holds
    # exactly the files below and nothing else (no manifest, no plant list, no task id).
    allowed = {"arm.md", "prompt.txt", "molde-reference.md", "mcp.json"}
    found = {p.relative_to(ctx).as_posix() for p in ctx.rglob("*") if p.is_file()}
    extra = {f for f in found if f not in allowed and not f.startswith("bin/")}
    if extra:
        raise SystemExit(f"context folder holds files the agent must not see: {sorted(extra)}")
    meta = Path(a.meta)
    meta.mkdir(parents=True, exist_ok=True)
    (meta / "manifest.json").write_text(json.dumps(manifest, indent=1))
    os.system(f"chmod -R a+rX {ctx}")
    # prep runs as root (a fresh volume belongs to root); the agent is uid 1000.
    os.system(f"chown -R 1000:1000 {ws}")
    print(json.dumps({"prepared": a.arm, "task": a.task, "planted": planted}))


if __name__ == "__main__":
    main()
