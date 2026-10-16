#!/usr/bin/env python3
"""Self-test of the oracle: it must pass a correct site and fail each kind of defect,
on the rule that names it. Run before anything is trusted (and again whenever the
oracle changes):

    python3 oracle/selftest.py [SITE_DIR] [DATA_JSON]

Defaults to the build benchmark's golden site and dataset. Each mutation is applied to
a fresh copy of the site; the expected outcome is (rule or "task", must fail). Exit code
0 only when every expectation holds.
"""
import json
import shutil
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import oracle  # noqa: E402

BENCH = HERE.parent.parent
SITE = Path(sys.argv[1]) if len(sys.argv) > 1 else BENCH / "cross-framework/harness/golden"
DATA = Path(sys.argv[2]) if len(sys.argv) > 2 else BENCH / "cross-framework/harness/data.json"


def edit(rel, old, new, count=1):
    def f(d):
        p = d / rel
        s = p.read_text()
        assert old in s, f"mutation anchor not found in {rel}: {old!r}"
        p.write_text(s.replace(old, new, count))
    return f


def delete(rel):
    return lambda d: (d / rel).unlink()


def add_file(rel, text):
    def f(d):
        (d / rel).parent.mkdir(parents=True, exist_ok=True)
        (d / rel).write_text(text)
    return f


KO = "stocks/KO/index.html"
# (name, mutation, task, rule that must fail; "task" = a task assertion)
CASES = [
    # defects molde detects
    ("broken absolute href", edit("index.html", 'href="/stocks/KO/"', 'href="/stocks/KOO/"'), "T0", "R2_links"),
    ("duplicate title", edit(KO, "<title>Coca-Cola Co (KO)</title>", "<title>PepsiCo Inc (PEP)</title>"), "T0", "R4_titles"),
    ("missing canonical", edit(KO, '<link rel="canonical" href="https://demo.example/stocks/KO/">\n', ""), "T0", "R5_canonical_og"),
    ("wrong og:url", edit(KO, 'og:url" content="https://demo.example/stocks/KO/"', 'og:url" content="https://demo.example/stocks/PEP/"'), "T0", "R5_canonical_og"),
    ("page missing from sitemap", edit("sitemap.xml", "  <url><loc>https://demo.example/stocks/PEP/</loc></url>\n", ""), "T0", "R3_sitemap"),
    ("sitemap lists a non-page", edit("sitemap.xml", "</urlset>", "  <url><loc>https://demo.example/gone/</loc></url>\n</urlset>"), "T0", "R3_sitemap"),
    ("double escape", edit("index.html", "AT&amp;T Inc", "AT&amp;amp;T Inc"), "T0", "R6_escaping"),
    ("double escape in title", edit("stocks/T/index.html", "<title>AT&amp;T Inc (T)</title>", "<title>AT&amp;amp;T Inc (T)</title>"), "T0", "R6_escaping"),
    ("invalid JSON-LD", edit(KO, "</head>", '<script type="application/ld+json">{"@type": "Corporation",}</script>\n</head>'), "T0", "R7_jsonld"),
    ("robots.txt missing", delete("robots.txt"), "T0", "R9_robots"),
    ("no title", edit(KO, "<title>Coca-Cola Co (KO)</title>", ""), "T0", "R4_titles"),
    ("two titles", edit(KO, "</head>", "<title>Extra</title>\n</head>"), "T0", "R4_titles"),
    # defects molde's checks do not see (reference.md: "What the checks do not see")
    ("broken relative link", edit(KO, 'href="/"', 'href="../KOO/"'), "T0", "R2_links"),
    ("broken single-quoted href", edit("index.html", 'href="/stocks/PEP/"', "href='/stocks/PEPX/'"), "T0", "R2_links"),
    ("broken unquoted href", edit("index.html", 'href="/stocks/SAN/"', "href=/stocks/SANX/"), "T0", "R2_links"),
    ("CSS url() to a missing file", edit("style.css", "body {", "body { background: url('/img/missing.png');", ), "T0", "R2_links"),
    ("CSS relative url() missing", edit("style.css", "body {", "body { background: url(missing.png);"), "T0", "R2_links"),
    ("inline style url() missing", edit(KO, "<body>", '<body style="background:url(/nope.png)">'), "T0", "R2_links"),
    ("missing stylesheet", delete("style.css"), "T0", "R2_links"),
    ("wrong value on one page", edit(KO, "0.5100 USD", "0.2775 USD"), "T0", "task"),
    ("wrong name on one page", edit("stocks/PEP/index.html", "<h1>PepsiCo Inc", "<h1>Coca-Cola Co"), "T0", "task"),
    ("badge on a confirmed dividend", edit(KO, "</section>", '<p class="badge">Estimated — not yet declared</p>\n</section>'), "T0", "task"),
    ("badge missing on an estimate", edit("stocks/PEP/index.html", '<p class="badge">', '<p class="note">'), "T0", "task"),
    ("stale cycle on the home page", edit("index.html", "cycle 2026-09-18T09:00:00Z-demo", "cycle 2026-09-11T09:00:00Z-demo"), "T0", "task"),
    ("no-dividend text missing", edit("stocks/SAN/index.html", "No upcoming dividend declared.", ""), "T0", "task"),
    ("index no longer links a stock", edit("index.html", '<li><a href="/stocks/SAN/">Banco Santander</a> — Financial Services</li>', "<li>Banco Santander</li>"), "T0", "task"),
    ("extra unlisted page", add_file("stocks/ZZZ/index.html", "<!DOCTYPE html><html lang=en><head><title>Z</title></head><body></body></html>"), "T0", "R3_sitemap"),
    # task-specific: the unchanged golden must fail every task that asks for a change
    ("T1 not done", None, "T1", "task"),
    ("T2 not done", None, "T2", "task"),
    ("T3 not done", None, "T3", "task"),
    ("T5 not done", None, "T5", "task"),
    ("T6 not done", None, "T6", "task"),
    ("T7 not done", None, "T7", "task"),
    ("T8 not done", None, "T8", "task"),
    ("T9 not done", None, "T9", "task"),
    ("T11 not done", None, "T11", "task"),
    ("T12 not done", None, "T12", "task"),
    ("T11 in an order other than by name", edit("index.html",
        '<li><a href="/stocks/KO/">Coca-Cola Co</a> — Consumer Defensive</li>\n\n  <li><a href="/stocks/PEP/">PepsiCo Inc</a> — Consumer Defensive</li>\n\n  <li><a href="/stocks/SAN/">Banco Santander</a> — Financial Services</li>\n\n  <li><a href="/stocks/T/">AT&amp;T Inc</a> — Communication Services</li>',
        '<li><a href="/stocks/T/">AT&amp;T Inc</a> — Communication Services</li>\n<li><a href="/stocks/SAN/">Banco Santander</a> — Financial Services</li>\n<li><a href="/stocks/PEP/">PepsiCo Inc</a> — Consumer Defensive</li>\n<li><a href="/stocks/KO/">Coca-Cola Co</a> — Consumer Defensive</li>'), "T11", "task"),
]
# Positive T11 case: the golden list re-ordered by name must pass T11.
T11_SORTED = edit("index.html",
    '<li><a href="/stocks/KO/">Coca-Cola Co</a> — Consumer Defensive</li>\n\n  <li><a href="/stocks/PEP/">PepsiCo Inc</a> — Consumer Defensive</li>\n\n  <li><a href="/stocks/SAN/">Banco Santander</a> — Financial Services</li>\n\n  <li><a href="/stocks/T/">AT&amp;T Inc</a> — Communication Services</li>',
    '<li><a href="/stocks/T/">AT&amp;T Inc</a> — Communication Services</li>\n<li><a href="/stocks/SAN/">Banco Santander</a> — Financial Services</li>\n<li><a href="/stocks/KO/">Coca-Cola Co</a> — Consumer Defensive</li>\n<li><a href="/stocks/PEP/">PepsiCo Inc</a> — Consumer Defensive</li>')


def main():
    data = json.loads(DATA.read_text())
    data_t1 = json.loads(DATA.read_text())
    for i, s in enumerate(data_t1["stocks"]):
        s["dividend_yield"] = f"{3.1 + i:.2f}%"
    failures = 0
    base = oracle.judge(SITE, "T0", data, True)
    print(f"{'PASS' if base['pass'] else 'FAIL'}  unchanged site passes T0")
    if not base["pass"]:
        failures += 1
        print(json.dumps(base, indent=1))
    base10 = oracle.judge(SITE, "T10", data, True, baseline=SITE)
    print(f"{'PASS' if base10['pass'] else 'FAIL'}  unchanged site passes T10 (nothing to do)")
    failures += not base10["pass"]
    for name, mut, task, rule in CASES:
        with tempfile.TemporaryDirectory() as t:
            d = Path(t) / "site"
            shutil.copytree(SITE, d)
            if mut:
                mut(d)
            v = oracle.judge(d, task, data_t1 if task == "T1" else data, True)
            failed_rule = (not v["task"]["ok"]) if rule == "task" else (not v["stated"][rule]["ok"])
            ok = (not v["pass"]) and failed_rule
            failures += not ok
            print(f"{'PASS' if ok else 'FAIL'}  {name}: fails {rule}" + ("" if ok else f"   <- got {json.dumps(v)[:600]}"))
    with tempfile.TemporaryDirectory() as t:
        d = Path(t) / "site"
        shutil.copytree(SITE, d)
        T11_SORTED(d)
        v = oracle.judge(d, "T11", data, True)
        print(f"{'PASS' if v['pass'] else 'FAIL'}  the list sorted by name passes T11" + ("" if v["pass"] else f"   <- {v['task']}"))
        failures += not v["pass"]
    # build failure is reported as R1
    v = oracle.judge(SITE, "T0", data, False)
    ok = not v["pass"] and not v["stated"]["R1_build"]["ok"]
    failures += not ok
    print(f"{'PASS' if ok else 'FAIL'}  failed build: fails R1_build")
    print(f"\n{len(CASES) + 4 - failures}/{len(CASES) + 4} expectations hold")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
