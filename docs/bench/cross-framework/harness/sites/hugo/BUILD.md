# Hugo benchmark port — byte-parity build

Reproduces the golden target (`../../golden (the vendored harness target)`, 8 files) at
**TOTAL raw=0 normalized=0** with `compare.py`.

## Binary

- Hugo **v0.168.0** (extended + withdeploy build, darwin/arm64), installed with `brew install hugo` (86 MB binary).

## Data

`data/stocks.json` is a verbatim copy of `demo-site/data.json` (4 stocks + `cycle_id`).
All pages are data-driven: stock pages come from the content adapter
`content/stocks/_content.gotmpl`, and the home page list / sitemap iterate
`hugo.Data.stocks` directly. To refresh:

```sh
cp ../../data.json (the vendored harness dataset) site/data/stocks.json
```

## Build

From the directory containing `site/` (output goes to `../out` relative to `site/`,
i.e. `./out`; delete it first for a clean build):

```sh
rm -rf out
hugo --source site --destination ../out \
  --cacheDir "$PWD/cache" --noBuildLock --logLevel warn
python3 ../../../compare.py out ../../golden   # expect TOTAL raw=0 normalized=0
```

`--cacheDir` and `--noBuildLock` just keep caches and `.hugo_build.lock` out of
`site/`; they don't affect output bytes.

## Expected output layout (8 files, nothing else)

```
out/index.html
out/robots.txt
out/sitemap.xml
out/stocks/{KO,PEP,SAN,T}/index.html
out/style.css
```

## Quirks / what had to be disabled and why

- `disableHugoGeneratorInject = true` — golden has no `<meta name="generator">`.
- `disableKinds = ["taxonomy", "term", "rss", "section", "404"]` — golden has no
  feeds, no `/stocks/` section index, no taxonomy pages, and `404.html` would show
  up as `extra_html` in compare.py.
- **Uppercase URLs** (`/stocks/KO/`): Hugo lowercases content-adapter page paths
  regardless of `disablePathToLower`, so each `.AddPage` sets an explicit
  `"url" (printf "/stocks/%s/" .ticker)`. That alone preserves case in both the
  output directory names and `.Permalink` (canonical/og:url); `disablePathToLower`
  turned out to be unnecessary and is not set.
- **Sitemap is manual**: Hugo's built-in sitemap format doesn't match the golden
  file, so `layouts/sitemap.xml` overrides it and renders `<url><loc>…</loc></url>`
  entries straight from the data. The XML declaration must be emitted via
  `printf`/`safeHTML` (see the first line of `layouts/sitemap.xml`) — as literal
  text, html/template escapes it to `&lt;?xml`.
- **robots.txt** via `enableRobotsTXT = true` + `layouts/robots.txt`.
- **Whitespace is load-bearing** (raw diff counts bytes):
  - `layouts/home.html` uses an *untrimmed* `{{ range }}` loop, producing a blank
    line before each `<li>` and one after the last.
  - `layouts/page.html` keeps the badge `if` untrimmed on its own indented line:
    confirmed stocks get a line containing exactly two spaces inside `<section>`.
  - Both HTML layouts end with `</html>` and **no trailing newline**; golden HTML
    files have none (robots.txt and sitemap.xml do end with a newline).
- `AT&T Inc` renders as `AT&amp;T Inc` automatically via Go html/template
  escaping; em dashes (—) and the ← arrow are literal UTF-8 in the templates.

## Residue

None — byte parity (raw=0) on all 8 files, no missing files, no extra HTML.
