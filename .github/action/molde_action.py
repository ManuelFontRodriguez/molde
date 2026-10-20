#!/usr/bin/env python3
"""The molde GitHub Action (action.yml): install molde, run `check` or `verify`, put the
report in the job summary and in one comment on the pull request, and fail on errors.

Every input arrives in the environment (MOLDE_*), never through a shell. Standard
library only, so it runs on any GitHub runner with Python 3.

The report is the pull request author's content (page paths, titles, hints quote the
site), so every string from it is escaped before it reaches the log, the summary, the
comment or an output: it can't run workflow commands, break out of the Markdown, or
mention anyone.
"""
import hashlib
import io
import json
import os
import platform
import re
import secrets
import shlex
import subprocess
import sys
import tarfile
import time
import urllib.request

MARKER = "<!-- molde-report"   # + this run's key + " -->"
MAX_ROWS = 30          # errors shown in full; the rest are counted
MAX_CHARS = 60_000     # GitHub comments stop at 65,536
BOT = "github-actions[bot]"
VERSION_RE = r"\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?"


def env(name, default=""):
    return os.environ.get(name, default).strip()


def flag(name, default):
    """A boolean input: true or false, in any case. Anything else stops the run, so a
    typo can't silently turn the gate off."""
    v = env(name, default).lower()
    if v not in ("true", "false"):
        sys.exit(f"::error::{name.removeprefix('MOLDE_').lower().replace('_', '-')} is `true` or `false`, not {esc_log(v)!r}")
    return v == "true"


def esc_log(s):
    """A value for one log line: no line breaks, so it can't start a workflow command."""
    return str(s).replace("\r", " ").replace("\n", " ")


def esc_md(s, limit=120):
    """A value as Markdown text: no HTML, no links or images, no mentions or issue
    references, no table separator, no line breaks; cut to `limit` characters."""
    s = s if isinstance(s, str) else json.dumps(s, ensure_ascii=False)
    s = " ".join(s.split())
    if limit and len(s) > limit:
        s = s[: limit - 1] + "…"
    return s.translate(MD_ESCAPES)


# each character escaped once: HTML, the table separator, mentions and references,
# code spans; then Markdown's own punctuation
MD_ESCAPES = str.maketrans(
    {"&": "&amp;", "<": "&lt;", ">": "&gt;", "|": "&#124;", "@": "&#64;", "#": "&#35;", "`": "&#96;"}
    | {c: "\\" + c for c in "\\*_[]()!~"}
)


def code(s):
    """A check id or key in a code span, when it is one (`links.internal`); anything
    else as escaped text."""
    s = str(s)
    return f"`{s}`" if re.fullmatch(r"[A-Za-z0-9_.-]{1,80}", s) else esc_md(s)


def annotation(s, prop=False):
    """A value escaped for a workflow command (`::error title=…::…`): a page path with a
    newline or `%` must not end the command or start another."""
    s = str(s).replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
    return s.replace(":", "%3A").replace(",", "%2C") if prop else s


def target():
    """The release archive's target for this runner."""
    system, machine = platform.system(), platform.machine().lower()
    arch = {"x86_64": "x86_64", "amd64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}.get(machine)
    if system == "Linux" and arch:
        return f"{arch}-unknown-linux-musl"
    if system == "Darwin" and arch:
        return f"{arch}-apple-darwin"
    sys.exit(f"::error::molde runs on Linux and macOS runners; this one is {system} {machine}")


def api(url, token, data=None, method=None):
    req = urllib.request.Request(url, data=json.dumps(data).encode() if data is not None else None, method=method)
    req.add_header("Accept", "application/vnd.github+json")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.loads(r.read() or b"null")


def fetch(url, attempts=3):
    """A public release file, retried: a download is the step most likely to flake."""
    for i in range(attempts):
        try:
            with urllib.request.urlopen(url, timeout=120) as r:
                return r.read()
        except OSError as e:
            if i == attempts - 1:
                sys.exit(f"::error::could not download {url}: {esc_log(e)}")
            time.sleep(2 ** i)


def resolve_version(repo):
    """The `version` input, else the release tag this action was pinned to. A pin that
    names no release (a commit SHA, a branch, `@v0`) needs `version`: the action never
    silently runs another molde than the one asked for. `latest` asks for the latest."""
    v = env("MOLDE_VERSION").removeprefix("v")
    if v == "latest":
        data = json.loads(fetch(f"https://api.github.com/repos/{repo}/releases/latest"))
        v = str(data.get("tag_name", "")).removeprefix("v")
    if v:
        if not re.fullmatch(VERSION_RE, v):
            sys.exit(f"::error::version is a release such as 0.2.0, or `latest`; not {esc_log(v)!r}")
        return v
    ref = env("MOLDE_ACTION_REF").removeprefix("v")
    if re.fullmatch(VERSION_RE, ref):
        return ref
    sys.exit(
        f"::error::the action is pinned to {esc_log(env('MOLDE_ACTION_REF')) or 'no ref'}, which names no "
        "molde release: pin it to a release (uses: …@v0.2.0), or set `version` (a release, or `latest`)"
    )


def install():
    """Download the release for this runner, check its SHA-256, return the binary's path.
    The archive is checked in memory and only the checked bytes are unpacked."""
    repo = env("MOLDE_ACTION_REPOSITORY") or "ManuelFontRodriguez/molde"
    version = resolve_version(repo)
    name = f"molde-{version}-{target()}"
    base = f"https://github.com/{repo}/releases/download/v{version}"
    archive = fetch(f"{base}/{name}.tar.gz")
    sums = fetch(f"{base}/SHA256SUMS").decode()
    want = None
    for line in sums.splitlines():
        parts = line.split()
        if len(parts) == 2 and parts[1].lstrip("*") == f"{name}.tar.gz":
            want = parts[0]
    got = hashlib.sha256(archive).hexdigest()
    if want != got:
        sys.exit(f"::error::{name}.tar.gz: SHA-256 {got} does not match SHA256SUMS ({want})")
    dest = os.path.join(os.environ.get("RUNNER_TEMP", "/tmp"), "molde-bin")
    os.makedirs(dest, exist_ok=True)
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as t:
        member = t.getmember(f"{name}/molde")
        if not member.isfile():
            sys.exit(f"::error::{name}.tar.gz: {name}/molde is not a file")
        with t.extractfile(member) as src, open(os.path.join(dest, "molde"), "wb") as out:
            out.write(src.read())
    path = os.path.join(dest, "molde")
    os.chmod(path, 0o755)
    print(f"molde {version} ({target()}), SHA-256 verified")
    return path


def command_line(molde):
    cmd = env("MOLDE_COMMAND", "check")
    extra = shlex.split(env("MOLDE_ARGS"))
    if cmd == "check":
        return [molde, "check", "--site", env("MOLDE_SITE", "site"), "--out", env("MOLDE_OUT", "out"), *extra]
    if cmd == "verify":
        d = env("MOLDE_DIR")
        if not d:
            sys.exit("::error::verify needs `dir`: the built site (public/, dist/, out/, _site/)")
        base = ["--base-url", env("MOLDE_BASE_URL")] if env("MOLDE_BASE_URL") else []
        return [molde, "verify", d, *base, *extra]
    sys.exit(f"::error::`command` is `check` or `verify`, not {esc_log(cmd)!r}")


def run_key():
    """What tells this run's report from another's on the same pull request: the
    workflow, the job, the command and its folder. Two steps checking two sites keep
    two comments and two report files."""
    what = "|".join([env("GITHUB_WORKFLOW"), env("GITHUB_JOB"), env("MOLDE_COMMAND", "check"),
                     env("MOLDE_DIR") or env("MOLDE_SITE", "site")])
    return hashlib.sha256(what.encode()).hexdigest()[:12]


def marker():
    return f"{MARKER} {run_key()} -->"


def cell(v):
    return esc_md(v)


def render(report, command):
    """The report as Markdown: verdict, counts per check, the first errors in full, the
    warnings and skipped checks folded. Every value from the report is escaped."""
    errors = [e for e in report.get("errors") or [] if isinstance(e, dict)]
    warnings = [w for w in report.get("warnings") or [] if isinstance(w, dict)]
    verdict = "pass" if report.get("verdict") == "pass" else "fail"
    head = "✅ **pass**" if verdict == "pass" else f"❌ **fail**: {len(errors)} error{'s' * (len(errors) != 1)}"
    lines = [marker(), f"### molde {esc_md(command)}: {head}", ""]
    pages = report.get("pages")
    if isinstance(pages, dict) and pages:
        lines += ["Pages: " + ", ".join(f"{esc_md(n)} {esc_md(str(k).replace('_', ' '))}" for k, n in pages.items()), ""]
    elif isinstance(report.get("build"), dict) and report["build"].get("pages") is not None:
        lines += [f"Pages: {esc_md(report['build']['pages'])}", ""]
    if errors:
        counts = {}
        for e in errors:
            counts[str(e.get("check", "?"))] = counts.get(str(e.get("check", "?")), 0) + 1
        lines += ["| check | errors |", "|---|---|"] + [f"| {code(k)} | {n} |" for k, n in sorted(counts.items())] + [""]
        lines += ["| check | page | details |", "|---|---|---|"]
        for e in errors[:MAX_ROWS]:
            details = {k: v for k, v in e.items() if k not in ("layer", "check", "page", "hint", "pages")}
            text = "; ".join(f"{esc_md(k)}: {cell(v)}" for k, v in details.items())
            lines.append(f"| {code(e.get('check', '?'))} | {cell(e.get('page', e.get('file', '')))} | {text} |")
        if len(errors) > MAX_ROWS:
            lines.append(f"\n…and {len(errors) - MAX_ROWS} more in the job's report.")
        hints = []
        for e in errors:
            h = e.get("hint")
            if h and (e.get("check"), h) not in hints:
                hints.append((e.get("check"), h))
        if hints:
            lines += ["", "<details><summary>What to do</summary>", ""]
            lines += [f"- {code(c)}: {esc_md(h, 400)}" for c, h in hints]
            lines += ["", "</details>"]
    if warnings:
        lines += ["", f"<details><summary>{len(warnings)} warning{'s' * (len(warnings) != 1)} (these never fail)</summary>", ""]
        for w in warnings[:MAX_ROWS]:
            lines.append(f"- {code(w.get('check'))} {cell(w.get('page', ''))}")
        lines += ["", "</details>"]
    skipped = [s for s in report.get("skipped") or [] if isinstance(s, dict)]
    if skipped:
        lines += ["", "<details><summary>Not checked</summary>", ""]
        lines += [f"- {esc_md(s.get('checks'))}: {esc_md(s.get('reason'), 400)}"
                  + (f". {esc_md(s['suggestion'], 400)}" if s.get("suggestion") else "") for s in skipped]
        lines += ["", "</details>"]
    text = "\n".join(lines) + "\n"
    if len(text) > MAX_CHARS:
        # cut at a line, and close what the cut left open, so the comment still renders
        text = text[: text.rfind("\n", 0, MAX_CHARS) + 1]
        text += "</details>\n" * (text.count("<details>") - text.count("</details>"))
        text += "\n…(cut: the full report is in the job)\n"
    return text


def own_logins(base, token):
    """Whose comments this action may edit: the Actions bot, and the token's own user
    when it is a personal token. Never a comment someone else wrote with the marker."""
    logins = {BOT}
    try:
        me = api(f"{base}/user", token)
        if isinstance(me, dict) and me.get("login"):
            logins.add(me["login"])
    except Exception:  # GITHUB_TOKEN can't read /user: the bot is the author
        pass
    return logins


def comment(text, token):
    """One comment per pull request and run key, updated on each run."""
    event_path = os.environ.get("GITHUB_EVENT_PATH")
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not (event_path and repo and token) or not flag("MOLDE_COMMENT", "true"):
        return
    base = os.environ.get("GITHUB_API_URL", "https://api.github.com")
    try:
        with open(event_path, encoding="utf-8") as f:
            pr = ((json.load(f) or {}).get("pull_request") or {}).get("number")
        if not pr:
            return
        mine_by = own_logins(base, token)
        # every page of comments: on a busy pull request ours may be past the first 100
        mine, page = None, 1
        while mine is None:
            batch = api(f"{base}/repos/{repo}/issues/{pr}/comments?per_page=100&page={page}", token) or []
            mine = next((c for c in batch
                         if marker() in (c.get("body") or "") and (c.get("user") or {}).get("login") in mine_by), None)
            if len(batch) < 100:
                break
            page += 1
        if mine:
            api(f"{base}/repos/{repo}/issues/comments/{mine['id']}", token, {"body": text}, "PATCH")
        else:
            api(f"{base}/repos/{repo}/issues/{pr}/comments", token, {"body": text}, "POST")
    except Exception as e:  # a fork's token can't comment: the summary still has the report
        print(f"::warning::could not comment on the pull request ({esc_log(e)}); the report is in the job summary")


def main():
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    except AttributeError:
        pass
    token = env("MOLDE_TOKEN")
    fail_on_error = flag("MOLDE_FAIL_ON_ERROR", "true")
    flag("MOLDE_COMMENT", "true")  # rejected early, before the run, if it is a typo
    molde = env("MOLDE_PATH") or install()
    argv = command_line(molde)
    print("running:", esc_log(" ".join(shlex.quote(a) for a in argv)))
    # molde never needs the token
    child_env = {k: v for k, v in os.environ.items() if k != "MOLDE_TOKEN"}
    proc = subprocess.run(argv, capture_output=True, text=True, encoding="utf-8", errors="replace", env=child_env)
    try:
        report = json.loads(proc.stdout)
    except ValueError:
        report = None
    if not isinstance(report, dict) or not isinstance(report.get("errors", []), list):
        sys.stderr.write(proc.stdout[-4000:] + proc.stderr[-4000:])
        sys.exit(f"::error::molde printed no report (exit {proc.returncode})")
    path = os.path.join(os.environ.get("RUNNER_TEMP", "."), f"molde-report-{run_key()}.json")
    with open(path, "w", encoding="utf-8") as f:
        json.dump(report, f, indent=1)
    command = env("MOLDE_COMMAND", "check")
    text = render(report, command)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as f:
            f.write(text)
    # the report quotes the site: no line of it may be read as a workflow command
    stop = secrets.token_hex(16)
    print(f"::stop-commands::{stop}")
    print(text)
    print(f"::{stop}::")
    comment(text, token)
    errors = report.get("errors") or []
    verdict = "pass" if report.get("verdict") == "pass" and proc.returncode == 0 else "fail"
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as f:
            f.write(f"verdict={verdict}\nerrors={len(errors)}\nreport={esc_log(path)}\n")
    for e in errors[:MAX_ROWS]:
        if not isinstance(e, dict):
            continue
        title = annotation(f"molde {e.get('check')}", prop=True)
        body = annotation(f"{e.get('page', e.get('file', ''))}: {e.get('hint', '')}")
        print(f"::error title={title}::{body}")
    # the verdict and the exit code decide, not only the error count: a run that says
    # fail, or a molde that exits non-zero, never gives a green step
    if verdict != "pass" and fail_on_error:
        if not errors:
            print(f"::error::molde: verdict {esc_log(report.get('verdict'))!r}, exit {proc.returncode}")
        sys.exit(1)


if __name__ == "__main__":
    main()
