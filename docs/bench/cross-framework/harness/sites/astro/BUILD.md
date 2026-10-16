# Astro benchmark port — byte-parity against ../../golden (the vendored harness target)

Astro 7.3.8 port of the 4-stock demo site. Reaches `TOTAL raw=0 normalized=0`
against the golden tree (verified with `../../../compare.py`).

## Install & build

```sh
cd site
npm install          # astro pinned exactly at 7.3.8 (package-lock.json vendored)
rm -rf dist          # always clean-build before comparing
npx astro build      # output in dist/
python3 ../../../compare.py dist ../../golden
```

Node 22.x used for the reference run. No global installs, no dev server needed.

## Layout

- `astro.config.mjs` — site/base config
- `src/data/data.json` — vendored copy of `../../data.json (the vendored harness dataset)`
  (the site is data-driven; pages and sitemap derive from `stocks[]`)
- `src/pages/index.astro` — stock list
- `src/pages/stocks/[ticker].astro` — one page per stock via `getStaticPaths`
- `src/pages/sitemap.xml.js` — hand-authored sitemap endpoint
- `public/style.css`, `public/robots.txt` — copied through verbatim

## Every trick needed for byte parity

1. **`compressHTML: false`** (astro.config.mjs). Astro's default minifier
   collapses the newlines/blank lines the golden files carry. This is
   non-negotiable; nothing else restores them.

2. **`build.format: 'directory'` + `trailingSlash: 'always'`** — pretty URLs
   (`/stocks/KO/index.html`) with the uppercase ticker preserved in the path
   (route param values keep their case).

3. **`set:html` string-building for repeated/conditional blocks.** Astro's
   JSX-ish expression loops (`{stocks.map(s => <li>…</li>)}`) trim/collapse the
   untrimmed whitespace between iterations, so the golden blank line after
   `<ul>`, between every `<li>`, and before `</ul>` cannot be produced by a
   component-model loop. Instead the `<li>` list is built as one joined string
   (`\n\n  <li>…</li>` per stock, plus a trailing `\n\n`) and injected with
   `<ul set:html={items}></ul>`.

   Same trick for the per-stock `<section class="upcoming">` block: when the
   dividend is confirmed the golden file has a line containing exactly two
   spaces (`"  "`) where the "Estimated" badge would sit — an Astro conditional
   would swallow it. The whole section (or the `<p>No upcoming dividend
   declared.</p>` fallback) is a template-literal string rendered via
   `<Fragment set:html={upcoming} />`.

4. **`@astrojs/sitemap` REJECTED.** The integration emits an
   `xmlns:xhtml` namespace on `<urlset>` and applies its own URL
   ordering/serialization, which can never match the golden `sitemap.xml`
   bytes. `src/pages/sitemap.xml.js` is a plain static endpoint that emits the
   exact `<?xml …?>` header, plain `xmlns`, two-space-indented
   `<url><loc>…</loc></url>` lines in data order, and a trailing newline.

5. **Manual `&` escaping only inside `set:html` strings** (`AT&T` →
   `AT&amp;T`): `set:html` bypasses Astro's escaper, so the list items escape
   `&` themselves. Everywhere else (`<title>{title}</title>`, `content={title}`,
   `{stock.name}`) Astro's normal expression/attribute escaping already
   produces `AT&amp;T Inc` — do not pre-escape there or you get `&amp;amp;`.

6. **Author the `<!DOCTYPE html>` line yourself** in each `.astro` page so the
   doctype/`<html>` newline placement is under your control. Golden HTML files
   have **no trailing newline** after `</html>`; Astro's natural output
   matches, so don't append one.

7. **`robots.txt` is static** (not data-dependent) and is shipped verbatim from
   `public/`; `style.css` likewise.
