# Changelog

Every change to molde's public surface (the command line, the site format, the check
report) is listed here under the release that makes it, with what to do. What each kind
of release may change is in the reference's
[compatibility](docs/reference.md#compatibility) section.

## Unreleased

### Added

- Templates get `page.title`, `page.description` and `page.index`: the page's rendered
  title and description patterns (the strings `std_head` prints) and its `index` value,
  so a template can add `twitter:title` or a tag only on indexable pages without
  repeating the route's patterns. Nothing to do.
- The check report's `pages_by_locale` entries and `i18n.sparse_locale` warnings carry
  the route's `template` and, on `foreach` routes, `foreach` (the collection) next to
  `route`, so routes that share a path pattern can be told apart. Nothing to do.
- `molde verify DIR`: molde's output checks on any generator's build (Hugo, Astro, a
  Next.js static export, Eleventy, Zola, or molde's own `out/current`). It prints `check`'s
  report shape plus `command`, `pages`, `warnings` and `skipped`, and exits 1 on an error.
  - **Rules for foreign output:** every file is classified (not-found pages, redirects and
    aliases, noindex pages), and one URL model handles base paths, clean URLs and
    `<base href>`.
  - **New check ids:** `links.outside_base`, `links.clean_urls`, `canonical.off_host`,
    `canonical.target_missing`, `canonical.target_noindex`, `canonical.shared`, `redirect.target_missing`,
    `sitemap.non_canonical_listed`, `sitemap.not_found`, `verify.base_url_mismatch`,
    `verify.no_pages`, `verify.project_root` and the warning `verify.all_noindex`.
  - **Release gate:** each generator's official starter, committed in
    `tests/verify/fixtures`.
  - See [Verify any site](docs/reference.md#verify-any-site). `molde check` is unchanged.
- A GitHub Action (`uses: ManuelFontRodriguez/molde@<version>`) runs `check` or `verify`:
  - it downloads the release it is pinned to (or `version:`), with its SHA-256 checked;
  - it writes the report to the job summary and keeps one comment on the pull request up
    to date, per workflow, job and site, escaping everything the report quotes;
  - it fails the job when the check fails (its verdict, or molde's exit code);
  - every input is in `action.yml`.

  CI runs the action on the fixtures in every pull request.

### Changed

- `links.internal` compares file names exactly, case included, on every file system. On
  macOS (which ignores case by default) a link to `/Logo.webp` used to pass when the file is
  `logo.webp`, and broke on the host (S3, Linux). A site with such a link now fails on every
  machine: fix the spelling the report names. The build's files are listed once, instead of
  asking the disk for each target.

### Fixed

- `titles.duplicate` counts indexable pages only. A page carrying a robots noindex meta
  (a not-found page written three times, as Next.js does, or a pager marked `index:
  false`) is not a search result and may share a title. A site that failed only on such
  pages passes. Nothing to do.
- A robots noindex inside a comment, a script, a style or a `<template>` is not the
  page's: `sitemap.noindex_listed` fired on a listed page whose noindex was commented
  out, and `titles.duplicate` would have skipped it. Crawlers index such a page.
- `canonical.mismatch`, `og.mismatch` and `canonical.not_in_sitemap` compare URLs as a
  browser does: the scheme and host in any case, and an empty path the same as `/`
  (`https://Example.com` and `https://example.com:443` are `https://example.com/`). A hand-written canonical that named
  the right page in another spelling used to fail; it passes. The path is still compared
  exactly. `std_head` always wrote the normal form, so molde's own pages are unaffected.
- `html.lang` accepts a more specific tag of the page's locale: `<html lang="en-US">` on
  an `en` page used to fail, though it is valid and names the same language. A region
  of another language (`es-US` on an `en` page) still fails, and hreflang stays exact.
  Nothing to do.
- `molde serve` answers a miss with the nearest `404.html` up its path, so a prefixed
  locale's `/de/404.html` shows under `/de/`; it used to show the root `/404.html`
  everywhere. Nothing to do.
- `molde serve --watch` answers from the start: it used to accept connections and answer
  nothing until the startup check finished (minutes on a large site). The last good
  build is served while that check runs, and with no build yet a "first build in
  progress" page reloads itself until one publishes. Nothing to do.
- `titles.multiple` counted an icon's `<title>` as a second page title when an SVG
  sprite nests an `<svg>` inside a `<symbol>` (Sphinx's Furo theme does, on every page):
  SVG and MathML are now skipped to their matching end tag. Nothing to do.
- A reader that stops early (`molde check --site site | head`) no longer makes molde
  panic with "failed printing to stdout: Broken pipe"; the exit code is the command's
  own. Nothing to do.
- `links.internal` read `href=` and `src=` in page text too, so a docs page showing HTML
  in a code block (`&lt;link href=&quot;/favicon.ico&quot;&gt;`) failed on a link that
  isn't one. Attributes are now read inside tags only, plus quoted URLs inside
  `<script>` as before; a real link inside `<code>` still counts. `href=` text inside
  another attribute's value, and framework bindings (`:href`, `x-bind:href`), are not
  links either (gohugo.io's 788 and Starlight's 246 false errors). Nothing to do.

## 0.1.0 — 2026-10-19

The first public release. Since 0.1.0-rc.1:

### Added

- New checks: `canonical.duplicate` and `og.duplicate` (a second canonical link, or a
  second `og:url`, `og:title` or `og:type`, usually a hand-written tag next to
  `{{ std_head }}`), and `og.title_mismatch` (`og:title` and `<title>` say different
  things; one containing the other passes). A site that passed may now fail: remove the
  extra tag, or make the og:title agree with the title.
- The head checks read the markup only: a canonical or Open Graph tag inside a comment,
  a script, a style or a `<template>` is no longer taken for the page's own.
- New checks on `robots.txt`, with `base_url`: `robots.sitemap_missing` (no `Sitemap:`
  line naming `<base_url>/sitemap.xml`) and `robots.blocks_all` (`User-agent: *` with
  `Disallow: /` on a site with pages to index). molde's own robots.txt passes both; a `static/robots.txt`, which
  replaces it, is now checked the same way.
- `molde guide` prints [docs/agents.md](docs/agents.md), a short guide for agents and
  people: the site layout, `routes.yaml`, templates, what the check covers and the
  commands. `molde --help` points to it. Nothing to do: it is new.

### Changed

- The head checks (canonical, Open Graph, description) read attribute values in any
  quoting, as browsers do: `'…'` and unquoted as well as `"…"`, with attribute names in
  any case. A correct head written with single quotes used to fail `canonical.missing`
  and `og.missing`; it now passes. A single-quoted tag that contradicts `std_head` used
  to be invisible; it is now checked.

- `links.internal` checks links written absolute to the site's own address. With
  `base_url: https://example.com`, `https://example.com/x/`, `http://example.com/x/` and
  `//example.com/x/` are links into the build, checked like `/x/`; before, they were
  taken for another site and never checked. A site with such a link to a missing page
  now fails: fix the link the report names. Other hosts are still not fetched.

## 0.1.0-rc.1 — 2026-10-16

The first public release candidate.

### What molde does

- `molde check` renders the site into an immutable build, runs every conformance
  check, and returns one JSON report. It publishes (swaps `out/current`) only on a
  passing verdict; otherwise the last good build stays live.
- Commands: `build`, `check`, `routes`, `render`, `diff`, `gc`, `serve` (with `--watch` and
  `--reload`), `events`, `mcp`.
- Site format: `routes.yaml` (path patterns, `foreach`, `paginate`, title,
  description, index and lastmod patterns, JSON-LD types), minijinja templates,
  `data.json`, an optional `site.yaml` (web standards, size budgets, locales) and
  `translations/`.
- Pages can be left out of search engines with `index: false` (noindex meta, kept out of
  the sitemap). With `base_url`, molde writes `sitemap.xml` and a `robots.txt` that points
  at it (your own `static/robots.txt` wins).
- Locales: the default locale at the root, or every locale prefixed
  (`prefix_default_locale`); `localized:` fields per route; `locales: none` for pages that
  belong to no language.
- Template filters of molde's own: `slug`, `year`, `date_format` and `num_format`, and
  with locales `t`, `t_html` and `loc`.
- Builds: every page gets a precompressed `.gz` sibling, an unchanged rebuild reuses the
  previous build, `molde diff` compares any two builds (`--from`, `--to`, `--full`),
  `molde gc` prunes them, and `molde serve` answers missing pages with the site's
  `/404.html`. `molde events` takes `--port` and `--min-interval-secs`.
- Checks: internal links (relative ones, `srcset` and stylesheet `url()` included),
  text escaped twice, titles, the sitemap, canonical and Open Graph tags,
  descriptions, robots, JSON-LD, output budgets, locales (paths, `lang`, hreflang,
  translations), and a source layer that reports template and file errors with the
  file, line, page and route.
- `group_by` makes one page per distinct value of an item field (genre or tag pages),
  following the data with no hand-kept list. `{field|slug}` in a path and the `slug`
  filter in templates turn any value into a URL segment (`Science Fiction` →
  `science-fiction`).
- Errors name the fix: a `--site` folder without `routes.yaml`, a `foreach` or
  `paginate.over` collection missing from `data.json` (with the arrays that exist), a
  `{field}` path value that is not a string or an integer, and a date filter given a
  number. Integer fields (`"id": 42`) work in paths, and a UTF-8 byte order mark at the
  start of `data.json` or a YAML file is ignored.
- `.DS_Store` and `Thumbs.db` under `static/` are not copied into builds, and a static
  page's unquoted `<meta name=robots content=noindex>` counts as noindex like the
  quoted forms.
- Above 50,000 URLs or 50 MB, `sitemap.xml` becomes a sitemap index of
  `sitemap-1.xml`, `sitemap-2.xml`, ….
- `molde mcp` serves all of this to an agent over the Model Context Protocol.
- Three example sites: `examples/demo` (the quickstart), `examples/multilingual` (three
  languages) and `examples/broken` (five mistakes and the report that finds them).
- Dates and numbers are formatted for 529 locales from bundled Unicode CLDR 48 data,
  matching ICU byte for byte.
- Release archives for Linux (x86-64, arm64; static) and macOS (arm64, x86-64), with
  SHA-256 checksums and third-party licence notices.
- molde runs on Linux and macOS and needs Rust 1.89 or newer to build from source.
