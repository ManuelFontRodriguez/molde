import type { APIRoute } from 'astro';
import { stocks, stockPath } from '../data/stocks';

const escapeXml = (s: string) =>
  s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

export const GET: APIRoute = ({ site }) => {
  const paths = ['/', ...stocks.map(stockPath)];
  const urls = paths
    .map((path) => `  <url><loc>${escapeXml(new URL(path, site).href)}</loc></url>`)
    .join('\n');

  const body = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${urls}
</urlset>
`;

  return new Response(body, {
    headers: { 'Content-Type': 'application/xml; charset=utf-8' },
  });
};
