"""Reference solutions for the parity Next.js port (judge-side only; see molde.py).

Style: Pages Router with getStaticPaths/getStaticProps and next/head; the port's prebuild
script (which writes the sitemap) is extended alongside. T3 moves the page file; its new
content is the port's own file with the address edit applied (read from the vendored port).
T4/T4b: inverse of the plants in runner/setup.py (judge-side).
"""

import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from _idiomatic import undo  # noqa: E402
from pathlib import Path

EXPECTED_FAIL = {"T10-overedit": "negative control: changes visible content when nothing needed changing"}

_PORT = Path(__file__).resolve().parents[2] / "cross-framework/harness/sites/nextjs"


def _moved_stock_page():
    s = (_PORT / "pages/stocks/[ticker].js").read_text()
    assert s.count("/stocks/${stock.ticker}/") == 1
    return s.replace("/stocks/${stock.ticker}/", "/acciones/${stock.ticker}/")


SECTOR_PAGE = """import Head from 'next/head';
import { getData } from '../../lib/data';
import { BASE_URL, esc, sectorSlug } from '../../lib/html';

export const config = { unstable_runtimeJS: false };

export async function getStaticPaths() {
  const names = [...new Set(getData().stocks.map((s) => s.sector))];
  return { paths: names.map((n) => ({ params: { sector: sectorSlug(n) } })), fallback: false };
}

export async function getStaticProps({ params }) {
  const stocks = getData().stocks.filter((s) => sectorSlug(s.sector) === params.sector);
  return { props: { name: stocks[0].sector, stocks } };
}

export default function Sector({ name, stocks }) {
  const title = `${name} stocks`;
  const url = `${BASE_URL}/sectors/${sectorSlug(name)}/`;
  const body =
    `\\n<h1>${esc(name)}</h1>\\n<ul>\\n` +
    stocks.map((s) => `  <li><a href="/stocks/${s.ticker}/">${esc(s.name)}</a></li>\\n`).join('') +
    `</ul>\\n<p><a href="/">← all stocks</a></p>\\n`;
  return (
    <>
      <Head>
        <link rel="stylesheet" href="/style.css" />
        <title>{title}</title>
        <link rel="canonical" href={url} />
        <meta property="og:title" content={title} />
        <meta property="og:url" content={url} />
        <meta property="og:type" content="website" />
      </Head>
      <div dangerouslySetInnerHTML={{ __html: body }} />
    </>
  );
}
"""

HOME_PAGINATED = """import Head from 'next/head';
import { getData } from '../lib/data';
import { BASE_URL, esc } from '../lib/html';

// Ship zero client JS: no script tags, no __NEXT_DATA__ (pages router only).
export const config = { unstable_runtimeJS: false };

const PER_PAGE = 2;

// Page 1 is "/" (empty slug); page n > 1 is "/page/n/".
export async function getStaticPaths() {
  const count = Math.max(Math.ceil(getData().stocks.length / PER_PAGE), 1);
  const paths = Array.from({ length: count }, (_, i) => ({ params: { slug: i === 0 ? [] : ['page', String(i + 1)] } }));
  return { paths, fallback: false };
}

export async function getStaticProps({ params }) {
  const data = getData();
  const n = params.slug && params.slug.length ? Number(params.slug[1]) : 1;
  const count = Math.max(Math.ceil(data.stocks.length / PER_PAGE), 1);
  return {
    props: { cycle: data.cycle_id, n, count, stocks: data.stocks.slice((n - 1) * PER_PAGE, n * PER_PAGE) },
  };
}

export default function Index({ cycle, n, count, stocks }) {
  const title = n > 1 ? `Demo — dividend stocks (page ${n})` : 'Demo — dividend stocks';
  const url = n === 1 ? `${BASE_URL}/` : `${BASE_URL}/page/${n}/`;
  const prev = n === 1 ? null : n === 2 ? '/' : `/page/${n - 1}/`;
  const next = n < count ? `/page/${n + 1}/` : null;
  // Exact body markup (incl. blank lines) as a template string; JSX text nodes
  // cannot reproduce the golden file's newlines/indentation.
  const body =
    `\\n<h1>Stocks (cycle ${esc(cycle)})</h1>\\n<ul>\\n` +
    stocks
      .map((s) => `\\n  <li><a href="/stocks/${s.ticker}/">${esc(s.name)}</a> — ${esc(s.sector)}</li>\\n`)
      .join('') +
    `\\n</ul>\\n` +
    (prev ? `<a href="${prev}">previous</a>\\n` : '') +
    (next ? `<a href="${next}">next</a>\\n` : '');
  return (
    <>
      <Head>
        <link rel="stylesheet" href="/style.css" />
        <title>{title}</title>
        <link rel="canonical" href={url} />
        <meta property="og:title" content={title} />
        <meta property="og:url" content={url} />
        <meta property="og:type" content="website" />
      </Head>
      <div dangerouslySetInnerHTML={{ __html: body }} />
    </>
  );
}
"""

LOCS = "const locs = ['/', ...data.stocks.map((s) => `/stocks/${s.ticker}/`)];"

SOLUTIONS = {
    "T1": [
        ("pages/stocks/[ticker].js", "`<p>${esc(stock.sector)}</p>\\n\\n` +",
         "`<p>${esc(stock.sector)}</p>\\n<p>Dividend yield: ${esc(stock.dividend_yield)}</p>\\n\\n` +"),
        ("pages/index.js", "— ${esc(s.sector)}</li>", "— ${esc(s.sector)} — yield ${esc(s.dividend_yield)}</li>"),
    ],
    "T2": [
        ("lib/html.js", "// Minimal HTML escaping",
         "// Sector page address: lowercase, spaces to hyphens.\n"
         "export const sectorSlug = (name) => name.toLowerCase().replace(/ /g, '-');\n\n// Minimal HTML escaping"),
        ("pages/sectors/[sector].js", None, SECTOR_PAGE),
        ("pages/stocks/[ticker].js", "import { BASE_URL, esc } from", "import { BASE_URL, esc, sectorSlug } from"),
        ("pages/stocks/[ticker].js", "`<p>${esc(stock.sector)}</p>\\n\\n` +",
         "`<p><a href=\"/sectors/${sectorSlug(stock.sector)}/\">${esc(stock.sector)}</a></p>\\n\\n` +"),
        ("scripts/prebuild.js", LOCS,
         "const sectorSlug = (n) => n.toLowerCase().replace(/ /g, '-');\n"
         "const sectors = [...new Set(data.stocks.map((s) => s.sector))];\n"
         "const locs = [\n  '/',\n  ...data.stocks.map((s) => `/stocks/${s.ticker}/`),\n  ...sectors.map((n) => `/sectors/${sectorSlug(n)}/`),\n];"),
    ],
    "T3": [
        ("pages/acciones/[ticker].js", None, _moved_stock_page()),
        ("pages/stocks/[ticker].js", "", "@DELETE"),
        ("pages/index.js", 'href="/stocks/${s.ticker}/"', 'href="/acciones/${s.ticker}/"'),
        ("scripts/prebuild.js", "`/stocks/${s.ticker}/`", "`/acciones/${s.ticker}/`"),
    ],
    "T5": [
        ("pages/index.js", '<meta property="og:type" content="website" />',
         '<meta property="og:type" content="website" />\n        <meta property="og:description" content="Upcoming dividends for every stock we track." />'),
        ("pages/stocks/[ticker].js", '<meta property="og:type" content="website" />',
         '<meta property="og:type" content="website" />\n'
         '        <meta property="og:description" content={`${stock.name} (${stock.ticker}): ${stock.sector}.`} />'),
    ],
    "T6": [
        ("pages/[[...slug]].js", None, HOME_PAGINATED),
        ("pages/index.js", "", "@DELETE"),
        ("scripts/prebuild.js", LOCS,
         "const pageCount = Math.ceil(data.stocks.length / 2);\n"
         "const locs = [\n  '/',\n  ...Array.from({ length: Math.max(pageCount - 1, 0) }, (_, i) => `/page/${i + 2}/`),\n"
         "  ...data.stocks.map((s) => `/stocks/${s.ticker}/`),\n];"),
    ],
    "T11": [
        ("pages/index.js", "    data.stocks\n      .map(",
         "    [...data.stocks]\n      .sort((a, b) => a.name.localeCompare(b.name, 'en', { sensitivity: 'base' }))\n      .map("),
    ],
    "T12": [
        ("pages/index.js", "  const body =",
         "  const sectors = [...new Set(data.stocks.map((s) => s.sector))];\n"
         "  const buttons = sectors.map((n) => `<button type=\"button\" data-sector=\"${esc(n)}\">${esc(n)}</button>`).join(' ');\n"
         "  const script = `document.querySelectorAll('button[data-sector]').forEach((b) => b.addEventListener('click', () => document.querySelectorAll('li[data-sector]').forEach((li) => { li.hidden = li.dataset.sector !== b.dataset.sector; })));`;\n  const body ="),
        ("pages/index.js", "<ul>\\n` +", "<p>${buttons}</p>\\n<ul>\\n` +"),
        ("pages/index.js", '<li><a href="/stocks/${s.ticker}/">', '<li data-sector="${esc(s.sector)}"><a href="/stocks/${s.ticker}/">'),
        ("pages/index.js", "`\\n</ul>\\n`;", "`\\n</ul>\\n<script>\\n${script}\\n</script>\\n`;"),
    ],
    "T4": undo("parity-nextjs", "T4"),
    "T4b": undo("parity-nextjs", "T4b"),
    "T10": [],
    "T10-overedit": [("pages/stocks/[ticker].js", "`<p>${esc(stock.sector)}</p>\\n\\n` +", "`<p>Sector: ${esc(stock.sector)}</p>\\n\\n` +")],
}
