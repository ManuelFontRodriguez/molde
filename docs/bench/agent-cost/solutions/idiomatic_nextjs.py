"""Reference solutions for the idiomatic Next.js port (judge-side only; see molde.py)."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from _idiomatic import source, undo  # noqa: E402

EXPECTED_FAIL = {"T10-overedit": "negative control: changes visible content when nothing needed changing"}
PAGE = "app/stocks/[ticker]/page.tsx"

HOME = "app/page.tsx"
LIB = "lib/stocks.ts"
SITEMAP = "app/sitemap.ts"
LAYOUT = "app/layout.tsx"

T0_FILES = ["package.json", "tsconfig.json", "next.config.ts", LAYOUT, HOME, PAGE, "app/robots.ts", SITEMAP,
            "lib/site.ts", LIB]
# T0: the frozen port is a valid answer; the data stays at the workspace's data.json.
T0 = [(rel, None, source("nextjs", rel)) for rel in T0_FILES if rel != LIB]
T0 += [(LIB, None, source("nextjs", LIB).replace("@/data/stocks.json", "@/data.json")),
       ("public/style.css", None, "@MOVE style.css")]

SECTOR_PAGE = """import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import { getSectors, getStocksInSector, sectorPath, stockPath } from "@/lib/stocks";

type Props = {
  params: Promise<{ slug: string }>;
};

export const dynamicParams = false;

export function generateStaticParams() {
  return getSectors().map((sector) => ({ slug: sector.slug }));
}

async function loadSector(params: Props["params"]) {
  const { slug } = await params;
  const sector = getSectors().find((s) => s.slug === slug);
  if (!sector) notFound();
  return sector;
}

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const sector = await loadSector(params);
  const title = `${sector.name} stocks`;
  const url = sectorPath(sector.name);

  return {
    title,
    alternates: { canonical: url },
    openGraph: { title, url, type: "website" },
  };
}

export default async function SectorPage({ params }: Props) {
  const sector = await loadSector(params);

  return (
    <main>
      <h1>{sector.name}</h1>
      <ul>
        {getStocksInSector(sector.name).map((stock) => (
          <li key={stock.ticker}>
            <Link href={stockPath(stock.ticker)}>{stock.name}</Link>
          </li>
        ))}
      </ul>
      <p>
        <Link href="/">Back to all stocks</Link>
      </p>
    </main>
  );
}
"""

STOCK_LIST = """import Link from "next/link";
import { stockPath, type Stock } from "@/lib/stocks";

export function StockList({ stocks }: { stocks: Stock[] }) {
  return (
    <ul>
      {stocks.map((stock) => (
        <li key={stock.ticker}>
          <Link href={stockPath(stock.ticker)}>{stock.name}</Link>
          {" — "}
          {stock.sector}
        </li>
      ))}
    </ul>
  );
}
"""

PAGER = """import Link from "next/link";

export function Pager({ page, pageCount }: { page: number; pageCount: number }) {
  const href = (n: number) => (n === 1 ? "/" : `/page/${n}/`);
  return (
    <nav>
      {page > 1 && <Link href={href(page - 1)}>Previous page</Link>}
      {page > 1 && page < pageCount && " | "}
      {page < pageCount && <Link href={href(page + 1)}>Next page</Link>}
    </nav>
  );
}
"""

LIST_PAGE = """import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { Pager } from "@/components/Pager";
import { StockList } from "@/components/StockList";
import { cycleId, getPage, pageCount } from "@/lib/stocks";

type Props = {
  params: Promise<{ page: string }>;
};

export const dynamicParams = false;

export function generateStaticParams() {
  return Array.from({ length: pageCount() - 1 }, (_, i) => ({ page: String(i + 2) }));
}

async function loadPage(params: Props["params"]) {
  const { page } = await params;
  const n = Number(page);
  if (!Number.isInteger(n) || n < 2 || n > pageCount()) notFound();
  return n;
}

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const n = await loadPage(params);
  const title = `Demo — dividend stocks (page ${n})`;
  const url = `/page/${n}/`;

  return {
    title,
    alternates: { canonical: url },
    openGraph: { title, url, type: "website" },
  };
}

export default async function ListPage({ params }: Props) {
  const n = await loadPage(params);

  return (
    <main>
      <h1>Stocks (cycle {cycleId})</h1>
      <StockList stocks={getPage(n)} />
      <Pager page={n} pageCount={pageCount()} />
    </main>
  );
}
"""

SEARCH_LIST = """"use client";

import { useState } from "react";
import Link from "next/link";
import type { Stock } from "@/lib/stocks";

type Item = Pick<Stock, "ticker" | "name" | "sector"> & { href: string };

export function SearchableStockList({ items }: { items: Item[] }) {
  const [query, setQuery] = useState("");
  const q = query.trim().toLowerCase();
  const visible = items.filter(
    (item) => item.name.toLowerCase().includes(q) || item.ticker.toLowerCase().includes(q),
  );

  return (
    <>
      <input
        type="search"
        aria-label="Search stocks"
        placeholder="Search by name or ticker"
        value={query}
        onChange={(event) => setQuery(event.target.value)}
      />
      <ul>
        {visible.map((item) => (
          <li key={item.ticker}>
            <Link href={item.href}>{item.name}</Link>
            {" — "}
            {item.sector}
          </li>
        ))}
      </ul>
    </>
  );
}
"""

SECTOR_FILTER = """"use client";

import { useState } from "react";
import Link from "next/link";
import type { Stock } from "@/lib/stocks";

type Item = Pick<Stock, "ticker" | "name" | "sector"> & { href: string };

export function SectorFilteredStockList({ items }: { items: Item[] }) {
  const sectors = [...new Set(items.map((item) => item.sector))];
  const [selected, setSelected] = useState<string | null>(null);
  const visible = selected === null ? items : items.filter((item) => item.sector === selected);

  return (
    <>
      <div>
        <button type="button" aria-pressed={selected === null} onClick={() => setSelected(null)}>
          All
        </button>
        {sectors.map((sector) => (
          <button
            key={sector}
            type="button"
            aria-pressed={selected === sector}
            onClick={() => setSelected(sector)}
          >
            {sector}
          </button>
        ))}
      </div>
      <ul>
        {visible.map((item) => (
          <li key={item.ticker}>
            <Link href={item.href}>{item.name}</Link>
            {" — "}
            {item.sector}
          </li>
        ))}
      </ul>
    </>
  );
}
"""

SOLUTIONS = {
    "T0": T0,
    # The data type gains the field; the page and the list show it.
    "T1": [
        (LIB, "  sector: string;\n", "  sector: string;\n  dividend_yield: string;\n"),
        (PAGE, "      <p>Sector: {stock.sector}</p>\n", "      <p>Sector: {stock.sector}</p>\n      <p>Dividend yield: {stock.dividend_yield}</p>\n"),
        (HOME, "            {stock.sector}\n", "            {stock.sector}\n            {\" — yield \"}\n            {stock.dividend_yield}\n"),
    ],
    # A [slug] route with generateStaticParams, a slug helper beside stockPath, a sitemap entry.
    "T2": [
        ("app/sectors/[slug]/page.tsx", None, SECTOR_PAGE),
        (LIB, "export function stockPath(",
         """export function sectorSlug(sector: string): string {
  return sector.toLowerCase().replaceAll(" ", "-");
}

export function sectorPath(sector: string): string {
  return `/sectors/${encodeURIComponent(sectorSlug(sector))}/`;
}

export function getSectors(): { name: string; slug: string }[] {
  const names = [...new Set(stockData.stocks.map((stock) => stock.sector))];
  return names.map((name) => ({ name, slug: sectorSlug(name) }));
}

export function getStocksInSector(sector: string): Stock[] {
  return stockData.stocks.filter((stock) => stock.sector === sector);
}

export function stockPath("""),
        (PAGE, 'import { getStock, getStocks, stockPath } from "@/lib/stocks";',
         'import { getStock, getStocks, sectorPath, stockPath } from "@/lib/stocks";'),
        (PAGE, "      <p>Sector: {stock.sector}</p>\n",
         "      <p>\n        Sector: <Link href={sectorPath(stock.sector)}>{stock.sector}</Link>\n      </p>\n"),
        (SITEMAP, 'import { getStocks, stockPath } from "@/lib/stocks";', 'import { getSectors, getStocks, sectorPath, stockPath } from "@/lib/stocks";'),
        (SITEMAP, "    ...getStocks().map(", "    ...getSectors().map((sector) => ({ url: `${siteUrl}${sectorPath(sector.name)}` })),\n    ...getStocks().map("),
    ],
    "T3": [("app/acciones/[ticker]/page.tsx", None, source("nextjs", PAGE)), (PAGE, None, "@DELETE"),
           ("lib/stocks.ts", "`/stocks/${encodeURIComponent(ticker)}/`", "`/acciones/${encodeURIComponent(ticker)}/`")],
    "T4": undo("idiomatic-nextjs", "T4", special={3: [("app/sitemap.ts", "  return [\n", "  return [\n    { url: `${siteUrl}/` },\n")]}),
    "T4b": undo("idiomatic-nextjs", "T4b"),
    # openGraph.description in the Metadata API of the home page and of generateMetadata.
    "T5": [
        (HOME, "  openGraph: { title, url: \"/\", type: \"website\" },",
         "  openGraph: {\n    title,\n    url: \"/\",\n    type: \"website\",\n    description: \"Upcoming dividends for every stock we track.\",\n  },"),
        (PAGE, "    openGraph: { title, url, type: \"website\" },",
         "    openGraph: {\n      title,\n      url,\n      type: \"website\",\n      description: `${stock.name} (${stock.ticker}): ${stock.sector}.`,\n    },"),
    ],
    # Page size and slicing live in lib/stocks.ts; / is page 1, /page/[page]/ is the rest.
    "T6": [
        ("components/StockList.tsx", None, STOCK_LIST),
        ("components/Pager.tsx", None, PAGER),
        ("app/page/[page]/page.tsx", None, LIST_PAGE),
        (LIB, "export function getStock(",
         """export const PAGE_SIZE = 2;

export function pageCount(): number {
  return Math.max(1, Math.ceil(stockData.stocks.length / PAGE_SIZE));
}

export function getPage(page: number): Stock[] {
  return stockData.stocks.slice((page - 1) * PAGE_SIZE, page * PAGE_SIZE);
}

export function getStock("""),
        (HOME, 'import { cycleId, getStocks, stockPath } from "@/lib/stocks";',
         'import { Pager } from "@/components/Pager";\nimport { StockList } from "@/components/StockList";\nimport { cycleId, getPage, pageCount } from "@/lib/stocks";'),
        (HOME, 'import Link from "next/link";\n', ""),
        (HOME, """      <ul>
        {getStocks().map((stock) => (
          <li key={stock.ticker}>
            <Link href={stockPath(stock.ticker)}>{stock.name}</Link>
            {" — "}
            {stock.sector}
          </li>
        ))}
      </ul>
""", "      <StockList stocks={getPage(1)} />\n      <Pager page={1} pageCount={pageCount()} />\n"),
        (SITEMAP, 'import { getStocks, stockPath } from "@/lib/stocks";', 'import { getStocks, pageCount, stockPath } from "@/lib/stocks";'),
        (SITEMAP, "    ...getStocks().map(",
         "    ...Array.from({ length: pageCount() - 1 }, (_, i) => ({ url: `${siteUrl}/page/${i + 2}/` })),\n    ...getStocks().map("),
    ],
    # A JSON-LD script rendered with the page; "<" is escaped so data cannot close the tag.
    "T7": [
        (PAGE, "  return (\n    <main>\n",
         """  const jsonLd = {
    "@context": "https://schema.org",
    "@type": "Corporation",
    name: stock.name,
    tickerSymbol: stock.ticker,
  };

  return (
    <main>
      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd).replace(/</g, "\\\\u003c") }}
      />
"""),
    ],
    # A global stylesheet imported in the root layout (the class name must stay `badge`).
    "T8": [
        ("app/globals.css", None, ".badge {\n  color: #c00000;\n  font-weight: 700;\n}\n"),
        (LAYOUT, 'import { siteUrl } from "@/lib/site";', 'import { siteUrl } from "@/lib/site";\nimport "./globals.css";'),
    ],
    # The server page passes plain data to a client component that owns the query state.
    "T9": [
        ("components/SearchableStockList.tsx", None, SEARCH_LIST),
        (HOME, 'import Link from "next/link";\nimport { cycleId, getStocks, stockPath } from "@/lib/stocks";',
         'import { SearchableStockList } from "@/components/SearchableStockList";\nimport { cycleId, getStocks, stockPath } from "@/lib/stocks";'),
        (HOME, """      <ul>
        {getStocks().map((stock) => (
          <li key={stock.ticker}>
            <Link href={stockPath(stock.ticker)}>{stock.name}</Link>
            {" — "}
            {stock.sector}
          </li>
        ))}
      </ul>
""", """      <SearchableStockList
        items={getStocks().map(({ ticker, name, sector }) => ({
          ticker,
          name,
          sector,
          href: stockPath(ticker),
        }))}
      />
"""),
    ],
    "T11": [
        (HOME, "        {getStocks().map((stock) => (",
         "        {getStocks()\n          .toSorted((a, b) => a.name.localeCompare(b.name, \"en\", { sensitivity: \"base\" }))\n          .map((stock) => ("),
    ],
    "T12": [
        ("components/SectorFilteredStockList.tsx", None, SECTOR_FILTER),
        (HOME, 'import Link from "next/link";\nimport { cycleId, getStocks, stockPath } from "@/lib/stocks";',
         'import { SectorFilteredStockList } from "@/components/SectorFilteredStockList";\nimport { cycleId, getStocks, stockPath } from "@/lib/stocks";'),
        (HOME, """      <ul>
        {getStocks().map((stock) => (
          <li key={stock.ticker}>
            <Link href={stockPath(stock.ticker)}>{stock.name}</Link>
            {" — "}
            {stock.sector}
          </li>
        ))}
      </ul>
""", """      <SectorFilteredStockList
        items={getStocks().map(({ ticker, name, sector }) => ({
          ticker,
          name,
          sector,
          href: stockPath(ticker),
        }))}
      />
"""),
    ],
    "T10": [],
    "T10-overedit": [(PAGE, "<p>Sector: {stock.sector}</p>", "<p>Industry: {stock.sector}</p>")],
}
