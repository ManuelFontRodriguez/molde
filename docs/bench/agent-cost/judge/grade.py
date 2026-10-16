#!/usr/bin/env python3
"""Judge, step 2 of 2: grade. Runs in the judge image with the benchmark mounted, on the
outputs of build_site.py. It never executes workspace code, except the site's own
JavaScript inside Chromium (T8, T9), which can reach only the local copy being served.

    grade.py --built /built --task TASK --out /out

Writes verdict.json: the oracle on the final build (against the reference data), the
data-integrity findings (requirement 8), the oracle on the mutated build (against the
mutated data), the browser checks for T8 and T9, and lychee as a second link judge.
"""
import argparse
import json
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "oracle"))
import oracle  # noqa: E402

BROWSER = {"T8", "T9", "T12"}
# T12's buttons come from the data (the mutation adds a sector), so it is also judged in the
# browser on the mutated build; T8 and T9 are about behaviour, judged on the final build.
BROWSER_MUTATED = {"T12"}


def lychee(out):
    cmd = (f"lychee --offline --no-progress --format json --root-dir {out} "
           f"--remap 'https://demo\\.example/(.*) file://{out}/$1' '{out}/**/*.html'")
    r = subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=180)
    log = r.stdout + r.stderr
    try:
        return {"exit": r.returncode, "report": json.loads(log[log.index("{"):log.rindex("}") + 1])}
    except ValueError:
        return {"exit": r.returncode, "raw": log[-2000:]}


def browser(built, task, data):
    spec = {"stockPrefix": oracle.stock_prefix(task), "stocks": data["stocks"]}
    r = subprocess.run(["node", str(HERE / "browser.js"), str(built), task, json.dumps(spec)],
                       capture_output=True, text=True, timeout=300,
                       env={"PATH": "/usr/local/bin:/usr/bin:/bin", "NODE_PATH": "/opt/playwright/node_modules",
                            "PLAYWRIGHT_BROWSERS_PATH": "/opt/playwright/browsers", "HOME": "/tmp"})
    try:
        return json.loads(r.stdout.strip().splitlines()[-1])
    except (ValueError, IndexError):
        return {"ok": False, "problems": [f"browser check did not run: {r.stderr[-500:]}"]}


def grade(built_dir, task):
    built = Path(built_dir)
    rec = json.loads((built / "build.json").read_text())
    ref = rec["reference_data"]
    final = built / "final"
    b = browser(final, task, ref) if task in BROWSER and final.is_dir() else None
    v = {"task": task, "port": rec["port"], "build": rec["final"], "data_file": rec.get("data_file"),
         "integrity": rec["integrity"]}
    v["oracle"] = oracle.judge(final, task, ref, rec["final"]["ok"],
                               baseline=str(built / "start") if (built / "start").is_dir() else None,
                               integrity=rec["integrity"], browser=b)
    if "mutated" in rec:
        # the browser checks (T8, T9) run once, on the final build; the mutated build is about
        # whether the pages follow the data
        skip = {"ok": True, "problems": [], "note": "browser checks run on the final build only"}
        mb = browser(built / "mutated", task, rec["mutated_data"]) \
            if task in BROWSER_MUTATED and (built / "mutated").is_dir() else skip
        mv = oracle.judge(built / "mutated", task, rec["mutated_data"], rec["mutated"]["ok"],
                          browser=mb if task in BROWSER else None)
        v["mutation"] = {"build_ok": rec["mutated"]["ok"], "log": rec["mutated"]["log"], "oracle": mv}
    else:
        v["mutation"] = {"build_ok": False, "log": "", "oracle": {"pass": False, "note": "no data to mutate"}}
    v["browser"] = b
    v["lychee"] = lychee(final) if final.is_dir() else None
    # second judge: lychee's broken links vs the oracle's R2 verdict
    rep = (v["lychee"] or {}).get("report") or {}
    lychee_broken = bool(rep.get("errors"))
    oracle_broken = not v["oracle"]["stated"].get("R2_links", {"ok": True})["ok"]
    v["second_judge"] = {"lychee_broken_links": lychee_broken, "oracle_broken_links": oracle_broken,
                         "agree": lychee_broken == oracle_broken,
                         "note": "lychee checks href/src (and remapped absolute URLs); it does not read CSS url() "
                                 "or srcset, so oracle-only failures of those kinds are expected disagreements"}
    v["pass"] = bool(v["oracle"]["pass"] and v["mutation"]["oracle"]["pass"])
    return v


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--built", required=True)
    ap.add_argument("--task", required=True)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    if not (Path(a.built) / "build.json").exists():
        # the build step itself failed: a harness error, never the arm's failure
        v = {"task": a.task, "pass": None, "judge_error": "the judge's build step produced no build.json (see judge.log)"}
        (out / "verdict.json").write_text(json.dumps(v, indent=1))
        print(json.dumps(v))
        return
    v = grade(a.built, a.task)
    (out / "verdict.json").write_text(json.dumps(v, indent=1, ensure_ascii=False))
    print(json.dumps({"pass": v["pass"], "oracle": v["oracle"]["pass"], "mutation": v["mutation"]["oracle"]["pass"],
                      "build_ok": v["build"]["ok"], "integrity": len(v["integrity"])}))


if __name__ == "__main__":
    main()
