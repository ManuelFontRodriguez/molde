# web-standards: the `<head>` owner

This document records the reasoning behind molde's `<head>` ownership: canonical URLs, Open
Graph, descriptions, indexability and sitemap `lastmod`, and the checks that keep them
correct. For usage, see [docs/reference.md](../reference.md). The multilingual extension
(hreflang, localized metadata) is designed in [i18n.md](i18n.md).

Decisions it rests on: an explicit `{{ std_head }}` slot in the template; canonical and Open
Graph ship first; multilingual support comes later and only when a site opts in. (An early
sketch prefixed every locale in the URL; the final design keeps the default locale at the
root, see [i18n.md](i18n.md).) Review findings S1–S4 and A1–A5 are folded in below.

## What this is

One plugin owns everything that goes into `<head>` beyond what the template author writes:
canonical, Open Graph, descriptions, robots and hreflang (with i18n). JSON-LD stays in
the template (built with `tojson`); molde only checks it. Single owner = single internal
ordering = stage-level purity with no plugin DAG (granularity was the bug, not the model).

Enablement stays the pattern the sitemap seeded: **`base_url` in site.yaml turns the
capability on together with the checks that keep it correct.** No `base_url`, no
web-standards, no checks. (A `Plugin` trait, to be extracted once a second capability
existed, was later dropped; see the end of this document.)

## Canonical + OG via `{{ std_head }}`

### Mechanism

The framework computes a per-page head block, a pure function of
`(site config, route, page path, item)`, and passes it into the template context as
`std_head` (a pre-escaped safe string). Templates render it explicitly inside `<head>`:

```html
<head><meta charset="utf-8">{{ std_head }}</head>
```

No post-render mutation: what the template says is what the page is. The block itself is
rendered through minijinja with autoescape on, with no hand-rolled escaping anywhere.

### Metadata is route data

`routes.yaml` grows one optional key: `title`, a minijinja pattern evaluated per page.
This extends "the router is data" to page metadata: titles become queryable
(`list_routes`), not buried in templates:

```yaml
routes:
  - path: /books/{id}/index.html
    template: book.html.j2
    foreach: books
    title: "{{ item.title }} by {{ item.author }}"
```

`std_head` then contains, in fixed order (OG is **all-or-nothing** (S2): a
fragmentary OG block is worse than none, parsers render garbage cards from it):

| Tag | Source | Emitted when |
|---|---|---|
| `<title>` | route `title` pattern | route has `title` |
| `<link rel="canonical">` | `page_url()` | always (capability on), except `index: false` pages, which get `<meta name="robots" content="noindex">` instead of canonical + OG (see below) |
| `<meta property="og:title">` | same as `<title>` | route has `title` |
| `<meta property="og:url">` | same as canonical | route has `title` |
| `<meta property="og:type">` | `"website"` (fixed for now) | route has `title` |

The OG block is knowingly below the OG spec minimum (`og:image` is out of scope): honesty
over ambition. A route without `title` keeps its hand-written `<title>` in the template, so
migration is per-route, never big-bang.

**One URL function (S4):** `page_url(base_url, path)` is the single place a page
path becomes a public URL. Sitemap emission, canonical, `og:url`, and every check derive
from it, so they cannot drift; a canonical↔sitemap membership check enforces the invariant
instead of hoping for it.

**Single-escape render path (S3):** the title pattern renders as *plain text*
(no autoescape), is passed as an ordinary string into the std_head template (`.html`-named
→ engine escapes exactly once, element and attribute contexts alike), and only the finished
block is marked safe for the page template. The demo fixture includes a name with `&`
(`AT&T Inc`) so the goldens permanently pin single-escaping.

### Checks that ship with it (on when `base_url` is set)

| Check | Fails when |
|---|---|
| `canonical.missing` | a page has no canonical tag (template forgot `{{ std_head }}`, or overrode it) |
| `canonical.mismatch` | canonical present but ≠ the page's own URL (hand-written drift) |
| `canonical.not_in_sitemap` | canonical URL absent from sitemap.xml (S4 invariant, enforced) |
| `og.missing` | a page from a titled route lacks any of og:title / og:url / og:type (S1) |
| `og.mismatch` | og:url present but ≠ the page's canonical URL (S1) |
| `titles.multiple` | more than one `<title>` in a page (route `title` + leftover template title) |
| `titles.missing` / `titles.duplicate` | as today (existing checks keep running) |

`canonical.missing` + `og.missing` together are the slot check: a page that never rendered
`std_head` cannot pass both by accident (hand-writing a correct canonical *and* a full,
consistent OG block to bypass the slot is no longer an accident, S1).

Check-parsing robustness (A1/A2): tag matching is attribute-order-independent
(`<link href=".." rel="canonical">` is valid HTML), and extracted URLs are entity-unescaped
before comparison so `&` in a URL can't false-positive.

### Indexability and non-HTML outputs

Some pages must exist but must not be indexed: a 404, thin pages below a quality gate,
pages kept for users but not for search. That is route data too:

```yaml
- path: /companies/{slug}/index.html
  template: company.html.j2
  foreach: companies
  title: "{{ item.name }}"
  index: "{{ item.indexable }}"   # or a plain bool; default true
```

- `index` is resolved **at route expansion**, once, into `Page::index`. The sitemap, the
  head block, every check and the MCP introspection read that one value, the same way
  `page_url` keeps URLs from drifting. A pattern that renders anything but `true`/`false`
  is a build error naming the route and page: an indexability typo must not silently
  publish (or hide) thousands of pages.
- An `index: false` page is rendered and linkable, but it is **not in the sitemap**, and
  `std_head` emits `<title>` + `<meta name="robots" content="noindex">`. It has no
  canonical (a noindex page has no URL to consolidate onto) and no OG (all-or-nothing:
  the block without `og:url` would be fragmentary).
- Checks: `robots.missing` fires when an `index: false` page lacks the meta. For those
  pages `sitemap.missing_page`, `canonical.missing`, `canonical.not_in_sitemap` and
  `og.missing` don't apply. The reverse contradiction (a page carrying a hand-written
  noindex meta but listed in the sitemap) is `sitemap.noindex_listed`.
- **Only `.html` paths are web pages.** Other route outputs (`/llms.txt`, `/feed.xml`)
  have no `<head>`: `std_head` renders empty, they are never in the sitemap, and the
  head/title checks skip them. They remain build files for `links.internal` and
  `paths.duplicate`.
- **`static/*.html`** is outside the router, so it's never in the sitemap. The sitemap
  check accepts an unlisted HTML file only if it carries a robots noindex meta. That lets
  a hand-written static 404 pass while an accidentally-static content page still fails
  `sitemap.missing_page`. The rule reads the file, not a config list, so the page's own
  markup is the single source of its indexability.
- Serving a real 404 *status* for `/404.html` is host config, not the build's job.

### Description and lastmod

Two more route patterns, same shape as `title`:

- `description` renders per page as plain text in `std_head` (escaped once, trimmed).
  It becomes `<meta name="description">`, plus `og:description` inside the OG block when
  the block is emitted. Checks: `description.missing` (declared, rendered empty) and
  `description.duplicate` (indexable pages only; noindex pages may share one).
- `lastmod` is manifest data, like `index`: it's resolved at route expansion into
  `Page::lastmod` and emitted as the sitemap entry's `<lastmod>`. Empty means omitted.
  Otherwise it must be `YYYY-MM-DD` or RFC 3339; anything else is
  `sitemap.lastmod_invalid`, and the value is left out rather than published wrong.
  **The build time is never a source**, so a rebuild with unchanged data stays
  byte-identical and dedups.

Table additions to the std_head block:

| Tag | Source | Emitted when |
|---|---|---|
| `<meta name="description">` | route `description` pattern | route has `description` and it renders non-empty |
| `<meta property="og:description">` | same | the OG block is emitted and the description is non-empty |

### Honest limits (recorded, not fixed)

- **Title patterns make conformance data-dependent** (A3): two snapshot items rendering
  identical titles fail `titles.duplicate` and the previous build stays live, consistent
  with stale-beats-torn; the fix is a more discriminating pattern (the demo's
  `title by author` already is).
- **Pattern typos degrade softly** (A4): a misspelled field renders as empty string and
  surfaces as `titles.missing` downstream rather than "route 2 references unknown field".
  A strict-undefined pattern environment is a candidate improvement, not a blocker.
- **No URL-encoding** (A5): a path value with a space now fails with `config.invalid`
  (or use `{field|slug}`). Letters outside ASCII are accepted and written as is, not
  percent-encoded, in the sitemap and the canonical URL; with `locales:`,
  `paths.invalid_chars` refuses them.

### Explicitly out of scope

- `og:description` / meta description were out at first and shipped afterwards (see
  "Description and lastmod"). Length bounds are not enforced; only `description.missing` /
  `description.duplicate` are.
- `og:type` configurability, images, twitter cards.
- hreflang was deferred until the i18n design existed; see [i18n.md](i18n.md). JSON-LD
  is built by templates and checked (`jsonld.*`), not emitted.

## The multilingual sketch (superseded)

This section is kept for the reasoning behind the constraints. The multilingual design
itself is in [i18n.md](i18n.md); build from that, not from this.

Constraints to honor:

1. **Opt-in or nonexistent.** No `locales:` key in site.yaml → there is no locale
   dimension: no prefixes, no `t()`, no hreflang, resolver is identity. Single-language
   properties never pay for i18n. The principle is "keep the resolver as easy as possible;
   multi-language is not always needed", enforced structurally.
2. ~~**When enabled: all locales prefixed, perfectly symmetric.** `locales: [es, en]` →
   `pages = routes × locales`, every URL is `/{locale}{route_path}`, root `/` redirects
   or serves a chooser. No default-locale-at-root special case, so the resolver stays one
   rule.~~ Superseded: the default locale stays at the root, and prefix-all is an opt-in.
3. hreflang then becomes a pure query over the expanded route manifest (the cross-product
   is already data), and its consistency check is a set comparison, not a crawl.

Open questions at the time: translation file shape (`translations/es.yaml`), `t()` in
templates vs pre-resolved context, key-coverage check granularity, and where localized
JSON-LD strings come from. Plus a contradiction found in review to resolve: with all
locales prefixed there is no root `index.html`, so `serve`'s `/` fallback 404s, any
`href="/"` fails `check_links`, and "root redirects" is impossible in pure static
output. The static chooser page is probably the answer, but that's a design decision.
Each question got its own adversarial review before code (see the risks table in
[i18n.md](i18n.md)).

## Interface extraction (not pursued)

The plan was to extract a shared stage interface, and a `plugins:` map in site.yaml,
once two capabilities existed. It was dropped: every capability stays built in, with its
checks, and plugins are not planned (see [ROADMAP.md](../../ROADMAP.md)).
