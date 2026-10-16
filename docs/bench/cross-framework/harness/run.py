#!/usr/bin/env python3
"""Reproduce the cross-framework benchmark. Usage:

    python3 run.py [framework ...]        # default: all

Frameworks: molde zola hugo eleventy astro nextjs

For each framework: copy its authored sources from sites/<fw>/ into a scratch
workspace, run setup once (installs stay inside the workspace, except Zola and
Hugo which come from Homebrew), then RUNS clean full builds on the small
dataset (4 stocks), a fidelity check against golden/, and RUNS clean full
builds on the big dataset (1,900 generated stocks). For molde it also times
`molde check` on both datasets. Results land in results.json and in
../<framework>.metrics.json (the numeric fields are updated in place; the
qualitative fields - residue, manual features, conformance, notes - are
hand-written and stay as they are).

Environment:
    BENCH_RUNS=11          timed runs per cell (median reported; default 11)
    BENCH_WORK=<dir>       scratch workspace (default /tmp/molde-bench-run)
    BENCH_REINSTALL=1      time `brew reinstall zola|hugo` as the install cost
    MOLDE_BIN=<path>       molde binary (default <repo>/target/release/molde,
                           built with `cargo build --release --locked` if missing)
"""
import json
import os
import platform
import shutil
import statistics
import subprocess
import sys
import re
import time
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
BENCH = HERE.parent
MOLDE_REPO = HERE.parent.parent.parent.parent  # docs/bench/<dir>/harness -> repo root
WORK = Path(os.environ.get("BENCH_WORK", "/tmp/molde-bench-run"))
RUNS = int(os.environ.get("BENCH_RUNS", "11"))
REINSTALL = os.environ.get("BENCH_REINSTALL") == "1"
BREW_ENV = {"HOMEBREW_NO_AUTO_UPDATE": "1", "HOMEBREW_NO_INSTALL_CLEANUP": "1"}

# Files that are not counted as "authored": lockfiles, build notes, the shared
# stylesheet / robots.txt copied through verbatim, package manifests, ignore files.
NOT_AUTHORED = {"BUILD.md", "package.json", "package-lock.json", "style.css", ".gitignore"}
NOT_AUTHORED_PATHS = {"public/robots.txt"}

# Each recipe: setup (once, cwd=workspace), build (every timed run), outdir
# (deleted before each build, compared after), datafile (overwritten with the
# big dataset for the big runs). {ws} = workspace dir.
RECIPES = {
    "molde": {
        "src": "molde",  # frozen snapshot of examples/demo (see its BUILD.md)
        "setup": [],
        "build": "{molde} build --site site --out out",
        # molde dedups byte-identical builds; wipe the whole out dir so every
        # timed run is a genuine full build, but compare the published pointer.
        "outdir": "out",
        "cmpdir": "out/current",
        "datafile": "site/data.json",
        "version": "{molde} --version",
    },
    "zola": {
        "src": "zola",
        "setup": [],
        "brew": "zola",
        "build": "python3 site/gen_content.py && zola --root site build",
        "outdir": "site/public",
        "datafile": "site/data.json",
        "version": "zola --version",
    },
    "hugo": {
        "src": "hugo",
        "setup": [],
        "brew": "hugo",
        # cacheDir persists across a row's timed runs (warm-cache full builds;
        # Astro's Vite cache behaves likewise).
        "build": "hugo --source site --destination {ws}/out"
        " --cacheDir {ws}/cache --noBuildLock --logLevel warn",
        "outdir": "out",
        "datafile": "site/data/stocks.json",
        "version": "hugo version",
    },
    "eleventy": {
        "src": "eleventy",
        "setup": ["cd {ws}/site && npm ci --silent"],
        "build": "cd {ws}/site && npx @11ty/eleventy --quiet",
        "outdir": "site/_site",
        "datafile": "site/src/_data/demo.json",
        "version": "cd {ws}/site && npm ls @11ty/eleventy --depth=0",
        "node": True,
    },
    "astro": {
        "src": "astro",
        "setup": ["cd {ws}/site && npm ci --silent"],
        "build": "cd {ws}/site && npx astro build",
        "outdir": "site/dist",
        "datafile": "site/src/data/data.json",
        "version": "cd {ws}/site && npm ls astro --depth=0",
        "node": True,
    },
    "nextjs": {
        "src": "nextjs",
        # The port reads data.json + static/style.css from DEMO_SITE_DIR;
        # npm run build triggers the pre-build sitemap/robots/style staging.
        "setup": [
            "cd {ws}/site && npm ci --silent",
            "mkdir -p {ws}/demo/static && cp " + str(HERE / "golden/style.css") + " {ws}/demo/static/",
        ],
        "build": "cd {ws}/site && rm -rf .next && DEMO_SITE_DIR={ws}/demo npm run build --silent > /dev/null",
        "outdir": "site/out",
        "datafile": "demo/data.json",
        "version": "cd {ws}/site && npm ls next --depth=0",
        "node": True,
    },
}

def sh(cmd, env=None, **kw):
    e = dict(os.environ, **(env or {}))
    r = subprocess.run(cmd, shell=True, capture_output=True, text=True, env=e, **kw)
    if r.returncode != 0:
        sys.exit(f"FAILED ({r.returncode}): {cmd}\n{r.stderr.strip()[-2000:]}")
    return r


def med(xs):
    return statistics.median(xs)


def timed(cmd, ws, prep=None):
    """Run cmd RUNS times (prep() before each), return list of ms."""
    times = []
    for _ in range(RUNS):
        if prep:
            prep()
        t0 = time.monotonic()
        sh(cmd, cwd=ws)
        times.append((time.monotonic() - t0) * 1000)
    return times


def stats(times):
    return {"median_ms": round(med(times), 1), "min_ms": round(min(times), 1),
            "max_ms": round(max(times), 1), "runs": len(times)}


def wipe(path):
    return lambda: shutil.rmtree(path, ignore_errors=True)


def compare(recipe, ws):
    # compare.py only diffs files present in golden/ and only reports extra
    # *.html, so molde's .gz siblings never affect the result.
    out = ws / recipe.get("cmpdir", recipe["outdir"])
    r = sh(f"python3 {BENCH}/compare.py {out} {HERE}/golden")
    return r.stdout.strip().splitlines()[-1]


def authored(src):
    files = []
    for p in sorted(src.rglob("*")):
        rel = p.relative_to(src).as_posix()
        if (p.is_file() and p.name not in NOT_AUTHORED and rel not in NOT_AUTHORED_PATHS
                and "node_modules" not in p.parts):
            files.append(rel)
    loc = sum((src / f).read_text().count("\n") for f in files)  # same as wc -l
    return files, loc


def du_mb(path):
    kb = int(sh(f"du -skL {path}").stdout.split()[0])
    return round(kb / 1024)


def main():
    targets = sys.argv[1:] or list(RECIPES)
    unknown = [t for t in targets if t not in RECIPES]
    if unknown:
        sys.exit(f"unknown framework(s): {unknown}; choose from {list(RECIPES)}")
    molde_bin = Path(os.environ.get("MOLDE_BIN", MOLDE_REPO / "target/release/molde"))
    if "molde" in targets and not molde_bin.exists():
        print("building molde (cargo build --release --locked)...")
        cargo = shutil.which("cargo") or str(Path.home() / ".cargo/bin/cargo")
        t0 = time.monotonic()
        sh(f"{cargo} build --release --locked", cwd=MOLDE_REPO)
        print(f"  cold build: {time.monotonic() - t0:.1f}s")
    big = sh(f"python3 {HERE}/gen_data_big.py").stdout
    machine = {
        "cpu": sh("sysctl -n machdep.cpu.brand_string").stdout.strip(),
        "ram_gb": round(int(sh("sysctl -n hw.memsize").stdout) / 2**30),
        "os": platform.platform(),
        "node": sh("node -v").stdout.strip(),
        "npm": sh("npm -v").stdout.strip(),
        "python": platform.python_version(),
        "runs_per_cell": RUNS,
        "load_avg_at_start": os.getloadavg(),
    }
    results = {"machine": machine, "frameworks": {}}
    old = {}
    if (HERE / "results.json").exists():
        old = json.loads((HERE / "results.json").read_text()).get("frameworks", {})
    for fw in targets:
        recipe = RECIPES[fw]
        ws = WORK / fw
        fmt = dict(ws=ws, molde=molde_bin)
        r = {"install_seconds": None}
        shutil.rmtree(ws, ignore_errors=True)
        ws.mkdir(parents=True)
        shutil.copytree(HERE / "sites" / recipe["src"], ws / "site")
        # install: wall time from an empty workspace (node_modules removed), median of 3,
        # warm package cache (npm) / cached bottle (brew)
        INSTALL_RUNS = 3
        for step in recipe["setup"]:
            sh(step.format(**fmt), cwd=ws)
        if recipe.get("node"):
            t = []
            for _ in range(INSTALL_RUNS):
                shutil.rmtree(ws / "site/node_modules", ignore_errors=True)
                t0 = time.monotonic()
                sh("cd {ws}/site && npm ci --silent".format(**fmt))
                t.append(time.monotonic() - t0)
            r["install_seconds"] = round(med(t), 2)
            r["deps_disk_mb"] = du_mb(ws / "site/node_modules")
            lock = json.loads((ws / "site/package-lock.json").read_text())
            r["dep_count"] = sum(1 for k in lock["packages"] if k.startswith("node_modules/"))
            pkg = {"eleventy": "@11ty/eleventy", "astro": "astro", "nextjs": "next"}[fw]
            r["version"] = lock["packages"]["node_modules/" + pkg]["version"]
            r["install_what"] = "npm ci, warm npm cache, median of 3"
        elif recipe.get("brew"):
            name = recipe["brew"]
            if REINSTALL:
                t = []
                for _ in range(INSTALL_RUNS):
                    t0 = time.monotonic()
                    sh(f"brew reinstall {name}", env=BREW_ENV)
                    t.append(time.monotonic() - t0)
                r["install_seconds"] = round(med(t), 2)
            exe = shutil.which(name)
            r["deps_disk_mb"] = du_mb(exe)
            r["dep_count"] = 1
            r["install_what"] = "brew reinstall, bottle already cached, median of 3" if REINSTALL else "brew (not timed)"
            r["version"] = re.search(r"(\d+\.\d+\.\d+)", sh(recipe["version"]).stdout).group(1)
        else:
            r["install_seconds"] = 0
            r["deps_disk_mb"] = du_mb(molde_bin)
            r["dep_count"] = len(tomllib.loads((MOLDE_REPO / "Cargo.lock").read_text())["package"]) - 1
            r["install_what"] = "prebuilt single binary (crates compiled in; dep_count = crates in Cargo.lock, excluding molde itself)"
            vo = sh(recipe["version"].format(**fmt)).stdout.strip()
            r["version"] = vo.split()[1]
            r["version_output"] = vo
        files, loc = authored(HERE / "sites" / recipe["src"])
        r["files_authored"], r["loc_authored"], r["authored_files"] = len(files), loc, files

        data = ws / recipe["datafile"]
        data.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(HERE / "data.json", data)
        build = recipe["build"].format(**fmt)
        small = timed(build, ws, wipe(ws / recipe["outdir"]))
        r["fidelity"] = compare(recipe, ws)
        outdir = ws / recipe.get("cmpdir", recipe["outdir"])
        golden = {p.relative_to(HERE / "golden").as_posix() for p in (HERE / "golden").rglob("*") if p.is_file()}
        built = [p for p in outdir.rglob("*") if p.is_file() and not p.name.endswith(".gz")]
        r["extra_files"] = sum(1 for p in built if p.relative_to(outdir).as_posix() not in golden)
        r["extra_bytes"] = sum(p.stat().st_size for p in built if p.relative_to(outdir).as_posix() not in golden)
        r["build_small"] = stats(small)
        if fw == "zola":
            r["small_split"] = {"generate": stats(timed("python3 site/gen_content.py", ws)),
                                "zola_build": stats(timed("zola --root site build", ws, wipe(ws / "site/public")))}

        data.write_text(big)
        r["build_big"] = stats(timed(build, ws, wipe(ws / recipe["outdir"])))
        if fw == "zola":
            # split: generator vs binary (independent medians; need not sum)
            gen = timed("python3 site/gen_content.py", ws)
            zb = timed("zola --root site build", ws, wipe(ws / "site/public"))
            r["big_split"] = {"generate": stats(gen), "zola_build": stats(zb)}
        if fw == "molde":
            chk = f"{molde_bin} check --site site --out out"
            r["check_big"] = stats(timed(chk, ws, wipe(ws / "out")))
            data.write_text((HERE / "data.json").read_text())
            r["check_small"] = stats(timed(chk, ws, wipe(ws / "out")))
        shutil.copy(HERE / "data.json", data)
        results["frameworks"][fw] = r
        print(f"{fw:9} small={r['build_small']['median_ms']}ms big={r['build_big']['median_ms']}ms  {r['fidelity']}", flush=True)

    # merge with previously recorded rows when a subset was run
    merged = dict(old)
    merged.update(results["frameworks"])
    results["frameworks"] = {k: merged[k] for k in RECIPES if k in merged}
    (HERE / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    for fw, r in results["frameworks"].items():
        mf = BENCH / f"{fw}.metrics.json"
        m = json.loads(mf.read_text()) if mf.exists() else {"framework": fw}
        m["version"] = r["version"]
        for k in ("install_seconds", "deps_disk_mb", "dep_count", "files_authored", "loc_authored"):
            m[k] = r[k]
        m["build_ms_small"] = round(r["build_small"]["median_ms"])
        m["build_ms_big"] = round(r["build_big"]["median_ms"])
        if "small_split" in r:
            m["build_ms_small_split"] = {"gen_content_py": round(r["small_split"]["generate"]["median_ms"]),
                                         "zola_build": round(r["small_split"]["zola_build"]["median_ms"])}
            m["build_ms_big_split"] = {"gen_content_py": round(r["big_split"]["generate"]["median_ms"]),
                                       "zola_build": round(r["big_split"]["zola_build"]["median_ms"])}
        raw, norm = r["fidelity"].split("raw=")[1].split()[0], r["fidelity"].split("normalized=")[1].split()[0]
        m["fidelity_raw_diff"], m["fidelity_normalized_diff"] = int(raw), int(norm)
        if "check_big" in r:
            m["check_ms_big"] = round(r["check_big"]["median_ms"])
            m["check_ms_small"] = round(r["check_small"]["median_ms"])
        m["runs_per_cell"] = RUNS
        mf.write_text(json.dumps(m, indent=2) + "\n")
    print(f"\nwrote {HERE / 'results.json'}")


if __name__ == "__main__":
    main()
