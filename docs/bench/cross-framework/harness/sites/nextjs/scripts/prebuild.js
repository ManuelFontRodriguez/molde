#!/usr/bin/env node
// PRE-BUILD step. Next's App Router sitemap convention (app/sitemap.js) emits a
// different XML shape (xhtml ns / changefreq handling / self-closing style) than
// the golden file, and the Pages Router has no sitemap convention at all, so the
// sitemap is generated manually before `next build` and served from public/.
// Also stages robots.txt and the shared style.css into public/.
const fs = require('fs');
const path = require('path');
const { DEMO_SITE_DIR, getData } = require('../lib/data');
const BASE_URL = 'https://demo.example';

const pub = path.join(__dirname, '..', 'public');
fs.mkdirSync(pub, { recursive: true });

const data = getData();
const locs = ['/', ...data.stocks.map((s) => `/stocks/${s.ticker}/`)];
const sitemap =
  '<?xml version="1.0" encoding="UTF-8"?>\n' +
  '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' +
  locs.map((l) => `  <url><loc>${BASE_URL}${l}</loc></url>`).join('\n') +
  '\n</urlset>\n';
fs.writeFileSync(path.join(pub, 'sitemap.xml'), sitemap);

fs.writeFileSync(
  path.join(pub, 'robots.txt'),
  `User-agent: *\nAllow: /\n\nSitemap: ${BASE_URL}/sitemap.xml\n`
);

fs.copyFileSync(path.join(DEMO_SITE_DIR, 'static', 'style.css'), path.join(pub, 'style.css'));

console.log(`prebuild: sitemap.xml (${locs.length} urls), robots.txt, style.css -> public/`);
