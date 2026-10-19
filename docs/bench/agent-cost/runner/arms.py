"""The arms: what each agent gets (DESIGN.md §2–3).

Every arm gets the same requirements text and the same note shape. The only differences
are the generator (the port), the build command, whether a checker exists, and the docs.
The note is passed with --append-system-prompt-file, the same position for every arm.

Tool access is by PATH: /work/context/bin is the only extra directory, and holds only the
tools the arm may use (molde, a molde shim, or site-check). The agent image has no molde
at all: prep.py copies it into /work/context/bin for molde arms only. In the molde-build
arm the binary sits next to the shim as `.molde`, so an agent could still run it directly;
every transcript is scanned for that (runner/metrics.py: `forbidden_tool_use`).
"""

import re
from pathlib import Path

MOLDE_DOCS = "/work/context/molde-reference.md"

NOTE = """# This workspace

This folder is a small static website built from `data.json`. {generator}

- Build: `{build}` (run it from this folder). Output: `{out}/`.
- Check: {check}
- Docs: {docs}

There is no internet access in this environment. Every dependency the site needs is already installed.

{requirements}
"""


CHECKER_MOLDE = ("`site-check out/current` checks the built site in `out/current/` against the requirements below "
                 "and prints one JSON report of every problem. It does not build; run the build first. It does not "
                 "check whether the pages follow the data or show the right content (requirements 8 and 10).")


def molde_note(build, out, check, docs):
    return dict(generator="It is built with molde, a static site generator; templates are minijinja (Jinja2 syntax).",
                build=build, out=out, check=check, docs=docs)


ARMS = {
    "molde-check": {
        "port": "molde",
        "bin": {"molde": "molde-full"},
        "note": molde_note("molde check --site . --out out", "out/current",
                           "the build command is also the check: `molde check` builds the site, runs molde's checks "
                           "and prints one JSON report of every problem; it publishes to `out/current` only when all pass.",
                           f"`{MOLDE_DOCS}` (the molde reference)."),
        "docs": "full",
    },
    "molde-build": {
        "port": "molde",
        "bin": {"molde": "molde-shim-build"},
        "note": molde_note("molde build --site . --out out", "out/current", "none.",
                           f"`{MOLDE_DOCS}` (the molde reference)."),
        "docs": "build-only",
    },
    # Review 2 B4: the like-for-like comparator for the thesis. molde without its own check,
    # with the same standalone checker the framework arms get.
    "molde+checker": {
        "port": "molde",
        "bin": {"molde": "molde-shim-build", "site-check": "site-check"},
        "note": molde_note("molde build --site . --out out", "out/current",
                           CHECKER_MOLDE, f"`{MOLDE_DOCS}` (the molde reference)."),
        "docs": "build-only",
    },
    # Round 2A (ROUND-2A.md): molde-check with one change, the note points to the short
    # guide (`molde guide`, docs/agents.md) first. Same binary, same reference.
    "molde-check-guide": {
        "port": "molde",
        "bin": {"molde": "molde-full"},
        "note": molde_note("molde check --site . --out out", "out/current",
                           "the build command is also the check: `molde check` builds the site, runs molde's checks "
                           "and prints one JSON report of every problem; it publishes to `out/current` only when all pass.",
                           "run `molde guide` for a short guide to molde (about 2k tokens); for details, "
                           f"`{MOLDE_DOCS}` is the full reference."),
        "docs": "full",
    },
    "molde-mcp": {
        "port": "molde",
        "bin": {"molde": "molde-full"},
        "mcp": True,
        "note": molde_note("molde check --site . --out out", "out/current",
                           "the build command is also the check: `molde check` builds the site, runs molde's checks "
                           "and prints one JSON report of every problem; it publishes to `out/current` only when all pass. "
                           "An MCP server named `molde` is also connected, with the tools check, list_routes, "
                           "explain_route, render_page and diff_build.",
                           f"`{MOLDE_DOCS}` (the molde reference)."),
        "docs": "full",
    },
}

FRAMEWORK = {
    "hugo": ("It is built with Hugo.", "hugo --destination public --cleanDestinationDir", "public"),
    "zola": ("It is built with Zola (`gen_content.py` turns data.json into content files first).",
             "python3 gen_content.py && zola build", "public"),
    "eleventy": ("It is built with Eleventy.", "npx @11ty/eleventy", "_site"),
    "astro": ("It is built with Astro.", "npx astro build", "dist"),
}
NEXT_PARITY = ("It is built with Next.js (static export).", "DEMO_SITE_DIR=./demo npm run build", "out")
NEXT_IDIOMATIC = ("It is built with Next.js (static export).", "npm run build", "out")

CHECKER = ("`site-check {out}` checks the built site in `{out}/` against the requirements below and prints "
           "one JSON report of every problem. It does not build; run the build first. It does not check whether "
           "the pages follow the data or show the right content (requirements 8 and 10).")


def _framework_arm(port, generator, build, out, checker):
    return {
        "port": port,
        "bin": {"site-check": "site-check"} if checker else {},
        "note": dict(generator=generator, build=build, out=out,
                     check=CHECKER.format(out=out) if checker else "none.", docs="none."),
        "docs": None,
    }


for fw in ("hugo", "astro"):
    g, b, o = FRAMEWORK[fw]
    ARMS[fw] = _framework_arm(f"idiomatic-{fw}", g, b, o, False)
    ARMS[f"{fw}+checker"] = _framework_arm(f"idiomatic-{fw}", g, b, o, True)
ARMS["nextjs"] = _framework_arm("idiomatic-nextjs", *NEXT_IDIOMATIC, False)
ARMS["nextjs+checker"] = _framework_arm("idiomatic-nextjs", *NEXT_IDIOMATIC, True)
for fw in ("hugo", "zola", "eleventy", "astro"):
    g, b, o = FRAMEWORK[fw]
    ARMS[f"parity-{fw}"] = _framework_arm(f"parity-{fw}", g, b, o, False)
ARMS["parity-nextjs"] = _framework_arm("parity-nextjs", *NEXT_PARITY, False)


# Port-factory arms: write an idiomatic port from empty (task "factory"). The note names
# only the framework; nothing about molde, the benchmark or other frameworks.
for fw, (g, b, o) in {"hugo": ("It is built with Hugo.", "hugo --destination public --cleanDestinationDir", "public"),
                      "astro": ("It is built with Astro.", "npx astro build", "dist"),
                      "nextjs": ("It is built with Next.js (static export).", "npm run build", "out")}.items():
    ARMS[f"factory-{fw}"] = _framework_arm(f"idiomatic-{fw}", g, b, o, False)
    ARMS[f"factory-{fw}"]["factory"] = fw


def note_text(arm, requirements):
    return NOTE.format(requirements=requirements.strip(), **ARMS[arm]["note"])


def build_only_reference(full):
    """The molde reference for the molde-build arm: the check report and agent-server
    sections removed (the shim refuses `check`, `mcp` and `events`). Other passing
    mentions of checks stay; an agent that tries `molde check` is told it is unavailable."""
    drop = ("## The check report", "## Security model")
    out, skipping = [], False
    for line in full.splitlines(keepends=True):
        if line.startswith("## "):
            skipping = line.strip() in drop
        if not skipping:
            if re.match(r"^molde (check|events|mcp) ", line):
                continue
            out.append(line)
    return "".join(out)


SHIM_BUILD = """#!/bin/sh
# molde for the molde-build arm: everything except the commands that check.
case "$1" in
  check|mcp|events) echo "molde $1 is not available in this environment" >&2; exit 2 ;;
  serve) for a in "$@"; do [ "$a" = "--watch" ] && { echo "molde serve --watch is not available in this environment" >&2; exit 2; }; done ;;
esac
exec /work/context/bin/.molde "$@"
"""
