import Head from 'next/head';
import { getData } from '../../lib/data';
import { BASE_URL, esc } from '../../lib/html';

// Ship zero client JS: no script tags, no __NEXT_DATA__ (pages router only).
export const config = { unstable_runtimeJS: false };

// DATA-DRIVEN routes: one page per stock in data.json, uppercase tickers kept.
export async function getStaticPaths() {
  return {
    paths: getData().stocks.map((s) => ({ params: { ticker: s.ticker } })),
    fallback: false,
  };
}

export async function getStaticProps({ params }) {
  const stock = getData().stocks.find((s) => s.ticker === params.ticker);
  return { props: { stock } };
}

function upcomingHtml(u) {
  if (!u) return '<p>No upcoming dividend declared.</p>\n';
  return (
    `<section class="upcoming${u.confirmed ? '' : ' estimated'}">\n` +
    `  <p>Next dividend: ${esc(u.amount)} ${esc(u.currency)} — ex ${esc(u.ex_date)}</p>\n` +
    `  ${u.confirmed ? '' : '<p class="badge">Estimated — not yet declared</p>'}\n` +
    `</section>\n`
  );
}

export default function Stock({ stock }) {
  const title = `${stock.name} (${stock.ticker})`;
  const url = `${BASE_URL}/stocks/${stock.ticker}/`;
  const body =
    `\n<h1>${esc(stock.name)} <small>${esc(stock.ticker)}</small></h1>\n` +
    `<p>${esc(stock.sector)}</p>\n\n` +
    upcomingHtml(stock.upcoming) +
    `\n<p><a href="/">← all stocks</a></p>\n`;
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
