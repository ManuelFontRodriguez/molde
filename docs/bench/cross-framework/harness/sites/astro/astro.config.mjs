import { defineConfig } from 'astro/config';

export default defineConfig({
  site: 'https://demo.example',
  trailingSlash: 'always',
  compressHTML: false,
  build: {
    format: 'directory',
  },
});
