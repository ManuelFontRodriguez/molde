// @ts-check
import { defineConfig } from 'astro/config';

// The sitemap is served by `src/pages/sitemap.xml.ts`: `@astrojs/sitemap` only
// emits `sitemap-index.xml` + chunks, while this site needs a single `/sitemap.xml`.
export default defineConfig({
  site: 'https://demo.example',
  trailingSlash: 'always',
  build: { format: 'directory' },
});
