#!/usr/bin/env python3
"""The independent judge of the agent-cost benchmark.

Grades one built site against requirements.md (the "stated" rules, identical for every
arm) and against one task's assertions. Plain Python standard library; no molde code and
no code shared with site-check (the checker some arms get), so neither molde nor the
checker grades itself.

    oracle.py --out BUILD_DIR --task T3 --data DATA_JSON [--build-ok yes|no]
              [--baseline BASELINE_BUILD_DIR] [--mutated]

Prints one JSON verdict to stdout:
    {"pass": bool, "stated": {rule: {"ok": bool, "problems": [...]}},
     "task": {"ok": bool, "problems": [...]}, "unstated": {...}, "pages": [...]}

`pass` is true only when every stated rule and every task assertion holds. "Unstated"
qualities are reported, never scored.

Page set: every .html file in the build, except 404 pages (404.html, 404/index.html) and
anything under a top-level directory starting with "_" (framework assets such as
_next/, _astro/). A file a/b/index.html is the page /a/b/; index.html is /; any other
x.html is /x.html (reported as an unstated note, since the requirements ask for pretty
URLs).
"""
import argparse
import html
import json
import re
import sys
import xml.etree.ElementTree as ET
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urljoin, urlsplit, unquote

BASE = "https://demo.example"
HOST = "demo.example"


# --------------------------------------------------------------------------- parsing

class Page(HTMLParser):
    """Collects what the rules need from one HTML document. html.parser handles single-
    quoted, double-quoted and unquoted attributes alike and decodes character references,
    so `text` and attribute values are what a browser shows."""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.titles = []          # decoded text of each <title>
        self.canonicals = []      # href of each link rel~=canonical
        self.metas = []           # (property-or-name, content) for every <meta>
        self.refs = []            # (attribute, value) for href/src/srcset/style url()
        self.jsonld = []          # raw text of each ld+json script
        self.styles = []          # text of <style> blocks
        self.text = []            # visible text chunks (outside script/style/title)
        self.html_lang = None
        self.inputs = 0
        self.class_count = {}     # class name -> number of elements carrying it
        self.scripts = 0
        self._in = []             # open element stack for the few we care about
        self._buf = None

    def handle_starttag(self, tag, attrs):
        a = {k.lower(): (v if v is not None else "") for k, v in attrs}
        if tag == "html":
            self.html_lang = a.get("lang")
        if tag == "title":
            self._in.append("title"); self._buf = []
        elif tag == "script":
            self.scripts += 1
            kind = "jsonld" if a.get("type", "").strip().lower() == "application/ld+json" else "script"
            self._in.append(kind); self._buf = []
        elif tag == "style":
            self._in.append("style"); self._buf = []
        if tag == "link" and "canonical" in a.get("rel", "").lower().split():
            self.canonicals.append(a.get("href", ""))
        if tag == "meta":
            key = a.get("property") or a.get("name")
            if key:
                self.metas.append((key.strip().lower(), a.get("content", "")))
        if tag == "input":
            self.inputs += 1
        for c in a.get("class", "").split():
            self.class_count[c] = self.class_count.get(c, 0) + 1
        for attr in ("href", "src"):
            if attr in a and not (tag == "link" and "canonical" in a.get("rel", "").lower().split()):
                # <a href>, <link href> (stylesheet, icon...), <img src>, <script src>...
                if tag == "base":
                    continue
                self.refs.append((f"{tag}[{attr}]", a[attr]))
        if "srcset" in a:
            for part in a["srcset"].split(","):
                url = part.strip().split(" ")[0]
                if url:
                    self.refs.append((f"{tag}[srcset]", url))
        if "style" in a:
            for u in css_urls(a["style"]):
                self.refs.append((f"{tag}[style]", u))

    def handle_startendtag(self, tag, attrs):
        self.handle_starttag(tag, attrs)

    def handle_endtag(self, tag):
        if self._in and tag in ("title", "script", "style"):
            kind = self._in.pop()
            body = "".join(self._buf or [])
            if kind == "title":
                self.titles.append(body.strip())
            elif kind == "jsonld":
                self.jsonld.append(body)
            elif kind == "style":
                self.styles.append(body)
            self._buf = None

    def handle_data(self, data):
        if self._in:
            if self._buf is not None:
                self._buf.append(data)
        else:
            self.text.append(data)

    def meta(self, key):
        return [c for k, c in self.metas if k == key]

    def visible_text(self):
        return re.sub(r"\s+", " ", "".join(self.text)).strip()


CSS_URL = re.compile(r"""url\(\s*(['"]?)(.*?)\1\s*\)""", re.I)
CSS_IMPORT = re.compile(r"""@import\s+(['"])(.*?)\1""", re.I)


def css_urls(css):
    out = [m.group(2) for m in CSS_URL.finditer(css)]
    out += [m.group(2) for m in CSS_IMPORT.finditer(css)]
    return [u for u in out if u and not u.startswith("data:")]


# --------------------------------------------------------------------------- the site

def page_url(rel):
    """Build-relative file path -> URL path."""
    p = rel.as_posix()
    if p == "index.html":
        return "/"
    if p.endswith("/index.html"):
        return "/" + p[: -len("index.html")]
    return "/" + p


def is_page(rel):
    parts = rel.parts
    if parts[0].startswith("_"):
        return False
    if rel.as_posix() in ("404.html", "404/index.html"):
        return False
    return rel.suffix == ".html"


REFRESH_STUB = re.compile(r'<meta[^>]+http-equiv\s*=\s*["\']?refresh', re.I)


def load_site(out, stubs=None):
    """Pages of the build. A redirect stub (a document whose content is a meta refresh,
    such as Hugo's /page/1/ alias) is not a page; it is listed in `stubs` (review 2 B5)."""
    pages = {}
    for f in sorted(out.rglob("*.html")):
        rel = f.relative_to(out)
        if is_page(rel):
            text = f.read_text(encoding="utf-8", errors="replace")
            p = Page()
            p.feed(text)
            p.close()
            if REFRESH_STUB.search(text) and len(p.visible_text()) < 200 and not p.titles[1:]:
                if stubs is not None:
                    stubs.append(page_url(rel))
                continue
            pages[page_url(rel)] = (rel, p)
    return pages


def resolves(out, url_path):
    """Does a site URL path name a file in the build? A path ending in / needs index.html;
    a path naming a directory is accepted if the directory has index.html (static hosts
    redirect /x to /x/), and noted as unstated."""
    path = unquote(url_path)
    if not path.startswith("/"):
        return False, None
    target = out / path.lstrip("/")
    if path.endswith("/"):
        return (target / "index.html").is_file(), None
    if target.is_file():
        return True, None
    if target.is_dir() and (target / "index.html").is_file():
        return True, "link to a directory without trailing slash"
    return False, None


def internal(url, page_full_url):
    """Absolute URL of a reference if it points into this site, else None."""
    u = url.strip()
    if not u or u.startswith(("#", "mailto:", "tel:", "javascript:", "data:")):
        return None
    full = urljoin(page_full_url, u)
    parts = urlsplit(full)
    if parts.scheme not in ("http", "https") or parts.hostname != HOST:
        return None
    return parts.path or "/"


# --------------------------------------------------------------------------- the rules

ESCAPE_DEBRIS = re.compile(r"&(amp|lt|gt|quot|apos|#\d+|#x[0-9a-f]+);", re.I)


def check_stated(out, pages, data, build_ok, integrity=()):
    R = {k: [] for k in ("R1_build", "R2_links", "R3_sitemap", "R4_titles",
                         "R5_canonical_og", "R6_escaping", "R7_jsonld", "R8_data", "R9_robots")}
    R["R8_data"] += list(integrity)
    notes = []
    if not build_ok:
        R["R1_build"].append("the build command failed")
    if not pages:
        R["R1_build"].append("the build produced no pages")

    # R2: links in pages, and url()/@import in every stylesheet in the build.
    for url, (rel, p) in pages.items():
        full = BASE + url
        refs = list(p.refs) + [("style", u) for s in p.styles for u in css_urls(s)]
        for where, ref in refs:
            path = internal(ref, full)
            if path is None:
                continue
            ok, note = resolves(out, path)
            if not ok:
                R["R2_links"].append(f"{url}: {where} '{ref}' -> {path} does not exist")
            elif note:
                notes.append(f"{url}: {where} '{ref}': {note}")
    for css in sorted(out.rglob("*.css")):
        rel = css.relative_to(out)
        if rel.parts[0].startswith("_"):
            continue
        css_full = BASE + "/" + rel.as_posix()
        for ref in css_urls(css.read_text(encoding="utf-8", errors="replace")):
            path = internal(ref, css_full)
            if path is not None and not resolves(out, path)[0]:
                R["R2_links"].append(f"/{rel.as_posix()}: url('{ref}') -> {path} does not exist")

    # R3: the sitemap lists every page exactly once and nothing else.
    sm = out / "sitemap.xml"
    if not sm.is_file():
        R["R3_sitemap"].append("/sitemap.xml is missing")
    else:
        try:
            root = ET.fromstring(sm.read_bytes())
            locs = [e.text.strip() for e in root.iter() if e.tag.endswith("loc") and e.text]
            expected = {BASE + u for u in pages}
            seen = set()
            for loc in locs:
                if loc in seen:
                    R["R3_sitemap"].append(f"listed twice: {loc}")
                seen.add(loc)
            for loc in sorted(expected - seen):
                R["R3_sitemap"].append(f"page missing from the sitemap: {loc}")
            for loc in sorted(seen - expected):
                R["R3_sitemap"].append(f"sitemap lists a URL that is not a page: {loc}")
        except ET.ParseError as e:
            R["R3_sitemap"].append(f"sitemap.xml is not valid XML: {e}")

    # R4: one non-empty title per page, unique across the site.
    by_title = {}
    for url, (rel, p) in pages.items():
        if len(p.titles) != 1:
            R["R4_titles"].append(f"{url}: {len(p.titles)} <title> elements")
        elif not p.titles[0]:
            R["R4_titles"].append(f"{url}: empty <title>")
        else:
            by_title.setdefault(p.titles[0], []).append(url)
    for t, urls in by_title.items():
        if len(urls) > 1:
            R["R4_titles"].append(f"title '{t}' shared by {', '.join(sorted(urls))}")

    # R5: canonical and og:url equal the page's own URL; og:title equals the title.
    for url, (rel, p) in pages.items():
        full = BASE + url
        ogu = p.meta("og:url")
        ogt = p.meta("og:title")
        if len(p.canonicals) != 1:
            R["R5_canonical_og"].append(f"{url}: {len(p.canonicals)} canonical links")
        elif p.canonicals[0] != full:
            R["R5_canonical_og"].append(f"{url}: canonical is '{p.canonicals[0]}', expected '{full}'")
        if len(ogu) != 1:
            R["R5_canonical_og"].append(f"{url}: {len(ogu)} og:url tags")
        elif ogu[0] != full:
            R["R5_canonical_og"].append(f"{url}: og:url is '{ogu[0]}', expected '{full}'")
        if len(ogt) != 1:
            R["R5_canonical_og"].append(f"{url}: {len(ogt)} og:title tags")
        elif p.titles and ogt[0] != p.titles[0]:
            R["R5_canonical_og"].append(f"{url}: og:title '{ogt[0]}' differs from title '{p.titles[0]}'")

    # R6: what the browser shows contains no escape debris (double escaping), and every
    # stock name from the data appears as written on its own page.
    for url, (rel, p) in pages.items():
        shown = [p.visible_text()] + p.titles + [c for _, c in p.metas]
        for s in shown:
            m = ESCAPE_DEBRIS.search(s)
            if m:
                R["R6_escaping"].append(f"{url}: shows '{m.group(0)}' (escaped twice)")
                break

    # R7: JSON-LD parses.
    for url, (rel, p) in pages.items():
        for i, raw in enumerate(p.jsonld):
            try:
                json.loads(raw)
            except ValueError as e:
                R["R7_jsonld"].append(f"{url}: JSON-LD block {i + 1} is not valid JSON ({e})")

    # R9: robots.txt names the sitemap.
    rb = out / "robots.txt"
    if not rb.is_file():
        R["R9_robots"].append("/robots.txt is missing")
    elif not re.search(r"(?im)^\s*sitemap:\s*" + re.escape(BASE + "/sitemap.xml") + r"\s*$", rb.read_text(errors="replace")):
        R["R9_robots"].append("/robots.txt does not name " + BASE + "/sitemap.xml")

    # Unstated qualities, reported only.
    for url, (rel, p) in pages.items():
        if not url.endswith("/"):
            notes.append(f"{url}: not a pretty URL")
        if not p.html_lang:
            notes.append(f"{url}: <html> has no lang")
    return R, notes


# --------------------------------------------------------------------------- the tasks

def slug(s):
    """T2's rule, exactly as the prompt states it: lowercase, spaces replaced by hyphens."""
    return s.lower().replace(" ", "-")


def stock_prefix(task):
    return "/acciones/" if task == "T3" else "/stocks/"


def expected_pages(task, data):
    pre = stock_prefix(task)
    pages = {"/"} | {f"{pre}{s['ticker']}/" for s in data["stocks"]}
    if task == "T2":
        pages |= {f"/sectors/{slug(s['sector'])}/" for s in data["stocks"]}
    if task == "T6":
        n = (len(data["stocks"]) + 1) // 2
        pages |= {f"/page/{i}/" for i in range(2, n + 1)}
    return pages


def has_link(p, page_url_path, target):
    """Does the page link to target? /x and /x/ are the same page (as in resolves())."""
    full = BASE + page_url_path
    t = target.rstrip("/")
    return any((internal(ref, full) or "").rstrip("/") == t for where, ref in p.refs if where.startswith("a["))


def amount_line(s):
    u = s.get("upcoming")
    return None if not u else (u["amount"], u["currency"], u["ex_date"])


def check_task(task, out, pages, data, baseline, browser=None):
    probs = []
    pre = stock_prefix(task)
    want = expected_pages(task, data)
    have = set(pages)
    for u in sorted(want - have):
        probs.append(f"expected page missing: {u}")
    extra = have - want
    if task == "T2":
        extra -= {"/sectors/"}   # an index of the sectors is a reasonable extra (Hugo taxonomies make one)
    for u in sorted(extra):
        probs.append(f"unexpected page: {u}")
    if task == "T3":
        for f in sorted(out.rglob("*")):
            if f.is_file() and f.relative_to(out).parts[0] == "stocks":
                probs.append(f"old location still built: /{f.relative_to(out).as_posix()}")
                break
        for url, (rel, p) in pages.items():
            for where, ref in p.refs:
                path = internal(ref, BASE + url)
                if path and path.startswith("/stocks/"):
                    probs.append(f"{url}: still links to the old location {path}")

    # Content that must hold in every task: each stock page shows its own name, ticker,
    # sector and dividend line; the index links to every stock page.
    index = pages.get("/")
    if index and data.get("cycle_id") and task != "T6" and data["cycle_id"] not in index[1].visible_text():
        probs.append(f"/: does not show the data's cycle '{data['cycle_id']}'")
    for s in data["stocks"]:
        u = f"{pre}{s['ticker']}/"
        if u not in pages:
            continue
        txt = pages[u][1].visible_text()
        for field in ("name", "ticker", "sector"):
            if s[field] not in txt:
                probs.append(f"{u}: does not show its {field} '{s[field]}'")
        line = amount_line(s)
        if line:
            for v in line:
                if v not in txt:
                    probs.append(f"{u}: does not show '{v}' from its upcoming dividend")
        elif "No upcoming dividend declared." not in txt:
            probs.append(f"{u}: has no upcoming dividend but does not say 'No upcoming dividend declared.'")
        badge_due = bool(s.get("upcoming")) and not s["upcoming"].get("confirmed")
        badges = pages[u][1].class_count.get("badge", 0)
        if badge_due and badges != 1:
            probs.append(f"{u}: the dividend is not confirmed, so it needs one 'Estimated' badge (found {badges})")
        if not badge_due and badges:
            probs.append(f"{u}: shows an 'Estimated' badge, but its dividend is confirmed or absent")
        if task == "T6":
            continue
        if index and not has_link(index[1], "/", u):
            probs.append(f"/: does not link to {u}")

    if task == "T1":
        for s in data["stocks"]:
            y = s.get("dividend_yield")
            u = f"{pre}{s['ticker']}/"
            if y is None or u not in pages:
                continue
            if y not in pages[u][1].visible_text():
                probs.append(f"{u}: does not show its dividend yield '{y}'")
            if index and y not in index[1].visible_text():
                probs.append(f"/: does not show {s['ticker']}'s dividend yield '{y}'")

    if task == "T2":
        for s in data["stocks"]:
            su = f"/sectors/{slug(s['sector'])}/"
            u = f"{pre}{s['ticker']}/"
            if u in pages and not has_link(pages[u][1], u, su):
                probs.append(f"{u}: does not link to its sector page {su}")
            if su in pages and not has_link(pages[su][1], su, u):
                probs.append(f"{su}: does not list {u}")
        for su in {f"/sectors/{slug(s['sector'])}/" for s in data["stocks"]}:
            if su in pages:
                members = {f"{pre}{s['ticker']}/" for s in data["stocks"] if f"/sectors/{slug(s['sector'])}/" == su}
                linked = {internal(r, BASE + su) for w, r in pages[su][1].refs if w.startswith("a[")}
                for other in sorted((linked & {f"{pre}{s['ticker']}/" for s in data["stocks"]}) - members):
                    probs.append(f"{su}: lists {other}, which is in another sector")

    if task == "T5":
        for url, (rel, p) in pages.items():
            d = p.meta("og:description")
            if url == "/":
                exp = "Upcoming dividends for every stock we track."
            else:
                s = next((s for s in data["stocks"] if url == f"{pre}{s['ticker']}/"), None)
                exp = f"{s['name']} ({s['ticker']}): {s['sector']}." if s else None
            if exp is None:
                continue
            if d != [exp]:
                probs.append(f"{url}: og:description is {d}, expected ['{exp}']")

    if task == "T6":
        stocks = data["stocks"]
        chunks = [stocks[i:i + 2] for i in range(0, len(stocks), 2)]
        for i, chunk in enumerate(chunks, start=1):
            u = "/" if i == 1 else f"/page/{i}/"
            if u not in pages:
                continue
            p = pages[u][1]
            for s in stocks:
                target = f"{pre}{s['ticker']}/"
                on_page = has_link(p, u, target)
                if s in chunk and not on_page:
                    probs.append(f"{u}: does not list {target}")
                if s not in chunk and on_page:
                    probs.append(f"{u}: lists {target}, which belongs on another page")
            if i < len(chunks) and not has_link(p, u, f"/page/{i + 1}/"):
                probs.append(f"{u}: no link to the next page /page/{i + 1}/")
            if i > 1:
                prev = "/" if i == 2 else f"/page/{i - 1}/"
                if not has_link(p, u, prev):
                    probs.append(f"{u}: no link to the previous page {prev}")

    if task == "T7":
        for s in data["stocks"]:
            u = f"{pre}{s['ticker']}/"
            if u not in pages:
                continue
            blocks = []
            for raw in pages[u][1].jsonld:
                try:
                    blocks.append(json.loads(raw))
                except ValueError:
                    pass
            flat = []
            for b in blocks:
                flat += b if isinstance(b, list) else (b.get("@graph", [b]) if isinstance(b, dict) else [])
            ok = any(isinstance(b, dict) and b.get("@type") == "Corporation"
                     and b.get("name") == s["name"] and b.get("tickerSymbol") == s["ticker"] for b in flat)
            if not ok:
                probs.append(f"{u}: no JSON-LD Corporation with name '{s['name']}' and tickerSymbol '{s['ticker']}'")

    if task == "T11" and index:
        # the home page links to the stock pages in A-to-Z order of name (case-insensitive),
        # whatever order data.json has (the mutation test adds stocks at both ends)
        want_order = [f"{pre}{s['ticker']}/" for s in sorted(data["stocks"], key=lambda s: s["name"].casefold())]
        seen = []
        for where, ref in index[1].refs:
            path = internal(ref, BASE + "/") if where.startswith("a[") else None
            path = (path.rstrip("/") + "/") if path else None
            if path in want_order and path not in seen:
                seen.append(path)
        if seen != want_order:
            probs.append(f"/: stocks listed in the order {seen}, expected by name {want_order}")

    if task in ("T8", "T9", "T12") and browser is not None:
        probs += [f"browser: {x}" for x in browser.get("problems", [])]
    if task == "T8" and browser is None:
        css = "\n".join(c.read_text(errors="replace") for c in out.rglob("*.css"))
        css += "\n".join(s for _, (r, p) in pages.items() for s in p.styles)
        red = re.compile(r"color\s*:\s*(red|#f00\b|#ff0000\b|rgb\(\s*255\s*,\s*0\s*,\s*0\s*\))", re.I)
        bold = re.compile(r"font-weight\s*:\s*(bold|bolder|[7-9]00)\b", re.I)
        rules = re.findall(r"([^{}]+)\{([^}]*)\}", re.sub(r"/\*.*?\*/", "", css, flags=re.S))
        hit = [(sel, body) for sel, body in rules if re.search(r"\.badge\b", sel)]
        if not any(red.search(b) for _, b in hit):
            probs.append("no CSS rule for .badge sets a red color")
        if not any(bold.search(b) for _, b in hit):
            probs.append("no CSS rule for .badge sets bold weight")

    if task in ("T9", "T12") and browser is None:
        probs.append(f"{task} can only be judged in the browser (judge/browser.js); no browser result given")

    if task == "T10" and baseline is not None:
        base_pages = load_site(baseline)
        for url in sorted(set(base_pages) | set(pages)):
            a = base_pages.get(url)
            b = pages.get(url)
            if not a or not b:
                probs.append(f"{url}: page set changed")
            elif a[1].visible_text() != b[1].visible_text() or a[1].titles != b[1].titles:
                probs.append(f"{url}: content changed, but nothing needed changing")
    return probs


# --------------------------------------------------------------------------- main

def judge(out, task, data, build_ok, baseline=None, integrity=(), browser=None):
    out = Path(out)
    stubs = []
    pages = load_site(out, stubs) if out.is_dir() else {}
    stated, notes = check_stated(out, pages, data, build_ok, integrity)
    notes += [f"{u}: redirect stub, not counted as a page" for u in stubs]
    tprobs = check_task(task, out, pages, data, Path(baseline) if baseline else None, browser) if pages else ["no pages"]
    stated_v = {k: {"ok": not v, "problems": v} for k, v in stated.items()}
    ok = all(v["ok"] for v in stated_v.values()) and not tprobs
    return {"pass": ok, "stated": stated_v, "task": {"ok": not tprobs, "problems": tprobs},
            "unstated": {"notes": notes}, "pages": sorted(pages)}


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--out", required=True)
    ap.add_argument("--task", required=True)
    ap.add_argument("--data", required=True)
    ap.add_argument("--build-ok", default="yes", choices=("yes", "no"))
    ap.add_argument("--baseline")
    a = ap.parse_args()
    data = json.loads(Path(a.data).read_text())
    v = judge(a.out, a.task, data, a.build_ok == "yes", a.baseline)
    json.dump(v, sys.stdout, indent=1, ensure_ascii=False)
    print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
