// Hand-authored sitemap endpoint. @astrojs/sitemap was rejected: it emits an
// xhtml namespace attribute and applies its own URL ordering/formatting, which
// can never match the golden bytes. This endpoint emits them exactly.
import data from '../data/data.json';

const SITE = 'https://demo.example';

export function GET() {
  const paths = ['/', ...data.stocks.map((s) => `/stocks/${s.ticker}/`)];
  const body =
    '<?xml version="1.0" encoding="UTF-8"?>\n' +
    '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' +
    paths.map((p) => `  <url><loc>${SITE}${p}</loc></url>`).join('\n') +
    '\n</urlset>\n';
  return new Response(body, { headers: { 'Content-Type': 'application/xml' } });
}
