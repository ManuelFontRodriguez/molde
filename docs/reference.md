# molde site reference

Everything a site author — human or agent — needs. For a first look, read the short
[guide](agents.md) (`molde guide`) instead. A site is a folder of pure data (routes,
data, templates, and optional static files, settings and translations); the framework
binary does the rest. If this document and `examples/demo` disagree, that's
a bug: file it.

**Contents:** [Site anatomy](#site-anatomy) · [static/](#static) ·
[routes.yaml](#routesyaml) · [site.yaml](#siteyaml) · [Locales](#locales) ·
[Locale data](#locale-data) · [Template contract](#template-contract) ·
[JSON-LD](#json-ld) · [Partials & macros](#partials--macros) ·
[URL conventions](#url-conventions) · [Output layout](#output-layout) ·
[The check report](#the-check-report) · [Verify any site](#verify-any-site) ·
[Security model](#security-model) ·
[Compatibility](#compatibility) · [Commands](#commands)

## Site anatomy

```
site/
├── routes.yaml      # the router — required
├── data.json        # the snapshot — required
├── site.yaml        # capability config — optional
├── templates/       # minijinja templates — required
├── translations/    # <locale>.yaml strings for t() — optional, with `locales:`
└── static/          # copied verbatim into every build — optional
```

## static/

Everything under `static/` is copied into the build as-is (`static/style.css` →
`/style.css`), except the file-manager litter `.DS_Store` and `Thumbs.db`. Text-like assets (css, js, svg, txt, xml, json, html, htm, map, webmanifest) get
`.gz` precompressed siblings; binaries don't. On a path collision, **routes win** — routes
are the site's source of truth. The link check covers `src="…"` as well as `href="…"`,
so a template referencing a missing asset fails the build.

With `base_url` set, a `robots.txt` pointing at the sitemap is emitted automatically;
ship your own `static/robots.txt` to override it. Yours is checked like molde's: it must
name `<base_url>/sitemap.xml` in a `Sitemap:` line (`robots.sitemap_missing`), and its
`User-agent: *` rules must not disallow `/` with nothing allowed back while the site
has pages to index (`robots.blocks_all`).

`static/*.html` files are not routes, so they are **never in the sitemap**. With
`base_url` set, a static HTML page passes `sitemap.missing_page` only if it carries
`<meta name="robots" content="noindex">` (a hand-written 404, a verification page); the
attribute values may be double-quoted, single-quoted or unquoted.
Anything meant to be indexed should be a route.

## routes.yaml

```yaml
routes:
  - path: /index.html                     # output path; static route
    template: index.html.j2               # file inside templates/
    title: "Demo — a small library"       # optional; enables <title> + OG via std_head

  - path: /books/{id}/index.html         # {field} filled from each item (a string or an integer field)
    template: book.html.j2
    foreach: books                        # render once per item of data.json's top-level "books" array
    title: "{{ item.title }} by {{ item.author }}"   # minijinja pattern, rendered per page
    index: "{{ item.indexable }}"         # optional; bool or pattern → true/false. Default true
    description: "{{ item.title }} ({{ item.published | year }}), {{ item.genre }}"   # optional; meta + og:description
    lastmod: "{{ item.updated }}"         # optional; sitemap <lastmod>, YYYY-MM-DD or RFC 3339
    jsonld: [WebPage, Book]               # optional; @types every page's JSON-LD must carry

  - path: /catalogue/index.html           # page 1 of a paginated list lives at the route's path
    template: catalogue.html.j2
    paginate:
      over: books                         # top-level array in data.json
      per_page: 100
      path: /catalogue/page/{n}/index.html  # pages 2..n; must contain {n}
    title: "The catalogue{% if pagination.n > 1 %}, page {{ pagination.n }}{% endif %}"

  - path: /404.html
    template: 404.html.j2
    title: Page not found
    index: false                          # rendered, never in sitemap.xml, noindex meta

  - path: /llms.txt                       # non-HTML output: no std_head, never in the sitemap
    template: llms.txt.j2
```

- `path` names a file and starts with `/` (usually `…/index.html` so URLs are pretty; a
  path written without the leading `/` gets one). It may not end with `/` (write
  `/home/index.html`, not `/home/`), contain spaces, control characters or any of
  `? # % " ' < > \`, or have an empty, `.` or `..` segment, and a segment is at most
  255 bytes. Letters in any script are fine; they are written as is, not
  percent-encoded, in the file name, the sitemap and the canonical URL. A `{field}` value from `data.json` follows
  the same rules and is one segment (no `/`). The value is a string or an integer
  (`"id": 42` gives `/books/42/`); a float, boolean, `null` or missing field fails with
  `config.invalid`, naming the route and the item (`books[3]`). A path that breaks a rule fails the check
  with `config.invalid`, naming the route or the value.
- `{field|slug}` writes the value as a slug instead: lowercase ASCII letters and digits,
  words joined by `-` (`Science Fiction` → `science-fiction`, `Ciencia
  ficción & fantasía` → `ciencia-ficcion-fantasia`, `Straße` → `strasse`). Accents are dropped and every
  other character separates words. Letters with no ASCII spelling (Cyrillic, CJK) are
  left out, and a value with nothing left is a `config.invalid` error: give those items
  a slug field of their own. Two values with the same slug (`Science Fiction`,
  `science-fiction`) would write one file twice and fail with `paths.duplicate`; they are
  never merged. Templates link to such a page with the `slug` filter, which is the same
  function: `href="/genres/{{ b.genre | slug }}/"`. `slug` is the only filter a path
  placeholder takes; a placeholder needs `foreach` to fill it, and a malformed one
  (`{{x}}`, `{x|}`) fails loading the site. The `slug` filter takes a string or an
  integer and is a render error on anything else (`null`, a missing field, a list), so
  a link can't quietly point at `/genres/none/`.
- `foreach` names a **top-level array** in data.json; each element becomes `item`. A name
  that is not there fails with `config.invalid`, naming the route and listing the
  top-level arrays that do exist (the same for `paginate.over`).
- `group_by` (with `foreach`) makes **one page per distinct value** of an item field
  instead of one per item: genre pages, tag pages, author pages. The page's `item` is
  `{"<field>": value, "items": [the collection's items with that value, in data order]}`,
  so `path: /genres/{genre|slug}/index.html` and `{% for b in item.items %}` work as
  written. Groups come in the order each value first appears in the data. A field that
  is a list (`tags: [a, b]`) puts the item in every group it names, and an empty list in
  none. Values must be non-empty strings or numbers and follow the path rules (or use
  `{field|slug}`, since real values such as "Science Fiction" hold spaces); an item
  with no value is a `config.invalid` error (give every item one, or leave it out of the
  collection), never silently dropped. A new value in the data gets its page on the next
  build, with no list to keep in sync. Not yet supported: a `localized` field, and
  `paginate` within a group.

  ```yaml
  - path: /genres/{genre|slug}/index.html
    template: genre.html.j2
    foreach: books
    group_by: genre
    title: "{{ item.genre }} books"
  ```
- `paginate` splits a **top-level array** (`over`) into pages of `per_page` items. Page 1
  renders at the route's own `path`; pages 2..n at `paginate.path` with `{n}` filled in
  (the only placeholder allowed there; quote the path if you write the block inline,
  `{path: "/p/{n}/index.html"}`, or YAML reads the braces as a mapping). The template and
  every route pattern (`title`, `description`, `index`, `lastmod`) get `pagination`
  (see the [Template contract](#template-contract)), so page 2+ can get its own title and
  pass `titles.duplicate`, or be `index: "{{ pagination.n == 1 }}"`. An empty list still
  renders page 1 with no items; a list that divides evenly has no trailing empty page.
  `paginate` and `foreach` on one route fail the build, as do `per_page: 0`, a route
  `path` with placeholders, a `paginate.path` without `{n}` and an `over` that isn't a
  top-level array. molde emits no `rel="prev"/"next"`
  links (Google ignores them); link pages with `pagination.links` so crawlers
  reach every page.
- `title` is a minijinja pattern rendered as **plain text** per page. It is the single
  source for `<title>`, `og:title` (emitted via `std_head`). Make it item-dependent on
  `foreach` routes or the duplicate-title check fires. A pattern referencing a missing
  field renders as empty string and surfaces as `titles.missing`.
- `index` (default `true`) says whether search engines may index the route's pages. It is
  a YAML bool, or a minijinja pattern rendered per page (same `item`/`data` context and
  filters as `title`) that must render `true` or `false` (`True`/`False` too — that's how
  minijinja prints a boolean). Anything else, including the empty string a misspelled
  field renders to, **fails the build** with an error naming the route and the page.
  An `index: false` page is rendered and linkable but left out of `sitemap.xml`, and
  `std_head` emits `<meta name="robots" content="noindex">` (plus `<title>`) in place of
  canonical + OG. The value is resolved at route expansion, so `list_routes` /
  `explain_route` report exactly what the build does.
- `description` is a minijinja pattern rendered per page as plain text, exactly like
  `title` (escaped once, leading/trailing whitespace trimmed). `std_head` emits
  `<meta name="description" content="…">`, and `og:description` inside the OG block
  whenever that block is emitted (titled, indexable pages). A route that declares it but
  renders empty for a page fails `description.missing`; two indexable pages with the
  same description fail `description.duplicate`. molde doesn't enforce a length; aim for
  roughly 50–160 characters.
- `lastmod` is a minijinja pattern rendered per page into the page's sitemap
  `<lastmod>`. Empty output means no `<lastmod>` for that page. Otherwise it must be
  `YYYY-MM-DD` or an RFC 3339 datetime (`2026-09-23T09:00:00Z`,
  `2026-09-23T09:00:00.5+02:00`); anything else is `sitemap.lastmod_invalid` and is left
  out of the sitemap. molde **never** fills it from the build time: derive it from the
  data (when the page's data last changed), e.g. `"{{ item.as_of }}"` or
  `"{{ data.books | map(attribute='updated') | max }}"`. Resolved at route expansion,
  so `explain_route` shows it.
- `jsonld` names the schema.org `@type`s every page of the route must carry in its
  `<script type="application/ld+json">` blocks: a string or a list. A type counts
  wherever it appears — top level, inside `@graph`, or nested (`about`, `publisher`),
  as a string `@type` or one entry of a list, bare (`Book`) or as a schema.org
  IRI (`https://schema.org/Book`, `schema:Book`). A page missing one fails
  `jsonld.type_missing`. See [JSON-LD](#json-ld). An empty `jsonld`, or `jsonld` on a
  non-`.html` route, fails loading the site.
- **Only `.html` paths are web pages.** A route whose path doesn't end in `.html`
  (`/llms.txt`, `/feed.xml`, `/search.json`) is rendered like any other, but it is never
  in the sitemap, `std_head` renders empty in it, and the title/canonical/OG/robots
  checks skip it. It is still a file in the build: links to it resolve, and it takes
  part in `paths.duplicate`.
- `path` (and `paginate.path`) may be a per-locale map, and a route may set `locales:`
  and `localized:` — only with `locales:` in site.yaml. See [Locales](#locales).

Files in `routes.yaml`, `data.json`, `site.yaml` and `translations/` may start with a
UTF-8 byte order mark; it is ignored. `--site` must name the folder that holds
`routes.yaml`; any other folder fails with `config.invalid` naming it.

## site.yaml

A present key enables a capability **and** the conformance checks that ship with it:

```yaml
base_url: https://example.com   # web-standards: sitemap.xml, std_head, canonical/OG checks
locale: es-ES                   # locale filters: date_format + num_format in templates
# locales: [es, en, de]         # instead of `locale`: one site in several languages, see Locales
budgets:                        # output size budgets: budget.exceeded / budget.unmatched
  - match: /indexes/*/index.html
    max_gzip_bytes: 60000
  - match: "*.html"
    max_bytes: 600000
  - match: /app.js
    max_bytes: 6000
```

No `site.yaml` (or no `base_url`) = none of that exists, `{{ std_head }}` renders empty.
`base_url` must be an absolute URL (`http://` or `https://` and a host, optionally a path
such as `https://example.com/blog`, with no `?query` or `#fragment`); anything else
fails loading the site.

`sitemap.xml` lists every indexable page. Above the protocol's per-file limits (50,000
URLs or 50 MB) it becomes a sitemap index pointing at `sitemap-1.xml`, `sitemap-2.xml`,
…, each within the limits; `robots.txt` points at `sitemap.xml` either way, and the
sitemap and canonical checks read through the index.
An unknown key in `site.yaml` or in a route (a typo such as `budget:` or `titel:`) fails
loading the site, so a misspelled capability can't silently switch itself off.

`locale` registers presentation filters so contract data stays canonical (ISO dates,
dot-decimal strings) while pages read localized. The data is Unicode CLDR 48, bundled
in the binary for 529 locales (every locale with basic coverage or better). No host ICU
is used, so output is the same on every machine and matches what ICU 78 / PHP intl
prints. The tag is resolved CLDR's way: `es-ES` uses `es` (no `es-ES` data of its own),
`en-GB` and `pt-PT` use their region's data, `zh-TW` uses Traditional Chinese, and
`pt_PT` is read as `pt-PT`. Any tag is accepted. A `date_format` / `num_format` call in a
locale CLDR has no data for (`tlh`) fails the build loudly when it renders, naming the
locale, the page and the template; a site that never calls them builds as usual. No
conformance checks ship with this key yet. It is for single-language sites; `locale` and
`locales` together fail loading the site. How tags resolve and what the formatters
print in detail: [Locale data](#locale-data); why it works this way:
[design/cldr-formatting.md](design/cldr-formatting.md).

`budgets` is a list of rules; each file of the build a rule's `match` glob selects must
stay within the rule's limits, or `budget.exceeded` fires (one error per file and
metric, carrying `file`, `match`, `metric`, `size`, `limit`, and `page`; files a route
wrote also get `route`/`template`). Every file is checked against every rule that matches it.

- `match` is a glob over the build path with a leading `/` (`/indexes/sp-500/index.html`):
  `*` stays within one path segment, `**` crosses segments (`/**/x` includes `/x`),
  `?` is one character, everything else is literal. Without a leading `/` the glob
  matches at any depth (`"*.css"` ≡ every .css file). Quote globs that start with `*`
  (YAML reads a bare `*` as an alias). A trailing `/` means everything below that
  directory (`/indexes/` ≡ `/indexes/**`).
- `max_bytes` limits the file as written; `max_gzip_bytes` limits its `.gz` sibling —
  the bytes a gzip-serving host actually sends. A file with no sibling (images, fonts)
  is measured raw for both. A `.gz` sibling of a built file is never matched on its
  own; a `.gz` with no uncompressed twin is an ordinary file.
- A rule needs at least one limit (a rule with neither fails loading the site), and it
  must match at least one file: a rule that matches nothing guards nothing and fails
  `budget.unmatched` (usually a typo in the glob).

## Locales

`locales:` in site.yaml turns on the locale dimension: every route renders once per
locale, and hreflang, `t()` and their checks come with it. Without the key none of
this exists and a site builds byte-identical to before.

```yaml
# site.yaml
base_url: https://example.com
locales: [es, en, de]          # the first is the default and lives at the root
# prefix_default_locale: true  # optional: /es/… too (then add a root page, see below)
```

```yaml
# routes.yaml
routes:
  - path: /index.html          # es /index.html, en /en/index.html, de /de/index.html
    template: home.html.j2
    title: "{{ t('home.title') }}"

  - path:                      # one pattern per locale; a locale left out gets no page
      es: /libros/{slug}/index.html
      en: /books/{slug}/index.html
      de: /buecher/{slug}/index.html
    template: book.html.j2
    foreach: books
    localized: [slug, title]   # item fields that are {locale: value} maps
    title: "{{ item.title | loc }} by {{ item.author }}"

  - path: /es-only.html
    template: x.html.j2
    locales: [es]              # a subset; default all

  - path: /feed.xml
    template: feed.xml.j2
    locales: none              # once, at this path verbatim, outside the locale dimension
```

```json
{"books": [{"author": "Mary Shelley",
  "slug": {"es": "frankenstein", "en": "frankenstein", "de": null},
  "title": {"es": "Frankenstein o el moderno Prometeo", "en": "Frankenstein", "de": null}}]}
```

- **The path rule:** the default locale's page is the unprefixed path; every other
  locale's is `/<prefix>` + it, where the prefix is the lowercased tag (`pt-BR` →
  `/pt-br/`). With `prefix_default_locale: true` the default is prefixed too, and a
  `locales: none` route at `/index.html` (a chooser) is required (`paths.root_missing`).
- **Tags** are a 2- or 3-letter language with an optional region (`es`, `fil`, `pt-BR`),
  case as written (not `pt-br`, `es-419` or script tags like `zh-Hant`; `locale=` on
  the filters accepts those). `prefix_default_locale` without `locales:` is a load
  error.
- **Localized fields:** every item must carry each field named in `localized:`, as an
  object whose keys are site locales and whose values are strings or `null`; anything
  else fails the build. An item exists in a locale only if **every** declared field has
  a value there — `null` or a missing key means **no page**, never a fallback. The same
  rule decides a `foreach` page and a paginated list entry (judged by the paginate
  route's own `localized:`), so a list never links to a missing page and `| loc` on a
  declared field never fails on a built page.
- **Templates can't print a localized field as text:** `{{ item.name }}`,
  `{{ item.name | upper }}`, `{{ item.name ~ '' }}`, `{{ item.name | tojson }}` and
  `{{ item }}` are all render errors, in templates and route patterns. Write
  `{{ item.name | loc }}` (the page's locale; errors when it has no value) or
  `{{ item.name[locale] }}` (empty when missing). `| loc` also takes a plain
  `{locale: value}` map from `data` whose keys are all site locales.
- **hreflang groups:** a page's group is the same route's pages for the same item
  (foreach), the same page number (paginate) or the page itself (static route), in
  every locale. Only indexable HTML pages count, and a group needs two of them (the
  template's `alternates` keeps noindex versions, for the language switcher).
  `std_head` then emits `<link rel="alternate" hreflang="…" href="…">` per member,
  itself included, plus `x-default` pointing at the default locale's member when that
  member exists. The canonical stays self-referencing; the sitemap carries no
  alternates.
- **Translations:** `translations/<tag>.yaml`, nested keys joined with dots
  (`home: {title: …}` ≡ `home.title: …`; defining both is a load error). Values are
  strings. Only `<tag>.yaml` files are read (`.yml` is ignored), and a file for a tag
  that isn't in `locales:` is a load error. The default locale's file defines the key
  set (it must exist once the directory does, and none of its values may be empty).
  `{{ t('greet', name=x) }}` fills the default locale's `{name}` placeholders in one
  pass (a placeholder the call doesn't pass is a render error on every page, whether or
  not the page's locale has the key) and is escaped in HTML; `t_html` keeps the
  translation's own markup unescaped, for strings that carry it, and escapes arguments.
  Every named argument must be a placeholder of the string (a typo is a render error).
- **Text pools:** `t(key, args=mapping)` / `t_html(key, args=mapping)` take placeholder
  values from a mapping whose unused entries are fine, for sites that keep several
  variants of a sentence and pick one per item, e.g.
  `{{ t_html(item.texts.faq1, args=item.faq_args) }}`. A placeholder neither the bag
  nor a named argument fills is still a render error naming the key; named arguments
  stay strict and win over a bag entry of the same name. `args` that isn't a mapping is
  a render error, and `args` is reserved (a `{args}` placeholder is read from the bag).
  A key missing in a locale renders empty and fails `translations.missing`. Without a
  `translations/` directory, `t()` is a render error.
- **Any well-formed tag is a locale** (`ja`, `ar`, `zh`, `no`, `pt-BR`): paths, hreflang,
  `t()` and `| loc` need nothing per language. Only the formatting filters need data,
  and CLDR's covers all of these — see the next point.
- **Formatting follows the page:** `date_format` and `num_format` use the page's locale;
  `locale="en"` overrides it for one call. They format in any of the 529 CLDR locales
  molde bundles (see [site.yaml](#siteyaml)); a call that renders in a locale without
  data — the page's or an explicit `locale=` — is a render error that aborts `check`,
  e.g. `date_format: locale 'tlh' has no formatting data — molde bundles CLDR 48.0.0 for
  529 locales (coverage basic or better) (page /tlh/) (in home.html.j2:2)`. Pages of
  that locale that don't call the filters, and other locales' pages, are unaffected.
- **Text direction is the template's job:** molde never touches `<html>`. For
  right-to-left locales (`ar`, `he`) derive it from `locale`, e.g.
  `<html lang="{{ locale }}"{% if locale in ["ar", "he"] %} dir="rtl"{% endif %}>`.
- **Paths must be plain URL characters** under locales (`paths.invalid_chars`):
  ASCII letters, digits, `- . _ ~ /`. Transliterate slugs in the data.
- `list_routes` shows each route's `locales`, `localized` and `pages_by_locale`;
  `explain_route` shows the page's `locale`, `alternates` (`{locale, path, index}`) and
  `localized_fields`.

## Locale data

`date_format` and `num_format` print from Unicode CLDR 48 tables compiled into the
binary. Nothing is read at run time (no environment variable, locale file or system
ICU), and rounding works on the decimal digits, never through floating point, so the
output depends only on the tag, the value, the arguments and the molde version. The
reference output is ICU 78.1 (CLDR 48) as printed by PHP's intl; `tests/cldr.rs`
compares molde with it string by string.

**Locale resolution.** A tag is canonicalized first (`pt_br`, `PT-BR` → `pt-BR`;
`zh-hant-tw` → `zh-Hant-TW`), then looked up in this order:

1. the tag itself;
2. one of 33 aliases: script changes from CLDR's likely subtags (`zh-TW → zh-Hant`,
   `zh-HK → zh-Hant-HK`, `sr-ME → sr-Latn-ME`, …) and deprecated codes (`iw → he`,
   `in → id`, `tl → fil`, `mo → ro`, …);
3. the tag with its last subtag dropped (`es-ES → es`), and steps 1–3 again.

- Region data is used when CLDR has it: `en-GB` prints `5 March 2026`, `en-IN` groups
  `12,34,567.50`, `de-CH` groups with `'`, `fr-CA` prints `2026-03-05` short.
- `zh` is Simplified Chinese; `zh-TW` and `zh-HK` get Traditional.
- `no`, `nb` and `nn` all resolve; WordPress's `nn_NO` is Nynorsk (`nn`).
- A tag nothing resolves to (`tlh`, `xx`, `und`, or a locale below CLDR's basic
  coverage) fails when a filter renders in it, naming the locale, the page and the
  template.

**Digits and separators** follow the locale's default numbering system, as ICU does,
including invisible characters: `ar` prints `‎-1,234.50` (Latin digits, U+200E before
the minus), `ar-EG` `؜-١٬٢٣٤٫٥٠`, `fa` `۱٬۲۳۴٫۵۰`, `hi` groups `12,34,567.00`. French
groups with U+202F, ru/pl/uk/sv/no/fi/pt-PT with U+00A0, `de-CH` with `'`; sv, no and fi
use U+2212 as the minus sign. As in ICU, `es` prints `1.234,50` (no minimum grouping).

**Dates.** Only a strict `YYYY-MM-DD` (years 0001–9999, a day that exists) is a date;
anything else passes through unchanged (datetimes, `2026-3-5`, `2026-02-30`, empty
strings). Every locale is formatted in the Gregorian calendar with CLDR's Gregorian
patterns, proleptically before 1582. Two known differences from ICU follow from that:
ICU prints `0001-01-01` in the Julian calendar, and for `fa` (`full`, `month`) and `th`
(`long`) it applies the Persian or Buddhist pattern; `tests/cldr.rs` lists these rows
and fails if one starts matching. `MMMM` is the format month name (ru `марта`), `LLLL`
the stand-alone one (ru `март`).

**Numbers.** Rounding is half-even on the decimal string: `0.125 → 0.12`,
`2.675 → 2.68`, `999.995 → 1,000.00`. `-0.001` at `decimals=2` keeps its sign
(`-0.00`), as ICU does. `decimals=N` drops leading integer zeros (`007.5` → `7.5`);
without it the value is printed as given. Very long numbers stay exact (ICU would print
floating-point noise past ~15 digits). A value that is not a plain decimal (`""`,
`1.2.3`, `.5`, `1,5`, `+1`, `1e5`, `NaN`) passes through unchanged.

## Template contract

Templates are [minijinja](https://docs.rs/minijinja) (the Rust Jinja2 dialect). Context
variables:

| Variable | Present | Contains |
|---|---|---|
| `data` | always | the entire data.json snapshot |
| `item` | `foreach` routes only | the current array element |
| `pagination` | `paginate` routes only | `n` (this page, from 1), `pages`, `per_page`, `total` (list length), `offset` (index of this page's first item, from 0), `items` (this page's slice), `prev` / `next` (public path like `/largest/page/2/`, or none), `links` (`[{n, path}]` for every page, for the pager) |
| `page.path` | always | the public path of this page: `/books/dune/` for `/books/dune/index.html`, `/llms.txt` as is |
| `page.url` | when `base_url` is set | `base_url` + `page.path`, byte-identical to the canonical and the sitemap `<loc>` |
| `page.title` / `page.description` | where `std_head` prints them (`base_url` set, `.html` outputs) on routes that declare them | the route's `title` / `description` pattern rendered for this page, as plain text: the same strings as `<title>` and the meta description (escaped once wherever you print them). For extra tags such as `twitter:title`. Undefined elsewhere |
| `page.index` | always | the route's `index` rule resolved for this page (`true` / `false`), for tags only indexable pages should carry |
| `locale` | with `locales:` | the page's locale tag (`en`); the default locale on `locales: none` pages. Use it for `<html lang="{{ locale }}">` |
| `locales` | with `locales:` | every site locale, default first |
| `alternates` | with `locales:` | the page's locale versions, itself included, noindex ones too: `[{locale, path, url?}]` (`url` with `base_url`), no `x-default`, empty when the page exists in one locale only. For a language switcher (hreflang is the indexable subset) |
| `std_head` | always (empty unless `base_url` set, and in non-`.html` outputs) | pre-escaped head block: `<title>` + meta description + canonical + OG, or `<title>` + meta description + robots noindex on `index: false` pages |

Render `{{ std_head }}` inside `<head>`. Don't hand-write `<title>` on routes that declare
a `title` (that's `titles.multiple`), and don't hand-write canonical/OG at all.

Environment facts agents otherwise have to gamble on:

- **Autoescape is ON** for `*.html.j2` templates (the `.j2` is stripped, `.html` decides).
  `{{ item.title }}` with `Pride & Prejudice` renders `Pride &amp; Prejudice`. `std_head` is pre-escaped and safe.
  Exactly `& < > " '` are escaped; `/` is not, so `href="{{ item.url }}"` stays
  `href="/books/dune/"` in the output. The explicit `| escape` filter escapes the same way.
- **Every other output is plain text**: `.json.j2`, `.js.j2`, `.yaml.j2`, `.txt.j2`,
  `.css.j2` render values as is, with no escaping. In a JSON output, build values with
  `tojson` (`{{ data.books | tojson }}`, `"name": {{ item.name | tojson }}`) rather than
  quoting `"{{ item.name }}"` by hand, which breaks on the first `"` in the data.
- **minijinja's default built-in filters/tests are available** — including `groupby`,
  `selectattr` (truthiness form works: `selectattr("loan")`), `sort(attribute="a.b")`
  with dotted paths, `length`, `join`, `default` — **plus `tojson`** (molde enables
  minijinja's `json` feature; see [JSON-LD](#json-ld) below). Feature-gated in
  minijinja and **not** enabled, so using them is an unknown-filter/syntax error:
  `urlencode` (`urlencode` feature), `{% break %}` / `{% continue %}` (`loop_controls`),
  Unicode-aware case folding in `sort` / `dictsort` (`unicode`; they are
  case-insensitive by default, but fold only ASCII — `É` sorts after `z`). Besides `escape`/`e` (see the engine contract) and, with `locales:`, `t`, `t_html` and
  `loc`, molde registers four filters of its own:
  - `slug` (always): `"Science Fiction" | slug` → `science-fiction`; the same
    function as `{field|slug}` in a route path (see [routes.yaml](#routesyaml)), so a
    link built with it names the page the route made.
  - `year` (always): `"2026-04-29" | year` → `2026`; also takes ISO datetimes. The
    building block for grouping by year: iterate date-sorted items and start a group
    when `b.published | year != loop.previtem.published | year`. Given a number, `null`
    or a missing field (`"published": 1999`) it fails the page with `template.render`,
    naming the filter, the value and the page; `date_format` does the same.
  - `date_format(style=…, pattern=…, locale=…)` (needs `locale`): a strict ISO
    `YYYY-MM-DD` date in the locale's CLDR style. `"2026-11-03"` → es
    `martes, 3 de noviembre de 2026` (`full`) / `3 de noviembre de 2026` (`long`, the
    default) / `3 nov 2026` (`medium`) / `3/11/26` (`short`) / `noviembre de 2026`
    (`month`: month and year). `pattern="EEEE, dd MMMM yyyy"` takes an LDML pattern
    instead, with the fields `G y M L d E c` (`MMMM` is the in-date month form, ru
    `марта`; `LLLL` the stand-alone one, `март`) and `'quoted'` literals. An unknown
    style, an unsupported field (times included) or both `style` and `pattern` are
    render errors. A day that doesn't exist (`2026-02-30`) passes through.
  - `num_format(decimals=…, grouping=…, locale=…)` (needs `locale`): a decimal string
    (`"0.0850"`) or a number. Without `decimals` it keeps the digits as given and only
    localizes them: `"0.0850"` → es `0,0850`, `"1234567.5"` → de `1234567,5` (no
    grouping). `decimals=N` (0–100) formats like ICU's `DECIMAL` / PHP's
    `NumberFormatter`: exactly N fraction digits, half-even rounding, grouping on —
    `"1234.5" | num_format(decimals=2)` → en `1,234.50`, de `1.234,50`, fr `1 234,50`
    (U+202F); `"1234567.891"` → hi `12,34,567.89`; `"-1234.5"` → ar `‎-1,234.50`.
    `grouping=true|false` overrides either default. Digits follow the locale's default
    numbering system (ar-EG `١٬٢٣٤٫٥٠`).
  - Both format in any locale CLDR 48 covers (529, bundled); in a locale without data a
    call is a render error naming the locale, the page and the template. Output matches
    ICU 78 byte-for-byte, invisible marks included ([Locale data](#locale-data)).
  - With `locales:` instead of `locale`, `date_format` and `num_format` use the page's
    locale, and three more exist: `t(key, **kwargs, args=mapping)`, `t_html(key, **kwargs, args=mapping)` and the
    `loc` filter (see [Locales](#locales)).

  `year`, `date_format` and `num_format` are **honest by construction**: input that isn't an ISO date / plain decimal
  string passes through UNCHANGED — they never invent or mangle a value (`"N/D"`
  stays `N/D`).
- **Undefined is lenient**: a missing field renders as empty and `{% if missing %}` is
  false — typos do not error at render time; conformance checks are the net. One level
  deeper does error: `{{ missing.x }}` fails the build (`undefined value`).
- **JSON `null` prints as nothing**: `{{ item.cap }}` with `"cap": null` writes nothing,
  as in Liquid, Nunjucks and Twig. It is still a value, not undefined: `| default("n/a")`
  keeps it (it only replaces undefined), and turning it into a string first
  (`~`, `| string`) spells `None`. To show a placeholder, guard nullable fields with `{% if item.cap is not none %}`: a bare
  `{% if item.cap %}` also hides a real `0`, `""` or `false`, and so does
  `| default("n/a", true)`, which replaces any falsy value, not just `null`.
- **JSON `null` is falsy**: `selectattr("loan")` drops items whose field is `null`.
  But `sort(attribute="loan.due")` on items where the path hits `null`/missing
  does NOT error — those items silently sort FIRST. Always filter nullable fields before
  sorting: `data.books | selectattr("loan") | sort(attribute="loan.due")`.
- **Whitespace**: block lines (`{% for %}`, `{% if %}`) leave blank lines in output —
  there is no trim_blocks default. Use `{%- ... -%}` if you want clean output/diffs;
  it's cosmetic either way.

### Engine contract

molde sets every engine option that changes output explicitly (`filters_env` in
`src/render.rs`), and `tests/engine_contract.rs` pins each row below (`std_head` output
being already safe is covered by `tests/head.rs`, which checks it is escaped exactly
once). A minijinja upgrade or a new feature flag that moves one fails the tests before
it reaches a site. Upgrades only happen through a PR: `Cargo.lock` is committed, CI
builds with `--locked`, and dependabot proposes each new version as its own PR, whose
CI run is the verdict on whether the upgrade changes anything.

| Behaviour | Contract |
|---|---|
| Autoescape | by template name after stripping `.j2` / `.jinja2` / `.jinja`: HTML for `.html`, `.htm`, `.xml`; none for everything else (`.json`, `.js`, `.yaml`, `.txt`, `.css`, `.svg`, bare `.j2`, `.jinja`) |
| HTML escaping | `& < > " '` → `&amp; &lt; &gt; &quot; &#x27;`; `/` is left alone. Values are escaped once, on output: filters (`upper`, `replace`, …) see the raw string |
| `\| safe` | output as is. `std_head` and `tojson` output are already safe |
| `\| e` / `\| escape` | the same five characters as autoescape (`/` stays `/`), in any output type; marks the result safe, so `\| e \| e` escapes once and an already-safe value passes unchanged. undefined and `none` give nothing. Not needed in HTML templates |
| Undefined | lenient: a missing name or key renders empty, is falsy, and loops as empty; `is defined` is `False`; `\| default(x)` replaces it. An attribute **of** an undefined value (`{{ nope.x }}`) is an error |
| `none` (JSON `null`) | a value, not undefined: prints as nothing (every output type), but concatenates as `None` (`~`, `\| string`), is falsy, `is none` / `is defined` are `True`, `none.x` renders empty, `\| length` errors. `\| default(x)` keeps it; `\| default(x, true)` replaces it (and any other falsy value). `{% for x in none %}` loops zero times, but `'x' in none` is an error (`'x' in undefined` is `False`) |
| Booleans | print `True` / `False` (what `index:` patterns compare against) |
| Numbers | integers print as integers (`3`); floats keep a fractional part (`1.0`, `2.0` for `10 / 5`); `/` is float division, `//` integer division; `round` returns a float for a float (`1.5 \| round` → `2.0`) and leaves an integer unchanged (`3 \| round` → `3`); halves round away from zero (`2.5 \| round` → `3.0`, Python gives `2`); `\| int` truncates (`3.7` → `3`) and a non-numeric string (`'x' \| int`) is an error, not `0` |
| `tojson` | object keys sorted (at every depth, data.json objects included), list order kept, compact (`indent=` to pretty-print); `< > & '` → `\u003c \u003e \u0026 \u0027`; `/` and non-ASCII as is; `none` → `null` |
| Trailing newline | the template's final newline is dropped (`"x\n"` → `x`); more than one keeps the rest |
| Whitespace control | `trim_blocks` and `lstrip_blocks` off: a block tag keeps the newline after it and the indentation before it; trim with `{%- -%}` |
| Dict iteration | `{% for k in d %}` and `d \| items` go by key in byte order (`B`, `_`, `a`, `b`), not the order in data.json; `\| dictsort` folds ASCII case (`_`, `a`, `B`, `b`) |
| `sort` | ASCII case-insensitive by default (`A,a,b,B`); `sort(case_sensitive=true)` for byte order (`A,B,a,b`). Non-ASCII is not folded: `['é','É'] \| sort` → `É,é` |
| `set` scope | a `set` inside `{% for %}` is local to the loop body: it neither leaks out nor changes an outer variable of the same name |
| `include` | the included template sees the including template's full context: `set` variables and, inside a loop, the loop variable |
| `import` / `from … import` | macros see the **importing** template's top-level context as it stood at the import, earlier `set`s included (Jinja2 imports "without context" by default). They do **not** see loop or `with` variables, nor a `set` after the import: pass such values as macro arguments |
| Methods | no Python-style methods: `{{ s.upper() }}`, `{{ d.items() }}`, `{{ s.split(',') }}`, `{{ s.startswith('a') }}` are `unknown method` errors; use filters and tests: `\| upper`, `\| items`, `\| split(',')`, `s is startingwith('a')` / `is endingwith`, `'a' in s` |
| Loop filters | `{% for x in xs if cond %}` filters the whole list before the body runs: a `namespace` the body changes (`ns.n = ns.n + 1`) is not seen by `cond`. To stop after the first match, test inside the body: `{% for x in xs %}{% if ns.n < 1 %}…{% set ns.n = ns.n + 1 %}{% endif %}{% endfor %}` |
| Inline `<script>` | pass data with `tojson`, unquoted: `var x = {{ x \| tojson }};` is a JS value that can't close the script. A quoted `"{{ x }}"` gets HTML escaping (`&lt;`), which JavaScript doesn't undo |
| Not enabled | the `urlencode` filter, `{% break %}` / `{% continue %}`, Unicode case folding in `sort` / `dictsort` |

## JSON-LD

Build the structured-data object in the template and serialize it with `tojson`:

```jinja
<script type="application/ld+json">
{{ {
  "@context": "https://schema.org",
  "@type": "Book",
  "name": item.title,
  "author": {"@type": "Person", "name": item.author},
  "url": "https://example.com/books/" ~ item.id ~ "/"
} | tojson }}
</script>
```

- `tojson` output is **safe inside `<script>`** and is not HTML-escaped again (autoescape
  is on, the filter marks its output safe): `<`, `>`, `&` and `'` inside strings become
  `\u003c`, `\u003e`, `\u0026`, `\u0027`, so a data value containing `</script>`
  cannot close the element early. `"` stays `\"` (it's JSON). The block parses back to
  exactly the data you passed.
- Object keys come out **sorted** (minijinja maps are ordered by key). JSON-LD doesn't
  care about key order.
- `tojson(indent=2)` pretty-prints; the default is one line.
- A pre-built object in data.json works the same way: `{{ item.jsonld | tojson }}`.
- Don't put `tojson` output in a double-quoted HTML attribute (the `"` is not escaped);
  use single quotes there: `data-x='{{ obj | tojson }}'`.

`check` verifies what the blocks say, not how they were written:

A block is a `<script>` whose `type` attribute is `application/ld+json` (any case or
quoting, `; charset=…` allowed). Blocks inside HTML comments don't count — browsers and
crawlers skip them too.

- **Always on:** every ld+json block on every HTML page of the build must parse as JSON,
  or `jsonld.invalid` fires with the parser's message and the block's position on the
  page (`block`, counting from 1). Hand-written blocks that interpolate data are the
  usual culprit; `tojson` can't produce one.
- **Per route:** `jsonld: [Dataset]` in routes.yaml makes every page of the route carry
  each listed `@type` somewhere in its blocks, or `jsonld.type_missing` fires naming the
  `missing` types and the ones it `found`. molde doesn't validate against schema.org
  (required properties, value types); it guarantees the types you promised are there.

## Partials & macros

Every file under `templates/` is loadable — only templates a route names become pages,
so shared fragments live alongside page templates. Convention: prefix non-route
templates with `_` (`_header.html.j2`) so a reader can tell them apart; the framework
does not treat the prefix specially.

- `{% include "_header.html.j2" %}` inserts the fragment with the caller's FULL
  context: `data`, `item`, `page`, `std_head`, and any `{% set %}` made before the include —
  the idiom for parameterized partials is `{% set filas = … %}{% include "_tabla.html.j2" %}`.
- `{% import "_util.html.j2" as u %}` + `{% macro %}` give reusable functions.
  Imported macros see the page globals (`data`, `item`, `page`, `std_head`) but NOT the
  caller's `{% set %}` locals — pass those as arguments.
- `{% call u.box("id") %}<p>{{ item.name }}</p>{% endcall %}` is how a macro wraps
  caller-supplied HTML: the macro emits the body via `{{ caller() }}`, and the body is
  normal template text with per-interpolation autoescape. Prefer this over building
  HTML in strings and marking them `| safe` — one forgotten `| safe` ships escaped
  markup to the page, and no conformance check catches that today.
- Autoescape for included/imported files follows the same name rule as pages: `.j2`
  is stripped, the remaining extension decides. Name HTML partials `_x.html.j2` —
  a bare `_x.j2` renders with autoescape OFF.
- Macro output is already escaped when it returns ("safe"): `{{ u.fmt(x) }}` will not
  double-escape.

## URL conventions

- Route `/books/dune/index.html` is served at `/books/dune/`. `page_url` = `base_url` +
  path with a trailing `index.html` stripped. Non-index routes (`/about.html`) keep the
  extension.
- The link checker accepts a link if it resolves to a file directly or as
  `<path>/index.html`, trailing slash or not. Fragments and queries are ignored;
  relative links are resolved from the page's folder (see "What the checks do not see"
  for exactly what is read).

## Output layout

```
out/
├── builds/<nanosecond-id>/   # immutable; one per DISTINCT build
├── current -> builds/<id>    # atomic symlink, only ever swapped on a passing check
└── .molde.lock               # locked while a command writes, so two never write at once
```

- `check` publishes (swaps `current`) **only on verdict pass** — a failing build leaves
  the previous one live. The failing build is still kept under `builds/`, so it can be
  inspected and diffed.
- **Byte-identical rebuilds are deduplicated**: re-running `check` without changes reuses
  the previous build id, so your diff baseline survives re-verification and build dirs
  don't accumulate.
- Every page gets a precompressed `.html.gz` sibling (for nginx `gzip_static`).
- **Nothing deletes builds automatically.** Each distinct build stays until you run
  `molde gc --out O --keep N`, which keeps the N newest and the published one, and
  removes render folders (`builds/.tmp-*`) a crashed run left behind more than an hour
  ago. Run it from cron, or after a long `serve --watch` session.

## The check report

`molde check --site S --out O` prints ONE JSON report and exits 1 on fail:

```json
{
  "verdict": "fail",
  "published": false,
  "build": { "pages": 5, "assets": 1, "ms": 3, "dir": "out/builds/…" },
  "errors": [
    {
      "layer": "output",
      "check": "canonical.missing",
      "page": "about/index.html",
      "expected": "https://example.com/about/",
      "route": "/about/index.html",
      "template": "about.html.j2",
      "hint": "render {{ std_head }} inside <head>"
    }
  ]
}
```

All layers always run — the report is complete, fix everything in one batch. Every
page-anchored error carries `route` + `template` (the fixable source) and a `hint`.
The one exception: the routes layer (`paths.duplicate` / `paths.reserved`) aborts
before rendering (a build that writes fewer pages than it claims is dishonest), so
its report has `build: null` and the other layers don't appear — fix the collision
first. These errors carry `route` + `template` (first offender) like every other
check, plus a `routes` array naming all offenders.

| check | fires when |
|---|---|
| `paths.duplicate` | two routes (or two items of one foreach, or a `paginate.path` and another route) expand to the same output path — nothing is rendered at all (`layer: routes`, `build: null`). Compared with the leading slash trimmed and case-insensitively (case-only pairs collide on macOS/Windows dev filesystems). One error per colliding path: `routes` lists every offender, `route`/`template` carry the first, `pages_colliding` the count |
| `paths.file_and_folder` | one page is written as a file at a path another page needs as a folder (`/docs` and `/docs/intro.html`): only one can exist on disk, so nothing is rendered (`page` = the file, `inside` = the page in the folder; `layer: routes`, `build: null`) |
| `paths.reserved` | with `base_url` set, a route claims `/sitemap.xml` or `/sitemap-<n>.xml` — the framework emits those files from the route manifest and would overwrite the page (same `build: null` refusal) |
| `links.internal` | an internal href resolves to no file in the build (with `base_url`, links written absolute to the site's own address are internal) |
| `titles.missing` | a page has no non-empty `<title>` |
| `titles.duplicate` | two indexable pages share a title (`title`; `duplicate_of` names the first). A page carrying a robots noindex meta is not a search result, so it may share one. With `locales:`, per locale: pages in different locales may share one; static files and `locales: none` pages are compared with every locale |
| `titles.multiple` | a page has more than one `<title>` |
| `sitemap.dangling` / `sitemap.missing_page` | sitemap and page set disagree (`loc` = the sitemap URL with no page, for `dangling`). An HTML page is excused from the sitemap when its route says `index: false` or the file carries a robots noindex meta |
| `sitemap.noindex_listed` | the sitemap lists a page that carries a robots noindex meta (a hand-written meta on an indexable route) |
| `canonical.missing` / `.mismatch` / `.not_in_sitemap` | canonical absent / wrong / not in sitemap (`expected`; `found` for `mismatch`; `url` for `not_in_sitemap`). `missing` and `not_in_sitemap` don't apply to `index: false` pages. URLs are compared as a browser compares them (a default port is no port): the scheme and host in any case, an empty path the same as `/`, the path exactly |
| `og.missing` / `og.mismatch` | titled, indexable route lacks full OG (`property` names the missing tag) / og:url wrong (`found`, `expected`) |
| `canonical.duplicate` / `og.duplicate` | a page carries a second canonical link, or a second `og:url`, `og:title` or `og:type` (`property`); `found` lists every value in page order. Usually `std_head` plus a hand-written tag. The head checks read the markup only: a tag inside a comment, a script, a style or a `<template>` doesn't count (one inside `<noscript>` does) |
| `og.title_mismatch` | a page's `og:title` and its `<title>` say different things, compared as a browser shows them (entities decoded, whitespace collapsed, case ignored); one containing the other passes, so an og:title without the title's "\| Brand" suffix is fine (`found` = the og:title, `expected` = the title) |
| `description.missing` | the route declares `description` but the page's meta description is empty or absent |
| `description.duplicate` | two indexable pages share a meta description (`description`; `duplicate_of` names the first); per locale, like `titles.duplicate` |
| `sitemap.lastmod_invalid` | a sitemap page's `lastmod` rendered something other than `YYYY-MM-DD` / RFC 3339 (`lastmod` carries the value) |
| `jsonld.invalid` | an `application/ld+json` block on an HTML page doesn't parse (`block` = its position on the page from 1, `error` = the parser message). Always on |
| `jsonld.type_missing` | the route declares `jsonld:` and the page's blocks lack one of those `@type`s at any depth (`missing`, `found`) |
| `budget.exceeded` | a file a site.yaml `budgets` rule matches is over a limit (`file`, `match`, `metric` = `bytes` or `gzip_bytes`, `size`, `limit`) |
| `budget.unmatched` | a `budgets` rule matches no file in the build (`match`) |
| `paths.locale_prefix` | with `locales:`, a page in the root namespace (default locale, or `locales: none`) starts with another locale's prefix (`/en/about.html` on an es route) and would sit inside that locale's tree (`layer: routes`, `build: null`). The default's own prefix is allowed unless `prefix_default_locale` is on |
| `paths.invalid_chars` | with `locales:`, an output path has a character outside ASCII letters, digits and `- . _ ~ /` (`char`; `layer: routes`, `build: null`) |
| `paths.root_missing` | with `prefix_default_locale: true`, nothing renders `/index.html` (`layer: routes`, `build: null`) |
| `html.lang` | with `locales:`, a page's `<html lang>` isn't its locale or a more specific tag of it (`en-US` on an `en` page passes; case-insensitive; any attribute quoting; `found`, `expected`). `locales: none` pages are skipped; hreflang stays exact |
| `hreflang.mismatch` | with `locales:` and `base_url`, a page's `<link rel="alternate" hreflang>` set differs from its group (`missing`, `extra`, `wrong_href`, `duplicate` = an hreflang linked twice; x-default included). `locales: none` and noindex pages must carry none |
| `translations.missing` / `.extra` / `.args` | a locale's file lacks (or leaves empty) a default-locale key / has a key the default doesn't / uses different `{placeholders}` (`locale`, `key`; `expected` and `found` for `.args`). `layer: translations` |
| `translations.unknown_key` | a template called `t()` with a key the default locale doesn't define; it rendered empty. One entry per key and template (`key`, `locales`, and `template`, or `source` for a routes.yaml pattern) |
| `html.double_escaped` | text escaped twice shows as `&amp;` (or `&lt;`, …) in the browser, e.g. `Pride &amp;amp; Prejudice` in the HTML: usually data that is already HTML-escaped, or a hand-written `replace("&", "&amp;")`, escaped again by autoescape. One error per page (`found`, `context`, `count`). `<pre>`, `<code>`, `<textarea>`, scripts, styles and comments are not checked |
| `robots.sitemap_missing` | with `base_url`, the build's `robots.txt` (usually a `static/robots.txt`, which replaces molde's) has no `Sitemap:` line naming `<base_url>/sitemap.xml` (`file`, `expected`, `found` = the Sitemap lines it has) |
| `robots.blocks_all` | with `base_url` and at least one indexable page, the build's `robots.txt` shuts every crawler out: the rules for `User-agent: *` (every such group, merged) have `Disallow: /` and no non-empty `Allow` (`file`). Read as RFC 9309 reads it. A site whose pages are all `index: false` may block crawlers |
| `robots.missing` | an `index: false` page lacks `<meta name="robots" content="noindex">` (template doesn't render `{{ std_head }}`) |
| `template.syntax` | a template does not parse (`template`, `line`, `message`; `page`/`route` of the first page that used it). `layer: source`, `build: null` |
| `template.render` | a page failed while rendering: an attribute of an undefined value, a filter error, a localized field printed without `\| loc`, a `date_format` in a locale without data… (`page`, `route`, `template`, `line` when known, `message` with the whole cause). `layer: source`, `build: null` |
| `config.parse` | `routes.yaml`, `site.yaml`, `data.json` or a `translations/*.yaml` file does not parse (`file`, `line`, `message`). `layer: source`, `build: null` |
| `config.invalid` | the files parse but the router rejects them: a `foreach` over a missing collection, an `index` pattern that renders neither `true` nor `false`, a bad locale setting (`message` names the route or value). `layer: source`, `build: null` |

With `locales:` the report also has `pages_by_locale` (`[{route, template, foreach?,
pages: {locale: n}}]` per localized route) and `warnings`, which never fail the check.
Both name a route by its path pattern (on a route with per-locale paths, those paths as
one string), its `template` and, on `foreach` routes, the collection, since several
routes can share one pattern.
`i18n.sparse_locale` fires when a locale has fewer than half the default locale's pages
for a route (`route`, `template`, `foreach`, `locale`, `pages`, `default_pages`) — a data builder that forgot a
locale would otherwise pass, since a missing value is a legitimate "no page". Sites
without `locales:` have neither key.

Source errors (`layer: source`) stop the check where they happen: the report has
`build: null` and that one error, since nothing was built to check. Fix it and run
`check` again. Only the tool itself failing (the out dir cannot be written, a disk
full) prints plain text on stderr instead of a report; both exit 1.

A `date_format` / `num_format` call that renders in a locale without CLDR data is a
`template.render` whose message reads `<filter>: locale '<tag>' has no formatting data —
molde bundles CLDR 48.0.0 for 529 locales …`: format the value in another locale with
`locale=`, or leave it unformatted. A bad `style`, `pattern`, `decimals` or unknown
kwarg is reported the same way.

### What the checks do not see

A passing check means every rule above holds for what the checks read. They read
the files molde wrote with patterns, not a browser, so some things are invisible to
them. Knowing which keeps "verified" honest.

What `links.internal` does read: `href` and `src` in any quoting (double, single or
none), on any element, including attributes that end in them (`data-src`, `xlink:href`), root-relative or relative (resolved from the page's folder, as a browser does), or, with
`base_url` set, written absolute to the site's own address (`https://example.com/x/`,
`http://` or `//example.com/x/` too: the host in any case, the path under `base_url`'s);
every `srcset` candidate; and `url()` and `@import` in `.css` files (resolved from the
stylesheet's folder), in `<style>` blocks and in `style` attributes. Attributes are read
inside tags (and quoted URLs inside `<script>`), never in text or inside another
attribute's value: an escaped code sample (`&lt;a href=&quot;/x&quot;&gt;` in a `<pre>`,
or in a `data-code="…"`) is not a link, while a real link inside `<code>` is. A framework
binding (`:href`, `x-bind:href`, `v-bind:src`) holds code, not a URL, and is not read. Entities and `%`-escapes are decoded first. What the checks do not see:

- **`<base href>`.** Relative links are always resolved from the page's own folder; a
  `<base>` element is ignored.
- **Other URL attributes.** `poster`, `<object data>`, `<form action>`, `<meta
  http-equiv="refresh">`, URLs in `<meta content>` (an `og:image` or `twitter:image` that
  names a missing file passes), URLs inside JSON-LD and URLs a script builds at runtime
  are not checked. Unquoted
  `href=`/`src=` text inside scripts, styles and comments is code, not markup, and is
  skipped (quoted ones are read).
- **No layout, CSS or JavaScript.** A page whose styles hide everything, or whose
  script fails, passes. Nothing renders the page.
- **Content a script adds at runtime** (client-side rendering, a search widget) is
  never seen.
- **Links to other sites** (`https://…`, `//host/…`, `mailto:`) are not fetched. A link
  to the site's own address is not another site: it is checked like a root-relative
  one (see above).
- **The words themselves.** A wrong value in the text, a stale date written by hand or
  a badge on the wrong pages passes: the checks verify structure and links, not
  meaning. Text escaped twice is the one content error they catch
  (`html.double_escaped`); inside `<pre>`, `<code>` and `<textarea>` it is not checked.

## Verify any site

`molde verify DIR` checks a folder of built HTML that any generator wrote: Hugo's
`public/`, Astro's `dist/`, a Next.js static export's `out/`, Eleventy's `_site/`, Zola's
`public/`, or molde's own `out/current`. It reads the files only, prints one JSON report in
`check`'s shape, and exits 1 when there is an error, 0 otherwise. Warnings never fail it.
Nothing is published: a foreign site deploys through its own CI, where the exit code is the
gate.

```sh
molde verify public --base-url https://example.com/
molde verify out --base-url https://user.github.io/repo/ --clean-urls
```

| Flag | Meaning |
|---|---|
| `--base-url URL` | the site's public address, with its base path if it has one: `http(s)://`, a host and an optional path, no query or fragment (anything else exits 2). Turns on the canonical, sitemap, robots.txt and own-address checks; without it they are skipped, and the report suggests the address the pages' canonicals use |
| `--clean-urls` | the host serves `/a` from `a.html` (Netlify, Vercel, Cloudflare Pages, GitHub Pages; not S3 or CloudFront) |
| `--ignore GLOB` | a site path that isn't this build's to check (`/cdn-cgi/**`, `/api/**`), with the glob rules of size budgets. Repeatable |
| `--spa-fallback PATH` | the host serves this file for every unknown page (a single-page app): a link to a page that isn't a file is fine; a missing asset still isn't. The file must be in the build (else exit 2) |
| `--require-canonical` | an indexable page without a canonical is an error (`canonical.missing`) |
| `--og` | require the Open Graph block (`og.missing`) and an og:title that agrees with the title (`og.title_mismatch`) |
| `--sitemap-complete` | an indexable page missing from the sitemap is an error, not a warning |
| `--unique` | duplicate titles and descriptions are errors, not warnings, except where a pager or taxonomy page is involved |
| `--allow-project-root` | verify the folder even though it holds a project file (`package.json`, `hugo.toml`, …): a repository published as is |

**What a file is.** Every `.html` and `.htm` file, in any case, is one of the following,
and each check judges it by that:

| Kind | Recognised by | Held to |
|---|---|---|
| `not_document` | no `<html`, `<head` or doctype in its first 2 KB, after a byte-order mark and leading comments (a verification file, a fragment) | nothing |
| `not_found` | `404.html`, `404/index.html`, `_not-found.html`, `_not-found/index.html` at any depth (a locale's own `fr/404.html`); the root `200.html` and `__spa-fallback.html`; the `--spa-fallback` file unless it is the root `index.html` | its links only, `rel=alternate` links aside (no one reads a 404's head) |
| `redirect` | a meta refresh with a URL (read as the HTML spec reads `content`: `0;url=…`, `0, URL = '…'`, `0;…`; not one inside `<noscript>`), or a canonical to another page of the build (an alias, a pager; an absolute canonical counts only with `--base-url`) | a refresh stub: its links and its target. A page with a canonical elsewhere: also one title, escaping, JSON-LD; and many such pages naming one page is `canonical.shared` |
| `noindex` | a robots noindex meta | links and `titles.multiple` (a title isn't required: a frame's content, mdBook's `toc.html`); kept out of the sitemap and of uniqueness |
| `indexable` | everything else | every check |

The report's `pages` counts each kind.

**Checks.** `check`'s ids where the meaning is the same, new ids where verify judges what
`check` can't see:

| check | when | fires when |
|---|---|---|
| `links.internal` | always | a link or asset (`href`, `src`, `srcset`, CSS `url()` and `@import`, read inside tags as `check` reads them; `<base>` and canonical hrefs are not links) names no file of the build. The same missing page linked from three or more pages is one error, with `target`, `count` and `pages` |
| `links.outside_base` | with a base path | a link leaves the base path (`/posts/x/` on a site at `/repo/`): it works on disk and 404s in production |
| `links.clean_urls` | without `--clean-urls` | most missing pages (links, canonicals, redirects and sitemap entries) would resolve as `a.html`: one error for all of them (`count`, `by_check`, `examples`) saying to try `--clean-urls` |
| `titles.missing`, `titles.multiple` | documents | as in `check` |
| `titles.duplicate`, `description.duplicate` | indexable documents | warnings; errors with `--unique`. A value more than 10 pages share with the same first holder (a theme's default description) is one entry, with `count` and five `examples` |
| `html.double_escaped`, `jsonld.invalid` | documents | as in `check` |
| `canonical.duplicate`, `og.duplicate` | documents | as in `check` |
| `canonical.off_host` | with `--base-url` | the canonical is on another host, or outside the base path |
| `canonical.target_missing` | with `--base-url` | the canonical names a page this build doesn't have |
| `canonical.target_noindex` | with `--base-url`, and on redirects always | the canonical names a page that asks not to be indexed |
| `canonical.missing` | with `--require-canonical` | an indexable page has none |
| `canonical.shared` | always (absolute canonicals with `--base-url`) | three or more pages (pagers and taxonomy pages aside) name one other page as their canonical, usually a canonical hard-coded in a template: one error (`target`, `count`, `examples`). One page naming another can't be told from an alias, and passes |
| `redirect.target_missing` | always (an absolute target with `--base-url`) | a meta refresh or an alias leads to a page this build doesn't have |
| `sitemap.dangling`, `sitemap.noindex_listed` | with a sitemap | as in `check` |
| `sitemap.non_canonical_listed` | with a sitemap | the sitemap lists a redirect: list the page it points to |
| `sitemap.missing_page` | with a sitemap | a warning; an error with `--sitemap-complete` |
| `sitemap.not_found` | with `--base-url` | a warning: no sitemap found |
| `robots.sitemap_missing`, `robots.blocks_all` | with `--base-url` and a `robots.txt` | robots.txt names a sitemap on this site that the build doesn't have (an error; one on another host isn't judged), or has no `Sitemap:` line while the build has a sitemap (a warning: valid, and Hugo's default) / shuts every crawler out of a site with pages to index |
| `og.missing`, `og.title_mismatch` | with `--og` | |
| `verify.base_url_mismatch` | with `--base-url`, or without it when the pages claim a base path | the pages claim another address (their canonicals' host and base path), and it explains most of the errors: one error naming it (`found`; `given` is null without `--base-url`), plus the errors and warnings that remain under both addresses |
| `verify.no_pages` | always | the folder has no HTML documents (an empty build) |
| `verify.project_root` | always | the folder is a project (it holds `package.json`, `hugo.toml`, …), not its output: the message names the output folder |

**The sitemap** is the first that exists: one robots.txt names, `sitemap.xml`,
`sitemap-index.xml` or `sitemap_index.xml`. A sitemap index is followed into its children,
and `<loc>`s are read namespaced or in CDATA; an extension's (`<image:loc>`, `<video:…>`,
`<news:…>`, `<xhtml:…>`) names no page and is skipped.

**The report** has `check`'s fields (`verdict`, `published`, always `false`, `build`,
`errors`), plus:
- `"command": "verify"`;
- `pages`, the counts by kind;
- `warnings`;
- `skipped`, each family that didn't run and why (`[{"checks", "reason", "suggestion"?}]`).

**URLs** resolve as a static host serves them: `/a/` and `/a` from `a/index.html`
(S3 website endpoints, GitHub Pages, Netlify; a CloudFront distribution in front of an
S3 REST origin returns 403 for `/a`, and serves `/a/` only with a rewrite), `/a` from
`a.html` only with `--clean-urls`, file names exactly (`%2F` is part of a name, not a
`/`). `<base href>` moves relative links: to a folder of this site, or out of verify's
reach (another host, or an absolute base without `--base-url`), where they are not
judged; the first `<base>` with an `href` counts, as in browsers, and a base that is only
a `#fragment` or `?query` moves nothing. The base path is compared decoded
(`/caf%C3%A9/` is `/café/`). A symlink inside the build is served as the file it names;
one that leaves the build is not part of it.

**Where verify's report differs from check's**, for a script that reads both: `build` is
`{dir, files, ms}` (pages are counted by kind in `pages`), or `null` when verify refused
the folder (`verify.project_root`, a missing folder); `links.internal` groups a target
three or more pages link to into one error; duplicates are warnings unless `--unique`;
and `robots.sitemap_missing` asks for any sitemap of the build, not
`<base_url>/sitemap.xml`.

Errors carry `page` but no `route` or `template`: verify can't know which source made a
page, and only molde's own `check` can. That is the difference between the two: run
`check` on a molde site, and `verify` on anyone's.

**What verify doesn't do:** fetch anything, run JavaScript, judge the text, check `#fragment`
links against the target page's ids, check locales and size budgets, or follow `--base-url` guesses. Everything in [what the checks do not
see](#what-the-checks-do-not-see) applies too, except that verify honours `<base href>`.

## Security model

molde reads a site folder and writes an out folder. Everything in between is local:
**molde makes no network requests and sends nothing anywhere** (no telemetry, no update
checks). The two commands that listen (`serve`, `events`) bind to `127.0.0.1` only.

**The site folder is trusted code.** Templates, `routes.yaml`, `site.yaml` and
`translations/` are executed or obeyed as written, like a Makefile; only build a site
whose files you would run. `data.json` is treated as untrusted input to the router: a
`{field}` value that is empty or contains `/`, `\` or `..` fails the build instead of
writing outside the build folder, and values printed in `.html`, `.htm` and `.xml`
templates are HTML-escaped by autoescape. Other templates (`.txt`, `.json`) print
values as they are: use `tojson` for JSON.

**What `molde mcp` lets an agent do.** The agent can only call these tools; none takes
a file-system path (`path` arguments are URL paths, matched against the routes):

| tool | reads | writes | arguments from the agent |
|---|---|---|---|
| `check` | `--site` | a new build under `--out/builds/`, `--out/current` on pass, the `--out/.molde.lock` lock file | none |
| `list_routes` | `--site` | nothing | none |
| `explain_route` | `--site` | nothing | a URL path, matched against the routes |
| `render_page` | `--site` | nothing | a URL path, matched against the routes |
| `diff_build` | `--out/builds/` | nothing | two build ids, accepted only if `out/builds` lists them; `full` (a boolean) |

So an agent steered by untrusted content (a web page it read, a poisoned issue) can make
molde rebuild and publish the site as its files currently stand, and read the site and
its builds; it cannot make molde read or write anything else. What it can do to the site
files themselves depends on the agent's own tools, not on molde.

**`serve` and `events` are development tools.** They listen on `127.0.0.1` only, have
no authentication and no TLS: keep them on localhost, never behind a public port or a
tunnel. Within that, they are safe to leave running:

- each connection has its own thread and a 5-second timeout, so an idle or slow client
  cannot hold the server up;
- they answer only requests addressed to this machine (`Host: localhost`, `127.0.0.1`
  or `[::1]` with their port; anything else gets 421), so a web page cannot reach them
  through DNS rebinding;
- `events` refuses `POST /hint` from a web page served elsewhere (an `Origin` header that
  is not this machine gets 403); curl and scripts send none and are accepted. A hint
  never reads the request body and only triggers the same verified check as running
  `molde check`, at most once per `--min-interval-secs`. `GET /status` returns the last
  check report;
- `serve` decodes `%`-escapes, then refuses `..`, and serves whatever the folder it is
  given contains, following symlinks inside it.

**Templates and symlinks.** Templates are part of the trusted site folder: a symlink
in `templates/` is followed, so its target's contents can end up in pages. Symlinks in
`static/` are skipped. Review a site folder from someone else before building it.

Report a vulnerability as described in [`SECURITY.md`](../SECURITY.md).

## Compatibility

molde is 0.x. Its public surface is **the CLI, the site format (`routes.yaml`,
`site.yaml`, `data.json`, `translations/`, the template contract) and the check
report**. The Rust library API is not part of it: it can change in any release.

What a version number promises:

| release | may change | never changes |
|---|---|---|
| patch (`0.1.0` → `0.1.1`) | bug fixes: a check that wrongly failed a correct site stops failing, a crash is fixed | the report's shape, check ids, CLI flags, site keys; no new or stricter check, so a site that passed still passes |
| minor (`0.1` → `0.2`) | the surface, in the documented ways below; new checks; output bytes | what is not listed below |

From `1.0` on, molde follows SemVer: breaking changes only in a major release.

Every change to the surface is listed in the changelog under the release that makes
it, with what to do.

**The check report.** A program may rely on:

- `verdict` (`pass` / `fail`), `published`, `build` (an object, or `null` when the
  check stopped before building), `errors` (an array).
- In each error: `layer`, `check`, and when present `page`, `route`, `template`,
  `file`, `line`, `message`, `hint`.
- **Check ids** (`links.internal`, `template.syntax`, …) keep their name and meaning
  within a minor series. A renamed or removed id is a minor-release change.
- New fields can appear in any release. New check ids and layers arrive in minor
  releases. Ignore what you don't know rather than rejecting it.
- `hint` and `message` texts are for reading, not matching: they can change in any
  release.

**Exit codes.** `0` when the command succeeded (for `check`, a passing verdict), `1`
when it failed (a failing verdict, a source error, the tool failing), `2` when the
command line itself is wrong (an unknown flag, a missing argument).

**Site files.** Any release can add keys, route options and template variables.
Removing or renaming one, or changing what an existing one means, waits for a minor
release and is in the changelog. Unknown keys fail the build with the key named, so
a removed key is never ignored silently.

**Output bytes.** The same site and the same molde produce the same bytes. A minor
release can change output (escaping, `std_head`, the sitemap); a patch release only
when the old output was a bug. `molde diff` against a build from the previous version
shows exactly what changed before you deploy.

## Commands

```sh
molde check  --site S --out O          # build + all checks -> one JSON report; publish on pass
molde build  --site S --out O          # build + publish, no checks
molde routes --site S                  # every route and its expanded pages, as JSON
molde render /books/dune/ --site S      # one page from live source to stdout — no build
molde diff   --out O [--full]          # last two builds; --full = changed lines per page
             [--from ID --to ID]       #   any two builds, by folder name under builds/
molde gc     --out O [--keep 10]       # delete all but the newest N builds (+ the published one)
molde serve  --out O --port 8080       # dev server on out/current
             [--watch --site S]        #   + rerun check on site changes (publish on pass)
             [--reload]                #   + auto-refresh open browser tabs on publish
molde events --site S --out O          # POST /hint -> verified rebuild (body ignored)
             [--port 8081 --min-interval-secs 30]
molde mcp    --site S --out O          # MCP on stdio: check, list_routes, explain_route,
                                       #   render_page (live preview), diff_build
molde guide                            # docs/agents.md: a short guide to this reference
molde verify DIR [--base-url URL]      # check any generator's built output (see Verify any site)
```

`diff` prints `from`, `to`, `added`, `removed`, `unchanged` and `changed` (per page its
`first_diff`, or with `--full` its `diff` lines and `diff_truncated`). `gc` prints `kept`,
`removed` and `abandoned_removed` (left-over render folders).

`routes` prints the JSON the MCP `list_routes` tool returns: per route its template,
patterns, page count, noindex count and example paths, plus `total_pages`.

`serve --watch` runs one verified check at startup, in the background: the last good
build (`out/current`) is served meanwhile, and with no build yet every request gets a
short "first build in progress" page (status 503) that reloads itself until one
publishes, or says the check failed. Then it polls the site dir
(dotfiles and the out dir excluded, so `--out` inside the site dir is safe) and
reruns `check` when it settles. A failing check never publishes — the browser
keeps the last good build. `--reload` (requires `--watch`) injects a polling
snippet into served HTML at serve time only: build artifacts stay
byte-deterministic, and dedup means an unchanged rebuild never refreshes tabs.
A request that matches no file gets the nearest `404.html` up its path as its body
(status 404): `/de/missing/` gets `/de/404.html` when the build has one, else
`/404.html`, else a plain "not found". That is how Cloudflare Pages serves misses, and
how a prefixed locale shows its own 404 page. nginx `error_page`, S3/CloudFront and
Netlify serve the root `/404.html` unless configured per prefix.

`diff` covers every non-`.gz` file (pages **and** sitemap.xml — so its file count can be
`pages + 1`). Detail is capped: 20 pages with detail, 200 diff lines per page. With
`--full`, lines equal at the start and end of a page are skipped before comparing; a
changed region too large to minimise (above about 16 million line pairs) is listed as
its old lines removed, then its new lines added, marked `"diff_minimal": false`.

**Diff baseline ritual: run `check` once BEFORE you start editing.** `diff` compares
builds, so the pre-edit build is your baseline — if you edit first, the pre-edit state
was never built and can't be diffed against. Dedup guarantees that baseline `check`
costs nothing if the site is unchanged, and that later re-verification never disturbs
the baseline.

`diff` compares the two newest builds, **failed ones included**: edit, fail, fix, pass,
and the default diff shows only the fix. To see everything since your baseline, pass it
with `--from`: build ids are timestamps, so `ls out/builds` lists them oldest first, and
`readlink out/current` names the live one. With fewer than two builds, `diff` exits 1
with `no builds yet` or `nothing to diff against`.
