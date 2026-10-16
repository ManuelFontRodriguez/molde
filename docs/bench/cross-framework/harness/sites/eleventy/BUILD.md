# Eleventy benchmark port — build instructions

Byte-parity rebuild of the golden target at `../../golden (the vendored harness target)`
(verified `cmp`-identical on all 8 files; `compare.py` TOTAL raw=0 normalized=0).

## Install

```sh
cd site
npm install   # installs @11ty/eleventy@3.1.6 (pinned in package.json/package-lock.json)
```

No global installs required.

## Build

```sh
rm -rf _site
npx @11ty/eleventy
```

Output lands in `_site/`. Verify with:

```sh
python3 ../../../compare.py _site ../../golden
```

Expected: `TOTAL raw=0 normalized=0 missing=[] extra_html=[]`.

## Layout

- `eleventy.config.js` — input dir `src`, passthrough copy of `src/style.css`
  to `/style.css`, plus two Eleventy 3 *virtual templates*
  (`eleventyConfig.addTemplate`) for `robots.txt` and `sitemap.xml`, keeping
  the authored surface to config + two page templates.
- `src/_data/demo.json` — verbatim copy of
  `../../data.json (the vendored harness dataset)` (global data `demo`).
- `src/index.njk` — index page, untrimmed Nunjucks `{% for %}` loop.
- `src/stocks.njk` — pagination over `demo.stocks`, `size: 1`, alias `s`,
  `permalink: "/stocks/{{ s.ticker }}/"` (uppercase tickers preserved —
  Eleventy does not lowercase permalinks).

## Quirks that matter for byte parity

- **Untrimmed Nunjucks semantics**: `trimBlocks`/`lstripBlocks` are off by
  default. The blank lines inside index's `<ul>`, the blank lines around the
  `<section>`/`No upcoming` block, and the confirmed-branch line containing
  exactly two spaces (`  ` where the badge `{% if %}` renders empty) all fall
  out of this. Do not add `{%-` trim markers.
- **No trailing newline**: golden HTML files end at `</html>` with no final
  newline, so `src/index.njk` and `src/stocks.njk` must not end with a
  newline. `robots.txt` and `sitemap.xml` *do* end with a newline (encoded in
  the virtual-template strings).
- **HTML escaping**: Eleventy's Nunjucks autoescape (on by default) renders
  `AT&T Inc` as `AT&amp;T Inc` in title/OG/h1/link text — no manual
  `| escape` needed.
- **BUILD.md placement**: input dir is `src/`, so this file (and other repo
  docs) are never picked up as Markdown templates — no stray `BUILD/index.html`.

## Irreducible residue

None. Raw diff is 0 and all 8 files are byte-identical (`cmp`).
