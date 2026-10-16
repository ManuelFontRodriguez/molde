import Head from 'next/head';
import { getData } from '../lib/data';
import { BASE_URL, esc } from '../lib/html';

// Ship zero client JS: no script tags, no __NEXT_DATA__ (pages router only).
export const config = { unstable_runtimeJS: false };

export async function getStaticProps() {
  return { props: { data: getData() } };
}

export default function Index({ data }) {
  const title = 'Demo — dividend stocks';
  // Exact body markup (incl. blank lines) as a template string; JSX text nodes
  // cannot reproduce the golden file's newlines/indentation.
  const body =
    `\n<h1>Stocks (cycle ${esc(data.cycle_id)})</h1>\n<ul>\n` +
    data.stocks
      .map((s) => `\n  <li><a href="/stocks/${s.ticker}/">${esc(s.name)}</a> — ${esc(s.sector)}</li>\n`)
      .join('') +
    `\n</ul>\n`;
  return (
    <>
      <Head>
        <link rel="stylesheet" href="/style.css" />
        <title>{title}</title>
        <link rel="canonical" href={`${BASE_URL}/`} />
        <meta property="og:title" content={title} />
        <meta property="og:url" content={`${BASE_URL}/`} />
        <meta property="og:type" content="website" />
      </Head>
      <div dangerouslySetInnerHTML={{ __html: body }} />
    </>
  );
}
