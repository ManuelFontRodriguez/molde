"""Where each port's files come from, where its data lives, and how it builds.

A workspace is the site's own folder (the agent's working directory is the site root).
Every recipe keeps all its inputs inside the workspace (review 1, B4). `build` runs with
the workspace as the current directory; `out` is the build output, relative to it.
`node_deps` names the pre-installed dependency set in the image (/opt/node-deps/<set>);
the workspace gets its own writable copy of its node_modules.

Files removed from every workspace before the agent sees it: BUILD.md (the build
benchmark's notes, which describe the golden target and the benchmark itself).
"""

from pathlib import Path

PARITY = "cross-framework/harness/sites"
IDIOMATIC = "agent-cost/ports-idiomatic"

PORTS = {
    # The molde port. Its build command for the judge is always a fresh `molde build`
    # (no gate), never the agent's out/current (review 1, B1).
    "molde": {
        "src": f"{PARITY}/molde",
        "data": "data.json",
        "build": "molde build --site . --out .judge-out >/dev/null",
        "out": ".judge-out/current",
        "node_deps": None,
    },
    "parity-hugo": {
        "src": f"{PARITY}/hugo",
        "data": "data/stocks.json",
        "build": "hugo --destination public --cleanDestinationDir --noBuildLock --logLevel warn",
        "out": "public",
        "node_deps": None,
    },
    "parity-zola": {
        "src": f"{PARITY}/zola",
        "data": "data.json",
        "build": "python3 gen_content.py && zola build",
        "out": "public",
        "generated": ["content/stocks", "content/page"],   # written by gen_content.py on every build
        "node_deps": None,
    },
    "parity-eleventy": {
        "src": f"{PARITY}/eleventy",
        "data": "src/_data/demo.json",
        "build": "rm -rf _site && npx @11ty/eleventy --quiet",
        "out": "_site",
        "node_deps": "parity-eleventy",
    },
    "parity-astro": {
        "src": f"{PARITY}/astro",
        "data": "src/data/data.json",
        "build": "rm -rf dist && npx astro build --silent",
        "out": "dist",
        "node_deps": "parity-astro",
    },
    # The Next.js parity port reads its data and stylesheet from DEMO_SITE_DIR; in the
    # build benchmark that folder sat outside the workspace. Here it is ./demo, inside.
    "parity-nextjs": {
        "src": f"{PARITY}/nextjs",
        "data": "demo/data.json",
        "extra": {"demo/static/style.css": "cross-framework/harness/golden/style.css"},
        "build": "rm -rf .next out && DEMO_SITE_DIR=./demo npm run build --silent >/dev/null",
        "out": "out",
        "node_deps": "parity-nextjs",
    },
    # The idiomatic ports (DESIGN.md §4): written by the port factory (runner/factory.md),
    # frozen under ports-idiomatic/<fw>/site. Until a port is frozen its `src` folder holds
    # only package.json + lockfile, which is also what the factory's T0 workspace gets.
    "idiomatic-hugo": {
        "src": f"{IDIOMATIC}/hugo/site",
        "data": "data/stocks.json",
        "build": "hugo --destination public --cleanDestinationDir --noBuildLock --logLevel warn",
        "out": "public",
        "node_deps": None,
    },
    "idiomatic-astro": {
        "src": f"{IDIOMATIC}/astro/site",
        "data": "src/data/stocks.json",
        "build": "rm -rf dist && npx astro build --silent",
        "out": "dist",
        "node_deps": "idiomatic-astro",
    },
    "idiomatic-nextjs": {
        "src": f"{IDIOMATIC}/nextjs/site",
        "data": "data/stocks.json",
        "build": "rm -rf .next out && npm run build --silent >/dev/null",
        "out": "out",
        "node_deps": "idiomatic-nextjs",
    },
}

DROP = {"BUILD.md"}


def stage(port, bench_dir, dest, data_json):
    """Copy a port into dest (a fresh workspace) with data_json as its data."""
    import shutil
    r = PORTS[port]
    src = Path(bench_dir) / r["src"]
    # The vendored sources hold no build output, so only dependency folders are skipped.
    # (A folder name is not a safe signal: public/ is output for Hugo and Zola but source
    # for Astro and Next.js.)
    shutil.copytree(src, dest, ignore=shutil.ignore_patterns(*DROP, "node_modules", ".next"))
    d = Path(dest) / r["data"]
    d.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy(data_json, d)
    for rel, from_rel in r.get("extra", {}).items():
        t = Path(dest) / rel
        t.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(Path(bench_dir) / from_rel, t)
    if r["node_deps"]:
        copy_deps(dest, r["node_deps"])
    return r


def copy_deps(dest, dep_set):
    """A writable copy of a pre-installed dependency set (see the Dockerfile)."""
    import shutil, subprocess
    nm = Path(dest) / "node_modules"
    if nm.is_symlink():
        nm.unlink()
    elif nm.exists():
        shutil.rmtree(nm)
    subprocess.run(["cp", "-r", f"/opt/node-deps/{dep_set}/node_modules", str(nm)], check=True)
