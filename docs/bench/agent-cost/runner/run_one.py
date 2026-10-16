#!/usr/bin/env python3
"""Run one (arm, task, prompt, repetition) end to end, from the Mac.

    python3 runner/run_one.py --arm molde-check --task T3 [--prompt P1] [--rep 1]
                              [--model claude-opus-5-5] [--effort high] [--set pilot]
                              [--dry-run]

Steps, each in its own container (DESIGN.md §6, §8):
 1. prep   (judge image, bench read-only): a fresh workspace volume and a context folder.
 2. agent  (agent image, internal network, egress proxy only, no bench material):
           Claude Code headless; stdout (stream-json) and stderr saved as they arrive.
 3. judge  (judge image, workspace read-only): rebuild, oracle, mutation test, lychee.
 4. archive the final workspace (without node_modules and build output) and its git diff.
 5. metrics: runner/metrics.py over the transcript and the verdict.
 6. remove the volume and the agent container.

The Claude Code token (from `claude setup-token`) is taken from CLAUDE_CODE_OAUTH_TOKEN if
set, otherwise from the macOS Keychain entry named by BENCH_KEYCHAIN_SERVICE (default
molde-bench-claude-oauth). It goes to the container on stdin, which the container moves to
fd 3 for Claude Code (CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR), so it is in no command line,
environment, file or log, and the agent's own commands can't read it. Any token-shaped
string still found in the run's files afterwards is scrubbed and listed in meta.json.
Results: results/<set>/<run-id>/.
"""
import argparse
import datetime as dt
import json
import os
import subprocess
import sys
import time
import hashlib
import re
import uuid
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ports import PORTS  # noqa: E402

HERE = Path(__file__).resolve().parent
AC = HERE.parent
BENCH = AC.parent
DOCS = BENCH.parent
NET = "molde-bench-int"
PROXY = "molde-bench-egress"
KEYCHAIN = os.environ.get("BENCH_KEYCHAIN_SERVICE", "molde-bench-claude-oauth")


JUDGE_LIMITS = ["--memory", "4g", "--cpus", "4", "--pids-limit", "1024"]   # review: judge was unbounded


def sh(args, timeout=None, **kw):
    """Run a command; on a host-side timeout, return a failed result instead of hanging the
    campaign (the caller removes the container by name)."""
    try:
        return subprocess.run(args, capture_output=True, text=True, timeout=timeout, **kw)
    except subprocess.TimeoutExpired as e:
        return subprocess.CompletedProcess(args, 124, e.stdout or "", f"host timeout after {timeout} s")


def ensure_network():
    if sh(["docker", "network", "inspect", NET]).returncode != 0:
        sh(["docker", "network", "create", "--internal", NET])
    st = sh(["docker", "inspect", "-f", "{{.State.Running}}", PROXY]).stdout.strip()
    if st != "true":
        sh(["docker", "rm", "-f", PROXY])
        r = sh(["docker", "run", "-d", "--name", PROXY, "--network", "bridge",
                "--log-opt", "max-size=10m", "--log-opt", "max-file=3", "molde-bench-egress:latest"])
        if r.returncode:
            sys.exit(f"egress proxy failed to start: {r.stderr}")
        sh(["docker", "network", "connect", "--alias", "egress", NET, PROXY])
        time.sleep(1)


def token_env():
    env = dict(os.environ)
    if env.get("CLAUDE_CODE_OAUTH_TOKEN"):
        return env
    r = sh(["security", "find-generic-password", "-s", KEYCHAIN, "-w"])
    if r.returncode or not r.stdout.strip():
        sys.exit(f"no Claude Code token: set CLAUDE_CODE_OAUTH_TOKEN or the Keychain entry {KEYCHAIN}")
    env["CLAUDE_CODE_OAUTH_TOKEN"] = r.stdout.strip()
    return env


def public(cmd):
    """A command line as it may be published: host paths replaced by placeholders."""
    rep = [(str(AC / "results"), "<results>"), (str(BENCH), "<bench>"), (str(DOCS), "<docs>"), (str(Path.home()), "<home>")]
    out = []
    for c in cmd:
        for a, b in rep:
            c = c.replace(a, b)
        out.append(c)
    return out


TOKEN_RE = re.compile(rb"sk-ant-[A-Za-z0-9_-]{8,}")


def scrub_token(out):
    """Replace anything shaped like an Anthropic token in every saved text file of a run
    (an agent can run `env` and print it). The archive is rebuilt without it if needed.
    Returns the files that had one, so a run that leaked is visible in meta.json."""
    hit = []
    for p in sorted(out.rglob("*")):
        if not p.is_file():
            continue
        if p.suffix == ".gz":
            r = sh(["sh", "-c", f'gzip -dc "{p}" | LC_ALL=C grep -c -a -E "sk-ant-[A-Za-z0-9_-]{{8,}}"'])
            if r.stdout.strip() not in ("", "0"):
                hit.append(str(p.relative_to(out)))
                p.unlink()   # the workspace archive is dropped rather than published with a token
            continue
        b = p.read_bytes()
        if TOKEN_RE.search(b):
            p.write_bytes(TOKEN_RE.sub(b"[REDACTED-TOKEN]", b))
            hit.append(str(p.relative_to(out)))
    return hit


def dead_run(transcript):
    """True if the agent's result event is an error that is not a cap (auth, API, limit)."""
    try:
        lines = transcript.read_text(errors="replace").strip().splitlines()
    except OSError:
        return False
    for line in reversed(lines[-20:]):
        try:
            e = json.loads(line)
        except ValueError:
            continue
        if e.get("type") == "result":
            return bool(e.get("is_error")) and e.get("subtype") not in ("error_max_turns", "error_max_budget_usd") \
                and (e.get("api_error_status") or e.get("terminal_reason") == "api_error")
    return False


def image_id(name):
    return sh(["docker", "image", "inspect", "-f", "{{.Id}}", name]).stdout.strip()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--arm", required=True)
    ap.add_argument("--task", required=True)
    ap.add_argument("--prompt", default="P1")
    ap.add_argument("--rep", type=int, default=1)
    ap.add_argument("--model", default="claude-opus-5-5")
    ap.add_argument("--effort", default="high")
    ap.add_argument("--max-turns", type=int, default=100)
    ap.add_argument("--timeout", type=int, default=3600, help="seconds for the agent step")
    ap.add_argument("--max-budget-usd", type=float, default=None,
                    help="Claude Code's own cost cap (DESIGN §9: 3x the pilot's p95); a hit is censored")
    ap.add_argument("--set", default="dev", help="results subfolder: dev, smoke, pilot, campaign")
    ap.add_argument("--dry-run", action="store_true", help="prepare and judge the untouched workspace; no agent")
    a = ap.parse_args()

    run_id = f"{dt.datetime.now(dt.timezone.utc):%Y%m%dT%H%M%SZ}-{a.arm}-{a.task}-{a.prompt}-r{a.rep}-{uuid.uuid4().hex[:6]}"
    out = AC / "results" / a.set / run_id
    ctx = out / "context"
    ctx.mkdir(parents=True)
    vol = f"molde-bench-ws-{uuid.uuid4().hex[:10]}"
    agent_name = f"molde-bench-agent-{uuid.uuid4().hex[:10]}"
    meta = {"run_id": run_id, "arm": a.arm, "task": a.task, "prompt": a.prompt, "rep": a.rep,
            "model": a.model, "effort": a.effort, "max_turns": a.max_turns, "set": a.set, "dry_run": a.dry_run,
            "timeout_s": a.timeout, "max_budget_usd": a.max_budget_usd,
            "images": {k: image_id(f"molde-bench-{k}:latest") for k in ("agent", "judge", "egress")},
            # review 2 I15: everything a replicator needs to know about the judge side
            "bench_commit": sh(["git", "-C", str(AC), "rev-parse", "HEAD"]).stdout.strip(),
            "bench_dirty": bool(sh(["git", "-C", str(AC), "status", "--porcelain", "--", "."]).stdout.strip()),
            "sha256": {str(p.relative_to(DOCS)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(
                [*AC.glob("oracle/*.py"), *AC.glob("judge/*.py"), AC / "judge/browser.js", *AC.glob("runner/*.py"),
                 AC / "checker/site-check", AC / "requirements.md", *AC.glob("tasks/*.md"), DOCS / "reference.md"])},
            "docker": sh(["docker", "version", "--format", "{{.Server.Version}} {{.Server.Os}}/{{.Server.Arch}}"]).stdout.strip(),
            "versions": sh(["docker", "run", "--rm", "molde-bench-judge:latest", "cat", "/opt/versions/all.txt"]).stdout,
            "started_utc": dt.datetime.now(dt.timezone.utc).isoformat()}
    bench_mounts = ["-v", f"{BENCH}:/bench:ro", "-v", f"{DOCS / 'reference.md'}:/docs/reference.md:ro"]
    try:
        sh(["docker", "volume", "create", vol])
        # 1. prep
        r = sh(["docker", "run", "--rm", "-u", "0", "--network", "none", *bench_mounts,
                "-v", f"{vol}:/ws", "-v", f"{ctx}:/ctx", "-v", f"{out}:/meta",
                "molde-bench-judge:latest", "python3", "/bench/agent-cost/runner/prep.py",
                "--arm", a.arm, "--task", a.task, "--prompt", a.prompt, "--ws", "/ws", "--ctx", "/ctx", "--meta", "/meta"])
        (out / "prep.log").write_text(r.stdout + r.stderr)
        if r.returncode:
            sys.exit(f"prep failed:\n{r.stdout}{r.stderr}")

        # 2. agent
        if not a.dry_run:
            ensure_network()
            token = token_env()["CLAUDE_CODE_OAUTH_TOKEN"]
            manifest = json.loads((out / "manifest.json").read_text())
            mcp = ["--strict-mcp-config"] + (["--mcp-config", "/work/context/mcp.json"] if "mcp.json" in manifest["files"] else [])
            claude = ["claude", "-p", "--model", a.model, "--effort", a.effort,
                      "--output-format", "stream-json", "--verbose",
                      "--tools", "Bash,Read,Edit,Write", *mcp, "--disable-slash-commands",
                      "--permission-mode", "bypassPermissions", "--max-turns", str(a.max_turns),
                      "--no-session-persistence", "--append-system-prompt-file", "/work/context/arm.md",
                      *(["--max-budget-usd", str(a.max_budget_usd)] if a.max_budget_usd else [])]
            # -k: KILL 30 s after TERM, so a run can't outlive its timeout
            # The token arrives on stdin, is moved to fd 3 (CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR)
            # and stdin is closed: it is in no environment (not the agent's shell, not docker
            # inspect) and no child process inherits fd 3. Verified with probes, 16 Oct.
            script = ('exec 3<&0 0</dev/null; exec timeout -k 30 ' + str(a.timeout)
                      + ' "$@" "$(cat /work/context/prompt.txt)"')
            cmd = ["docker", "run", "-i", "--name", agent_name, "--network", NET,
                   "-e", "CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR=3",
                   "-e", "HTTPS_PROXY=http://egress:8888", "-e", "HTTP_PROXY=http://egress:8888",
                   "-e", "NO_PROXY=localhost,127.0.0.1",
                   # API trouble fails fast instead of eating the timeout
                   "-e", "CLAUDE_CODE_MAX_RETRIES=4", "-e", "API_TIMEOUT_MS=600000",
                   "-e", "PATH=/work/context/bin:/usr/local/bin:/usr/bin:/bin",
                   "-e", "BASH_DEFAULT_TIMEOUT_MS=300000", "-e", "BASH_MAX_TIMEOUT_MS=600000",
                   "--pids-limit", "512", "--memory", "6g", "--cpus", "4",
                   "--cap-drop", "ALL", "--security-opt", "no-new-privileges",
                   "-v", f"{vol}:/work/site", "-v", f"{ctx}:/work/context:ro", "-w", "/work/site",
                   "molde-bench-agent:latest", "sh", "-c", script, "sh", *claude]
            meta["agent_command"] = public(cmd)   # the token is on stdin only, never in the command
            t0 = time.monotonic()
            since = dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
            with open(out / "transcript.jsonl", "w") as so, open(out / "agent.stderr", "w") as se:
                try:
                    rc = subprocess.run(cmd, input=token, stdout=so, stderr=se, text=True,
                                        timeout=a.timeout + 300).returncode
                except subprocess.TimeoutExpired:
                    rc = 124
                    meta["host_timeout"] = True
            meta["agent_exit"] = rc
            meta["agent_wall_s"] = round(time.monotonic() - t0, 1)
            # Out of memory only if Docker says so: exit 137 is any SIGKILL, including a
            # container removed from outside, which is infrastructure, not the arm.
            st = sh(["docker", "inspect", "-f", "{{.State.OOMKilled}} {{.State.ExitCode}}", agent_name]).stdout.split()
            meta["oom_killed"] = bool(st) and st[0] == "true"
            meta["container_found"] = bool(st)
            sh(["docker", "rm", "-f", agent_name])
            # what this run tried to reach, from the egress proxy (review 2 I8)
            pl = sh(["docker", "logs", "--since", since, PROXY])
            (out / "proxy.log").write_text(pl.stdout + pl.stderr)
            meta["proxy_refused"] = sorted(set(re.findall(r'refused on filtered domain "([^"]+)"', pl.stdout + pl.stderr)))
            del token

        # 3. judge, in two containers (review 2 I5). Build: runs the agent's build code, with no
        #    benchmark material, only the two scripts it needs. Grade: the oracle, never
        #    executing workspace code (except the site's own JS in Chromium, T8/T9).
        port = json.loads((out / "manifest.json").read_text())["port"]
        # A run that died on auth, an API error or a usage limit has nothing to judge:
        # skip the minutes of builds (metrics classes it as infrastructure anyway).
        if not a.dry_run and dead_run(out / "transcript.jsonl"):
            meta["judge_skipped"] = "agent ended with an API error result"
        else:
            jtask = "T0" if a.task == "factory" else a.task
            built = f"molde-bench-built-{uuid.uuid4().hex[:10]}"
            jb, jg = f"{built}-build", f"{built}-grade"
            sh(["docker", "volume", "create", built])
            try:
                r1 = sh(["docker", "run", "--rm", "--name", jb, "--network", "none", "-u", "0", *JUDGE_LIMITS,
                         "-v", f"{vol}:/in/ws:ro", "-v", f"{built}:/built",
                         "-v", f"{AC / 'judge/build_site.py'}:/judge/judge/build_site.py:ro",
                         "-v", f"{AC / 'runner/ports.py'}:/judge/runner/ports.py:ro",
                         "molde-bench-judge:latest", "python3", "/judge/judge/build_site.py",
                         "--port", port, "--task", jtask, "--workspace", "/in/ws", "--out", "/built"], timeout=1800)
                r2 = sh(["docker", "run", "--rm", "--name", jg, "--network", "none", *JUDGE_LIMITS, *bench_mounts,
                         "-v", f"{built}:/built:ro", "-v", f"{out}:/out", "molde-bench-judge:latest",
                         "python3", "/bench/agent-cost/judge/grade.py", "--built", "/built", "--task", jtask,
                         "--out", "/out"], timeout=1800)
                (out / "judge.log").write_text(r1.stdout + r1.stderr + r2.stdout + r2.stderr)
            finally:
                sh(["docker", "rm", "-f", jb, jg])
                sh(["docker", "volume", "rm", "-f", built])
        # 4. archive the workspace and its diff (including files the agent created). Build
        #    output and caches are left out of the diff for every port alike, so diff size
        #    measures the agent's edits, not what its last build wrote (review: T10 bias).
        excl = sorted({PORTS[port]["out"].split("/")[0], "out", "node_modules", ".next", ".astro", ".judge-out",
                       "*.tsbuildinfo", ".hugo_build.lock", *PORTS[port].get("generated", [])})
        spec = " ".join(f"':(glob)**/{x}'" if "*" in x else x for x in excl)
        tex = " ".join(f"--exclude=./{x}" for x in excl if "*" not in x)
        r = sh(["docker", "run", "--rm", "--network", "none", "-v", f"{vol}:/in/ws:ro", "-v", f"{out}:/out",
                "molde-bench-judge:latest", "sh", "-c",
                "cp -r /in/ws/.git /tmp/g && G='git --git-dir=/tmp/g --work-tree=/in/ws -c safe.directory=*'"
                f" && $G add -A && $G reset -q HEAD -- {spec} && $G diff --cached --stat HEAD > /out/diff-stat.txt"
                " && $G diff --cached HEAD > /out/diff.patch"
                f" && tar {tex} -czf /out/workspace.tar.gz -C /in/ws ."], timeout=600)
        (out / "archive.log").write_text(r.stdout + r.stderr)
    finally:
        sh(["docker", "rm", "-f", agent_name])
        sh(["docker", "volume", "rm", "-f", vol])
        meta["finished_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
        (out / "meta.json").write_text(json.dumps(meta, indent=1))
        meta["token_scrubbed_files"] = scrub_token(out)   # review 2 I7
        (out / "meta.json").write_text(json.dumps(meta, indent=1))

    # 5. metrics
    r = sh([sys.executable, str(HERE / "metrics.py"), str(out)])
    print(r.stdout.strip() or r.stderr.strip())


if __name__ == "__main__":
    main()
