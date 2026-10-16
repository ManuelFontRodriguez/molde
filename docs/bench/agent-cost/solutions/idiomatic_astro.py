"""Reference solutions for the idiomatic Astro port (judge-side only; see molde.py)."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from _idiomatic import source, undo  # noqa: E402

EXPECTED_FAIL = {"T10-overedit": "negative control: changes visible content when nothing needed changing"}
PAGE = "src/pages/stocks/[ticker].astro"

LAYOUT = "src/layouts/BaseLayout.astro"
STOCKS_TS = "src/data/stocks.ts"
SITEMAP = "src/pages/sitemap.xml.ts"
INDEX = "src/pages/index.astro"

# T0: the frozen port, except that the data stays in the workspace's data.json (imported
# from the project root) and the stylesheet moves to public/.
T0_FILES = ["package.json", "astro.config.mjs", "tsconfig.json", LAYOUT, STOCKS_TS, SITEMAP, INDEX,
            "src/pages/robots.txt.ts", PAGE]
T0 = [(rel, None, source("astro", rel).replace("from './stocks.json'", "from '../../data.json'")) for rel in T0_FILES]
T0.append(("public/style.css", None, "@MOVE style.css"))

SECTOR_HELPERS = """
export const sectorSlug = (sector: string) => sector.toLowerCase().replace(/ /g, '-');
export const sectorPath = (sector: string) => `/sectors/${sectorSlug(sector)}/`;
export const sectors: string[] = [...new Set(stocks.map((stock) => stock.sector))];
"""

SECTOR_PAGE = """---
import type { GetStaticPaths } from 'astro';
import BaseLayout from '../../layouts/BaseLayout.astro';
import { stocks, stockPath, sectors, sectorSlug, type Stock } from '../../data/stocks';

export const getStaticPaths = (() =>
  sectors.map((sector) => ({
    params: { slug: sectorSlug(sector) },
    props: { sector, stocks: stocks.filter((stock) => stock.sector === sector) },
  }))) satisfies GetStaticPaths;

interface Props {
  sector: string;
  stocks: Stock[];
}

const { sector, stocks: members } = Astro.props;
---

<BaseLayout title={`${sector} stocks`} description={`${sector} stocks we track.`}>
  <h1>{sector}</h1>
  <ul>
    {members.map((stock) => <li><a href={stockPath(stock)}>{stock.name}</a></li>)}
  </ul>
  <p><a href="/">← All stocks</a></p>
</BaseLayout>
"""

# paginate() on a rest route; the params are remapped so that page 1 is `/` and the rest `/page/<n>/`
# (paginate's own scheme would be `/<n>/`, so its page.url.prev/next are not used).
PAGED_INDEX = """---
import type { GetStaticPaths } from 'astro';
import BaseLayout from '../layouts/BaseLayout.astro';
import { cycleId, stocks, stockPath, listPath, PAGE_SIZE } from '../data/stocks';

export const getStaticPaths = (({ paginate }) =>
  paginate(stocks, { pageSize: PAGE_SIZE }).map((route) => ({
    ...route,
    params: { page: route.props.page.currentPage === 1 ? undefined : `page/${route.props.page.currentPage}` },
  }))) satisfies GetStaticPaths;

const { page } = Astro.props;
const title = page.currentPage > 1 ? `Demo — dividend stocks (page ${page.currentPage})` : 'Demo — dividend stocks';
---

<BaseLayout title={title} description="Upcoming dividends for every stock we track.">
  <h1>Stocks (cycle {cycleId})</h1>
  <ul>
    {
      page.data.map((stock) => (
        <li>
          <a href={stockPath(stock)}>{stock.name}</a> — {stock.sector}
        </li>
      ))
    }
  </ul>
  <nav>
    {page.url.prev && <a href={listPath(page.currentPage - 1)} rel="prev">← Previous</a>}
    {page.url.next && <a href={listPath(page.currentPage + 1)} rel="next">Next →</a>}
  </nav>
</BaseLayout>
"""

SOLUTIONS = {
    "T0": T0,
    "T1": [
        (STOCKS_TS, "  sector: string;\n", "  sector: string;\n  dividend_yield: string;\n"),
        (PAGE, "  <p>Sector: {stock.sector}</p>\n", "  <p>Sector: {stock.sector}</p>\n  <p>Dividend yield: {stock.dividend_yield}</p>\n"),
        (INDEX, "<a href={stockPath(stock)}>{stock.name}</a> — {stock.sector}", "<a href={stockPath(stock)}>{stock.name}</a> — {stock.sector} — {stock.dividend_yield}"),
    ],
    "T2": [
        (STOCKS_TS, "export const stockPath = (stock: Stock) => `/stocks/${stock.ticker}/`;\n",
         "export const stockPath = (stock: Stock) => `/stocks/${stock.ticker}/`;\n" + SECTOR_HELPERS),
        ("src/pages/sectors/[slug].astro", None, SECTOR_PAGE),
        (PAGE, "import { stocks, type Stock }", "import { stocks, sectorPath, type Stock }"),
        (PAGE, "  <p>Sector: {stock.sector}</p>", "  <p>Sector: <a href={sectorPath(stock.sector)}>{stock.sector}</a></p>"),
        (SITEMAP, "import { stocks, stockPath }", "import { stocks, stockPath, sectors, sectorPath }"),
        (SITEMAP, "['/', ...stocks.map(stockPath)]", "['/', ...stocks.map(stockPath), ...sectors.map(sectorPath)]"),
    ],
    "T5": [
        (LAYOUT, "  title: string;\n", "  title: string;\n  description: string;\n"),
        (LAYOUT, "const { title } = Astro.props;", "const { title, description } = Astro.props;"),
        (LAYOUT, '    <meta property="og:url" content={canonicalURL} />\n',
         '    <meta property="og:url" content={canonicalURL} />\n    <meta property="og:description" content={description} />\n'),
        (INDEX, '<BaseLayout title="Demo — dividend stocks">',
         '<BaseLayout title="Demo — dividend stocks" description="Upcoming dividends for every stock we track.">'),
        (PAGE, "<BaseLayout title={`${stock.name} (${stock.ticker})`}>",
         "<BaseLayout\n  title={`${stock.name} (${stock.ticker})`}\n  description={`${stock.name} (${stock.ticker}): ${stock.sector}.`}\n>"),
    ],
    "T6": [
        (STOCKS_TS, "export const stockPath", "export const PAGE_SIZE = 2;\nexport const listPath = (n: number) => (n === 1 ? '/' : `/page/${n}/`);\n\nexport const stockPath"),
        ("src/pages/[...page].astro", None, PAGED_INDEX.replace("import { cycleId, stocks, stockPath, listPath, PAGE_SIZE }", "import { cycleId, stocks, stockPath, listPath, PAGE_SIZE }")),
        (INDEX, None, "@DELETE"),
        (SITEMAP, "import { stocks, stockPath }", "import { stocks, stockPath, PAGE_SIZE }"),
        (SITEMAP, "  const paths = ['/', ...stocks.map(stockPath)];",
         "  const pageCount = Math.ceil(stocks.length / PAGE_SIZE);\n"
         "  const listPages = Array.from({ length: pageCount - 1 }, (_, i) => `/page/${i + 2}/`);\n"
         "  const paths = ['/', ...listPages, ...stocks.map(stockPath)];"),
    ],
    "T7": [
        (PAGE, "  <h1>{stock.name}", "  <script type=\"application/ld+json\" set:html={JSON.stringify(jsonLd)} />\n  <h1>{stock.name}"),
        (PAGE, "const { upcoming } = stock;\n",
         "const { upcoming } = stock;\nconst jsonLd = {\n  '@context': 'https://schema.org',\n  '@type': 'Corporation',\n  name: stock.name,\n  tickerSymbol: stock.ticker,\n};\n"),
    ],
    "T8": [("public/style.css", "background: #fce8b2; }", "background: #fce8b2; color: red; font-weight: bold; }")],
    "T9": [
        (INDEX, "  <ul>\n", '  <input type="search" id="search" placeholder="Search by name or ticker" aria-label="Search stocks" />\n  <ul id="stock-list">\n'),
        (INDEX, "        <li>\n", "        <li data-search={`${stock.name} ${stock.ticker}`.toLowerCase()}>\n"),
        (INDEX, "</BaseLayout>", """</BaseLayout>

<script>
  const input = document.querySelector<HTMLInputElement>('#search')!;
  const items = document.querySelectorAll<HTMLLIElement>('#stock-list li');
  input.addEventListener('input', () => {
    const query = input.value.trim().toLowerCase();
    items.forEach((li) => {
      li.hidden = !(li.dataset.search ?? '').includes(query);
    });
  });
</script>"""),
    ],
    "T3": [("src/pages/acciones/[ticker].astro", None, source("astro", PAGE)), (PAGE, None, "@DELETE"),
           ("src/data/stocks.ts", "`/stocks/${stock.ticker}/`", "`/acciones/${stock.ticker}/`")],
    "T4": undo("idiomatic-astro", "T4"),
    "T4b": undo("idiomatic-astro", "T4b"),
    "T11": [
        (INDEX, "import { cycleId, stocks, stockPath } from '../data/stocks';\n",
         "import { cycleId, stocks, stockPath } from '../data/stocks';\n\n"
         "const sorted = [...stocks].sort((a, b) => a.name.localeCompare(b.name, 'en', { sensitivity: 'base' }));\n"),
        (INDEX, "      stocks.map((stock) => (", "      sorted.map((stock) => ("),
    ],
    "T12": [
        (INDEX, "import { cycleId, stocks, stockPath } from '../data/stocks';\n",
         "import { cycleId, stocks, stockPath } from '../data/stocks';\n\n"
         "const sectors = [...new Set(stocks.map((stock) => stock.sector))];\n"),
        (INDEX, "  <ul>\n", """  <div id="sector-filter">
    <button type="button" data-sector="">All</button>
    {sectors.map((sector) => <button type="button" data-sector={sector}>{sector}</button>)}
  </div>
  <ul id="stock-list">
"""),
        (INDEX, "        <li>\n", "        <li data-sector={stock.sector}>\n"),
        (INDEX, "</BaseLayout>", """</BaseLayout>

<script>
  const items = document.querySelectorAll<HTMLLIElement>('#stock-list li');
  document.querySelectorAll<HTMLButtonElement>('#sector-filter button').forEach((button) => {
    button.addEventListener('click', () => {
      const sector = button.dataset.sector;
      items.forEach((li) => {
        li.hidden = !!sector && li.dataset.sector !== sector;
      });
    });
  });
</script>"""),
    ],
    "T10": [],
    "T10-overedit": [(PAGE, "<p>Sector: {stock.sector}</p>", "<p>Industry: {stock.sector}</p>")],
}
