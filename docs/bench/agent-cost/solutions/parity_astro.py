"""Reference solutions for the parity Astro port (judge-side only; see molde.py).

Style: file-based routes with getStaticPaths, paginate() for the list, and the port's own
sitemap endpoint extended. T3 moves the page file; its new content is the port's own file
with the address edit applied (read from the vendored port, so nothing is copied by hand).
T4/T4b: inverse of the plants in runner/setup.py (judge-side).
"""

import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from _idiomatic import undo  # noqa: E402
from pathlib import Path

EXPECTED_FAIL = {"T10-overedit": "negative control: changes visible content when nothing needed changing"}

_PORT = Path(__file__).resolve().parents[2] / "cross-framework/harness/sites/astro"


def _moved_stock_page():
    s = (_PORT / "src/pages/stocks/[ticker].astro").read_text()
    assert s.count("/stocks/${stock.ticker}/") == 1
    return s.replace("/stocks/${stock.ticker}/", "/acciones/${stock.ticker}/")


SECTOR_PAGE = """---
import data from '../../data/data.json';

export function getStaticPaths() {
  const slugOf = (name) => name.toLowerCase().replaceAll(' ', '-');
  const names = [...new Set(data.stocks.map((s) => s.sector))];
  return names.map((name) => ({
    params: { sector: slugOf(name) },
    props: { name, stocks: data.stocks.filter((s) => s.sector === name) },
  }));
}

const { name, stocks } = Astro.props;
const title = `${name} stocks`;
const url = `https://demo.example/sectors/${name.toLowerCase().replaceAll(' ', '-')}/`;
---
<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8">
<link rel="stylesheet" href="/style.css">
<title>{title}</title>
<link rel="canonical" href={url}>
<meta property="og:title" content={title}>
<meta property="og:url" content={url}>
<meta property="og:type" content="website">
</head>
<body>
<h1>{name}</h1>
<ul>
  {stocks.map((s) => <li><a href={`/stocks/${s.ticker}/`}>{s.name}</a></li>)}
</ul>
<p><a href="/">← all stocks</a></p>
</body>
</html>
"""

HOME_PAGINATED = """---
import data from '../data/data.json';

// Page 1 is "/" (no slug); page n > 1 is "/page/n/". Two stocks per page, in data order.
export function getStaticPaths() {
  const PER_PAGE = 2;
  const count = Math.max(Math.ceil(data.stocks.length / PER_PAGE), 1);
  return Array.from({ length: count }, (_, i) => ({
    params: { slug: i === 0 ? undefined : `page/${i + 1}` },
    props: { n: i + 1, count, stocks: data.stocks.slice(i * PER_PAGE, (i + 1) * PER_PAGE) },
  }));
}

const { n, count, stocks } = Astro.props;
const esc = (s) => s.replace(/&/g, '&amp;');
const path = n === 1 ? '/' : `/page/${n}/`;
const prev = n === 1 ? null : n === 2 ? '/' : `/page/${n - 1}/`;
const next = n < count ? `/page/${n + 1}/` : null;
const title = n > 1 ? `Demo — dividend stocks (page ${n})` : 'Demo — dividend stocks';
const url = `https://demo.example${path}`;
const items =
  stocks
    .map((s) => `\\n\\n  <li><a href="/stocks/${s.ticker}/">${esc(s.name)}</a> — ${esc(s.sector)}</li>`)
    .join('') + '\\n\\n';
---
<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8">
<link rel="stylesheet" href="/style.css">
<title>{title}</title>
<link rel="canonical" href={url}>
<meta property="og:title" content={title}>
<meta property="og:url" content={url}>
<meta property="og:type" content="website">
</head>
<body>
<h1>Stocks (cycle {data.cycle_id})</h1>
<ul set:html={items}></ul>
{prev && <a href={prev}>previous</a>}
{next && <a href={next}>next</a>}
</body>
</html>
"""

SITEMAP_PATHS = "const paths = ['/', ...data.stocks.map((s) => `/stocks/${s.ticker}/`)];"

SOLUTIONS = {
    "T1": [
        ("src/pages/stocks/[ticker].astro", "<p>{stock.sector}</p>",
         "<p>{stock.sector}</p>\n<p>Dividend yield: {stock.dividend_yield}</p>"),
        ("src/pages/index.astro", "— ${esc(s.sector)}</li>", "— ${esc(s.sector)} — yield ${esc(s.dividend_yield)}</li>"),
    ],
    "T2": [
        ("src/pages/sectors/[sector].astro", None, SECTOR_PAGE),
        ("src/pages/stocks/[ticker].astro", "<p>{stock.sector}</p>",
         "<p><a href={`/sectors/${stock.sector.toLowerCase().replaceAll(' ', '-')}/`}>{stock.sector}</a></p>"),
        ("src/pages/sitemap.xml.js", SITEMAP_PATHS,
         "const sectors = [...new Set(data.stocks.map((s) => s.sector))];\n"
         "  const paths = [\n    '/',\n    ...data.stocks.map((s) => `/stocks/${s.ticker}/`),\n"
         "    ...sectors.map((n) => `/sectors/${n.toLowerCase().replaceAll(' ', '-')}/`),\n  ];"),
    ],
    "T3": [
        ("src/pages/acciones/[ticker].astro", None, _moved_stock_page()),
        ("src/pages/stocks/[ticker].astro", "", "@DELETE"),
        ("src/pages/index.astro", 'href="/stocks/${s.ticker}/"', 'href="/acciones/${s.ticker}/"'),
        ("src/pages/sitemap.xml.js", "`/stocks/${s.ticker}/`", "`/acciones/${s.ticker}/`"),
    ],
    "T5": [
        ("src/pages/index.astro", '<meta property="og:type" content="website">',
         '<meta property="og:type" content="website">\n<meta property="og:description" content="Upcoming dividends for every stock we track.">'),
        ("src/pages/stocks/[ticker].astro", '<meta property="og:type" content="website">',
         '<meta property="og:type" content="website">\n<meta property="og:description" content={`${stock.name} (${stock.ticker}): ${stock.sector}.`}>'),
    ],
    "T6": [
        ("src/pages/[...slug].astro", None, HOME_PAGINATED),
        ("src/pages/index.astro", "", "@DELETE"),
        ("src/pages/sitemap.xml.js", SITEMAP_PATHS,
         "const pageCount = Math.ceil(data.stocks.length / 2);\n"
         "  const paths = [\n    '/',\n    ...Array.from({ length: Math.max(pageCount - 1, 0) }, (_, i) => `/page/${i + 2}/`),\n"
         "    ...data.stocks.map((s) => `/stocks/${s.ticker}/`),\n  ];"),
    ],
    "T11": [
        ("src/pages/index.astro", "  data.stocks\n    .map(",
         "  [...data.stocks]\n    .sort((a, b) => a.name.localeCompare(b.name, 'en', { sensitivity: 'base' }))\n    .map("),
    ],
    "T12": [
        ("src/pages/index.astro", "const items =",
         "const sectors = [...new Set(data.stocks.map((s) => s.sector))];\n"
         "const buttons = sectors.map((n) => `<button type=\"button\" data-sector=\"${esc(n)}\">${esc(n)}</button>`).join(' ');\n"
         "const script = `document.querySelectorAll('button[data-sector]').forEach((b) => b.addEventListener('click', () => document.querySelectorAll('li[data-sector]').forEach((li) => { li.hidden = li.dataset.sector !== b.dataset.sector; })));`;\nconst items ="),
        ("src/pages/index.astro", '<li><a href="/stocks/${s.ticker}/">', '<li data-sector="${esc(s.sector)}"><a href="/stocks/${s.ticker}/">'),
        ("src/pages/index.astro", "<ul set:html={items}></ul>",
         "<p set:html={buttons}></p>\n<ul set:html={items}></ul>\n<script is:inline set:html={script}></script>"),
    ],
    "T4": undo("parity-astro", "T4"),
    "T4b": undo("parity-astro", "T4b"),
    "T10": [],
    "T10-overedit": [("src/pages/stocks/[ticker].astro", "<p>{stock.sector}</p>", "<p>Sector: {stock.sector}</p>")],
}
