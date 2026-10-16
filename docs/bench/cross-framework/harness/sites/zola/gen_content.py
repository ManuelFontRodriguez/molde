#!/usr/bin/env python3
"""Zola cannot build pages from a data file; emit one content file per stock."""
import json, pathlib, shutil, sys

site = pathlib.Path(__file__).parent
data = json.loads((site / (sys.argv[1] if len(sys.argv) > 1 else "data.json")).read_text())

stocks_dir = site / "content" / "stocks"
shutil.rmtree(stocks_dir, ignore_errors=True)
stocks_dir.mkdir(parents=True)
(stocks_dir / "_index.md").write_text("+++\nrender = false\n+++\n")

for s in data["stocks"]:
    fm = [
        "+++",
        f'title = {json.dumps(s["name"] + " (" + s["ticker"] + ")")}',
        f'path = "stocks/{s["ticker"]}"',
        'template = "stock.html"',
        "[extra]",
        f'name = {json.dumps(s["name"])}',
        f'ticker = {json.dumps(s["ticker"])}',
        f'sector = {json.dumps(s["sector"])}',
    ]
    if s["upcoming"]:
        u = s["upcoming"]
        fm.append(
            f'upcoming = {{ ex_date = {json.dumps(u["ex_date"])}, amount = {json.dumps(u["amount"])},'
            f' currency = {json.dumps(u["currency"])}, confirmed = {"true" if u["confirmed"] else "false"} }}'
        )
    fm.append("+++")
    (stocks_dir / s["ticker"].lower()).mkdir()
    (stocks_dir / s["ticker"].lower() / "index.md").write_text("\n".join(fm) + "\n")
