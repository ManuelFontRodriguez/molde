#!/usr/bin/env python3
"""Print a verdict.json in a few lines: problems first, then pages, notes and lychee."""
import json
import sys

v = json.load(open(sys.argv[1]))
if v.get("error"):
    print("   error:", v["error"])
    sys.exit(0)
if not v["pass"]:
    if not v["build"]["ok"]:
        print("   build log:", v["build"]["log"][-800:])
    for label, o in (("", v["oracle"]), ("mutation ", v["mutation"]["oracle"])):
        for k, x in o["stated"].items():
            if x["problems"]:
                print(f"   {label}{k}: {x['problems'][:4]}")
        if o["task"]["problems"]:
            print(f"   {label}task: {o['task']['problems'][:4]}")
if v.get("integrity"):
    print("   integrity:", v["integrity"][:4])
if v.get("browser"):
    print("   browser:", json.dumps(v["browser"])[:400])
ly = v.get("lychee") or {}
rep = ly.get("report") or {}
print(f"   pages {v['oracle']['pages']} | notes {v['oracle']['unstated']['notes'][:3]} | lychee exit {ly.get('exit')}"
      + (f" errors {rep.get('errors')} total {rep.get('total')}" if rep else f" {ly.get('raw', '')[:300]}"))
