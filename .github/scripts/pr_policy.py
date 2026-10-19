#!/usr/bin/env python3
"""The pull-request policy CI checks (docs/git-conventions.md, "What CI checks").

    PR_TITLE='feat(check): …' python3 .github/scripts/pr_policy.py <base-sha> <head-sha>

Exits 1 and prints every broken rule. The rules exist because the PR title becomes the
squash commit, and its type says what kind of release the change needs
(docs/reference.md#compatibility): a new check id or a stricter check is a minor
change, never a patch.

1. The title is `type(scope): description` (`!` before the colon for a breaking change),
   with a type from the table in docs/git-conventions.md. Release PRs are `molde X.Y.Z`.
2. A `feat` or `fix` that changes src/check/ also changes CHANGELOG.md.
3. A PR that adds a check id (a new `"check": "…"` value under src/) is `feat`, or breaking.
4. A `fix` whose CHANGELOG lines say a site now fails is a stricter check: it is `feat`.
"""
import os
import re
import subprocess
import sys

TYPES = {"feat", "fix", "test", "ci", "chore", "docs", "style", "refactor", "perf", "build",
         "revert", "bench"}
TITLE = re.compile(r"^(?P<type>[a-z]+)\((?P<scope>[^()\s][^()]*)\)(?P<breaking>!)?: \S")
RELEASE = re.compile(r"^molde \d+\.\d+\.\d+(-[0-9A-Za-z.]+)?$")
CHECK_ID = re.compile(r'"check":\s*"([a-z][a-z0-9_]*\.[a-z0-9_.]+)"')
STRICTER = re.compile(r"(?i)\b(now fails?|may now fail|would now fail|now reported)\b")


def git(*args):
    return subprocess.run(["git", *args], capture_output=True, text=True, check=True).stdout


def check_ids(rev):
    out = subprocess.run(["git", "grep", "-h", "-E", r'"check":[[:space:]]*"', rev, "--", "src"],
                         capture_output=True, text=True).stdout
    return set(CHECK_ID.findall(out))


def problems(title, changed, added_changelog_lines, new_ids):
    errs = []
    if RELEASE.match(title):
        return errs
    m = TITLE.match(title)
    if not m:
        return [f"title {title!r} is not `type(scope): description` (docs/git-conventions.md)"]
    kind, breaking = m["type"], bool(m["breaking"])
    if kind not in TYPES:
        errs.append(f"type `{kind}` is not one of: {', '.join(sorted(TYPES))}")
    touches_check = any(f.startswith("src/check/") for f in changed)
    if kind in ("feat", "fix") and touches_check and "CHANGELOG.md" not in changed:
        errs.append("a feat or fix that changes src/check/ changes the check report's behaviour: "
                    "add its entry to CHANGELOG.md, with what to do")
    if new_ids and kind != "feat" and not breaking:
        errs.append(f"new check ids ({', '.join(sorted(new_ids))}) arrive in a minor release: "
                    f"the title's type is `feat`, not `{kind}`")
    if kind == "fix" and not breaking and any(STRICTER.search(l) for l in added_changelog_lines):
        errs.append("the CHANGELOG says a site now fails, so the check got stricter: that is a minor "
                    "change (docs/reference.md#compatibility), so the type is `feat`, not `fix`")
    return errs


def main():
    base, head = sys.argv[1], sys.argv[2]
    title = os.environ.get("PR_TITLE", "")
    changed = git("diff", "--name-only", f"{base}...{head}").split()
    diff = git("diff", "--unified=0", f"{base}...{head}", "--", "CHANGELOG.md")
    added = [l[1:] for l in diff.splitlines() if l.startswith("+") and not l.startswith("+++")]
    new_ids = check_ids(head) - check_ids(base)
    errs = problems(title, changed, added, new_ids)
    for e in errs:
        print(f"::error::{e}")
    print(f"title: {title!r}; files changed: {len(changed)}; new check ids: {sorted(new_ids) or 'none'}")
    sys.exit(1 if errs else 0)


if __name__ == "__main__":
    main()
