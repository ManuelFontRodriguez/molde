#!/usr/bin/env python3
"""Compare a build dir against the golden target. Usage: compare.py <build_dir> [golden_dir]"""
import sys, difflib
from pathlib import Path

build = Path(sys.argv[1])
golden = Path(sys.argv[2] if len(sys.argv) > 2 else Path(__file__).parent / "golden")

def norm(text):  # cosmetic-insensitive: strip trailing ws, drop blank lines
    return [l.rstrip() for l in text.splitlines() if l.strip()]

targets = sorted(p.relative_to(golden) for p in golden.rglob("*") if p.is_file())
total_raw, total_norm, missing = 0, 0, []
for rel in targets:
    got = build / rel
    if not got.is_file():
        missing.append(str(rel)); continue
    g, b = (golden / rel).read_text(), got.read_text()
    raw = 0 if g == b else sum(1 for l in difflib.unified_diff(g.splitlines(), b.splitlines(), lineterm="") if l[:1] in "+-" and l[:3] not in ("+++","---"))
    nrm = 0 if norm(g) == norm(b) else sum(1 for l in difflib.unified_diff(norm(g), norm(b), lineterm="") if l[:1] in "+-" and l[:3] not in ("+++","---"))
    total_raw += raw; total_norm += nrm
    print(f"{rel}: raw_diff_lines={raw} normalized_diff_lines={nrm}")
extras = sorted(str(p.relative_to(build)) for p in build.rglob("*.html") if p.is_file() and not (golden / p.relative_to(build)).is_file())
print(f"TOTAL raw={total_raw} normalized={total_norm} missing={missing} extra_html={extras[:10]}{'...' if len(extras)>10 else ''}")
