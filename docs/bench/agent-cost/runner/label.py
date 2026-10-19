#!/usr/bin/env python3
"""Blind validation of the `claimed_done` rule (DESIGN.md §7): two people label a sample of
final messages without knowing the arm, and their labels are compared with each other and
with the structural rule in metrics.py.

    python3 runner/label.py sample --set campaign [--n 40] [--seed 20261018]
    python3 runner/label.py label  --who A      # one person, in a terminal
    python3 runner/label.py label  --who B      # the other person, separately
    python3 runner/label.py score

`sample` draws runs stratified by arm (the same number from each where possible), shuffles
them, and writes labels/sample.json with an opaque id and the final message only. Names of
generators and frameworks are replaced by "[tool]" so the arm can't be read off the text.
The mapping from id to run stays in labels/key.json, which labellers must not open.

`label` shows each message and asks one question: does the agent say it finished the task
(y), or that it is blocked, unable or unfinished (n)? Answers go to labels/<who>.json and
it resumes where it stopped.

`score` prints Cohen's kappa between the two people, and each person's agreement with the
structural rule (and with the lexical variant), for the report.
"""
import argparse
import json
import random
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
AC = HERE.parent
LABELS = AC / "labels"
TOOLS = re.compile(r"(?i)\b(molde(-[a-z]+)?|hugo|zola|eleventy|11ty|astro|next\.?js|next|site-check|"
                   r"minijinja|jinja2?|nunjucks|tsx|jsx|gotmpl|routes\.yaml|site\.yaml|\.j2|\.astro|"
                   r"getStaticPaths|generateStaticParams|app router)\b")


def cmd_sample(a):
    LABELS.mkdir(exist_ok=True)
    if (LABELS / "sample.json").exists():
        sys.exit("labels/sample.json exists; a sample is drawn once")
    rows = []
    for m in sorted((AC / "results" / a.set).glob("*/metrics.json")):
        j = json.loads(m.read_text())
        if j.get("outcome") in ("pass", "fail", "censored") and (j.get("final_message") or "").strip():
            rows.append(j)
    by_arm = {}
    for j in rows:
        by_arm.setdefault(j["arm"], []).append(j)
    rng = random.Random(a.seed)
    for v in by_arm.values():
        rng.shuffle(v)
    picked, i = [], 0
    while len(picked) < a.n and any(i < len(v) for v in by_arm.values()):
        for arm in sorted(by_arm):
            if i < len(by_arm[arm]) and len(picked) < a.n:
                picked.append(by_arm[arm][i])
        i += 1
    rng.shuffle(picked)
    sample, key = [], {}
    for n, j in enumerate(picked, start=1):
        sid = f"m{n:03d}"
        sample.append({"id": sid, "message": TOOLS.sub("[tool]", j["final_message"])})
        key[sid] = {"run_id": j["run_id"], "arm": j["arm"], "claimed_done": j["claimed_done"],
                    "claimed_done_lexical": j.get("claimed_done_lexical")}
    (LABELS / "sample.json").write_text(json.dumps(sample, indent=1, ensure_ascii=False))
    (LABELS / "key.json").write_text(json.dumps({"seed": a.seed, "set": a.set, "key": key}, indent=1))
    print(f"sampled {len(sample)} final messages from {len(by_arm)} arms -> labels/sample.json")


def cmd_label(a):
    sample = json.loads((LABELS / "sample.json").read_text())
    out = LABELS / f"{a.who}.json"
    done = json.loads(out.read_text()) if out.exists() else {}
    todo = [s for s in sample if s["id"] not in done]
    print(f"{len(done)} labelled, {len(todo)} to go. For each message: y = the agent says it finished "
          "the task; n = it says it is blocked, unable, or not finished. q = stop (resume later).\n")
    for s in todo:
        print("=" * 72)
        print(f"{s['id']}\n")
        print(s["message"].strip())
        print()
        while True:
            ans = input("finished? [y/n/q] ").strip().lower()
            if ans in ("y", "n", "q"):
                break
        if ans == "q":
            break
        done[s["id"]] = ans == "y"
        out.write_text(json.dumps(done, indent=1))
    print(f"\n{len(done)}/{len(sample)} labelled -> {out.relative_to(AC)}")


def kappa(x, y):
    n = len(x)
    if not n:
        return None
    po = sum(a == b for a, b in zip(x, y)) / n
    px, py = sum(x) / n, sum(y) / n
    pe = px * py + (1 - px) * (1 - py)
    return None if pe == 1 else (po - pe) / (1 - pe)  # undefined when every label is the same


def fmt(k):
    return "undefined (every label the same)" if k is None else f"{k:.3f}"


def cmd_score(a):
    key = json.loads((LABELS / "key.json").read_text())["key"]
    people = sorted(p.stem for p in LABELS.glob("*.json") if p.stem not in ("sample", "key"))
    labels = {p: json.loads((LABELS / f"{p}.json").read_text()) for p in people}
    ids = sorted(set.intersection(*(set(v) for v in labels.values()))) if labels else []
    print(f"{len(ids)} messages labelled by everyone ({', '.join(people)})")
    if len(people) >= 2:
        x, y = [labels[people[0]][i] for i in ids], [labels[people[1]][i] for i in ids]
        print(f"Cohen's kappa {people[0]} vs {people[1]}: {fmt(kappa(x, y))}")
    for p in people:
        for rule in ("claimed_done", "claimed_done_lexical"):
            r = [bool(key[i][rule]) for i in ids]
            h = [labels[p][i] for i in ids]
            agree = sum(a == b for a, b in zip(r, h))
            print(f"{p} vs {rule}: {agree}/{len(ids)} agree, kappa {fmt(kappa(h, r))}")


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("sample")
    s.add_argument("--set", default="campaign")
    s.add_argument("--n", type=int, default=40)
    s.add_argument("--seed", type=int, default=20261018)
    lb = sub.add_parser("label")
    lb.add_argument("--who", required=True)
    sub.add_parser("score")
    a = ap.parse_args()
    {"sample": cmd_sample, "label": cmd_label, "score": cmd_score}[a.cmd](a)


if __name__ == "__main__":
    main()
