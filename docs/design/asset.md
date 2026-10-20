# `asset()`: content-hashed URLs for static files (issue #22)

**Status: draft (20 Oct 2026), not reviewed, no code.** Written to decide whether and how
to build #22; section 1 re-checks the issue's premise against the sites as they are now.

## 1. Why, re-checked

The issue (written before 0.1.0) counted three Python steps on dividendpedia's CI,
`bundle_css.py`, `fingerprint.py` and `check_assets.py`, about 5 minutes together. Today
only `check_assets.py` is left:

- **Bundling went away** when the site moved to one stylesheet per page (its PR #42, "one
  style sheet per page instead of 33").
- **Fingerprinting went away** with the CloudFront move: each deploy uploads a new
  `releases/<id>/`, and a CloudFront Function maps every URL into the live release, so the
  CDN's cache keys change with each release and nothing needs a `?v=`. Browsers get
  `max-age=300`: they keep a file for 5 minutes, then revalidate it (a 304 round trip per
  asset on a repeat visit).
- **`check_assets.py`** (≈2 min) checks that every root-relative or own-domain reference
  in HTML, CSS, JS and JSON names a file of the build, case included. molde check now
  covers most of it: `links.internal` reads `href`, `src`, `data-src`, `srcset` and CSS
  `url()` exactly (#23), absolute own-address links too (B3). It does not read `poster`
  or paths inside JSON or JS files.

So the "delete 5 minutes of CI" measure of done no longer holds. What `asset()` still
gives:

1. **Year-long browser caching** of CSS, JS, fonts and images (`immutable`), without the
   5-minute revalidations. A real speed gain on repeat visits, and the one thing the
   current setup can't do: a URL that never changes its content is the only safe way to
   tell a browser "keep this".
2. **A reference to a missing asset fails the build** where it is written (template and
   page named), not after the build.
3. **The verified output is the shipped output**, for any site that fingerprints today.

That is worth having, but it is no longer urgent: priority "design first, build when a
site asks for long caching", below the 0.2 items that have a user now.

## 2. What it is

A template function:

```jinja
<link rel="stylesheet" href="{{ asset('/assets/css/site.css') }}">
<script src="{{ asset('/assets/js/header.js') }}" defer></script>
```

renders `/assets/css/site.52dcd6fbee.css` and `/assets/js/header.0b41f9c2d7.js`, and the
build contains those files.

| Decision | Choice | Why |
|---|---|---|
| Hash in the name or in a query | **the name**: `site.<hash>.css` | a query (`?v=`) is dropped from the cache key by CloudFront's default policy and by some CDNs, so it caches stale files there; a name changes the key everywhere. Every bundler does this |
| The original file | **kept** at its own path | references written without `asset()` (a third-party snippet, a hand-written page in `static/`) keep working; the cost is one extra copy of each hashed file |
| Hash | first 10 hex characters of the SHA-256 of the bytes as written to the build | 40 bits is far beyond the number of versions one path will have; SHA-256 is also what the build manifest (#18) lists |
| What it accepts | a root-relative path to a file under `static/` | route outputs (a `.css.j2` route) are rendered in parallel with the pages that would reference them; supporting them needs a render order. Later, if asked |
| Missing file | a build error, `asset.missing` (`layer: source`, with `template`, `page`, `asset`), exact case | the template is wrong, and the page would ship a 404. It stops the build like `template.render`, so the message names the template |
| Base path | the result is root-relative, exactly as `page.path` is | one rule for every path molde hands a template |

## 3. Stylesheets that reference other files

A stylesheet's `url()` and `@import` name fonts and images. If `site.css` is hashed but
the font it loads isn't, the font gets the 5-minute rule. When a stylesheet is hashed,
molde rewrites its local `url()` and `@import` references to the referenced files' hashed
names before hashing it. So the stylesheet's hash covers its fonts: a new font gives a
new stylesheet name.

- **Order:** the files form a graph (sheet → font, sheet → sheet via `@import`). They are
  hashed leaves first. A cycle (`a.css` imports `b.css`, which imports `a.css`) is an
  `asset.cycle` error naming the files.
- **A `url()` to a missing file** inside a hashed sheet is `asset.missing` with `file`.
  Today it is `links.internal` after the build, and that still runs on the original copy.
- **Only hashed sheets are rewritten.** `static/` files nothing calls `asset()` on are
  copied byte for byte, as now.

## 4. How it fits the build

- **Once per file:** the first `asset()` call for a path hashes it (and its graph);
  later calls read a shared map. On 43,000 pages that reference the same five files,
  that is five hashes. The map is filled under a lock, and the result is deterministic,
  because it depends only on the bytes.
- **Writing:** `copy_static` writes the hashed copies (and their `.gz` siblings) next to
  the originals.
- **Dedup and `diff`:** unchanged. A changed stylesheet shows as a new file and changed
  references in every page, which is the truth.
- **`serve`:** nothing new. The files exist.
- **`check`:** `links.internal` already proves every hashed reference resolves. The
  manifest (#18) marks the hashed files `immutable: true`, so a deploy script can give
  them `Cache-Control: public, max-age=31536000, immutable` and everything else its
  current rule. molde never sets headers (the issue's out-of-scope stands).

## 5. Out of scope

- Bundling several sheets into one (`css_bundle([...])`). The one site that wanted it
  already has one sheet per page; and bundling is where minification, ordering and
  source maps start, which is a bundler's job.
- Hashing route outputs (section 2), images referenced from data.json, and rewriting
  references inside JS.
- An `asset.unhashed` check ("a local CSS file referenced without `asset()`"). It may be
  useful once a site opts in to long caching, as a site.yaml switch; not before.

## 6. Tests that must exist before it ships

- Byte-identical builds for a site that never calls `asset()` (the golden tests).
- The hash changes with the bytes and only with them; the same input gives the same name
  across machines (path separators, line endings untouched).
- A stylesheet whose font changes gets a new name, and the old name is gone from the new
  build.
- `asset.missing` on a wrong case (`/Assets/site.css`) on a case-insensitive disk (macOS
  CI).
- `asset.cycle`.
- 43k-page timing within noise of today (one hash per file, not per call).

## 7. Open questions for the maintainer

1. **Build it now or when a site needs it?** Recommendation: when a site wants long
   caching. dividendpedia's assets revalidate every 5 minutes today, which costs a 304
   round trip each, not a download.
2. **`check_assets.py`:** separately from `asset()`, `links.internal` could read `poster`
   and `<object data>` (cheap, in the same pass). That would leave `check_assets.py` only
   the JSON and JS paths of the WordPress-era theme files, which the WordPress removal
   should retire anyway. Worth a small issue of its own.
