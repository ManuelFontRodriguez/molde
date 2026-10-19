# `molde verify`: the check for any static site's output

**Status: reviewed once (19 Oct 2026); it failed, and v2 is to be written before any code.**
Review 1 is summarised at the end. It built every generator's default starter, ran a
prototype of this design on them, and found false failures on all five. The sections
below are v1 as reviewed.

## 1. Why

`molde check` builds a molde site and checks the result. Its output-layer checks (links,
titles, the sitemap, canonical and Open Graph, escaping, JSON-LD, robots.txt) read only
the built files, and most people's sites aren't built by molde. The agent-cost
benchmark (docs/bench/agent-cost, round 1) found that what an agent pays to use molde
is mostly learning the generator, which the model has never seen. A checker costs
almost nothing to learn: one command, one JSON report. `molde verify` offers the check
without the generator. molde's own generator stays the path where every error names
its route and template.

### How this differs from a linter

| | A linter (ESLint, an HTML validator) | `molde verify` |
|---|---|---|
| **What it reads** | source files, or one page at a time | the built site, the folder that is deployed, all of it at once |
| **What it finds** | code patterns and style in one file | properties of the whole build: a link to a page that doesn't exist, two pages with one title, a sitemap that disagrees with the pages, a canonical pointing elsewhere, `AT&amp;T` on screen, a robots.txt that blocks every crawler |
| **Opinions** | many, configurable | almost none: what is wrong for a browser or a search engine, whatever the style |
| **Knows the tools** | yes, it parses one language | no: Hugo, Astro, a script or molde, the HTML is the same |
| **Where errors point** | file and line of the code | the page in the output; with molde's own generator (`molde check`), also the route and template |

Many site bugs exist in no source file:
- **A link breaks because the data changed** (a renamed slug, a value with an accent), while every template is correct.
- **Two pages share a title** because two data items share a name.

Only the output shows them. That is also why the check can act as a gate: it is closer to
testing the release than to linting the code. The trade-off is provenance. On a foreign
generator, verify names the page but not the template or content file that made it. That gap is what molde's own generator closes. The two kinds of tool are complementary.

## 2. Goals and non-goals

**Goals.**
- `molde verify <dir>` checks a folder of built HTML (Hugo's `public/`, Astro's `dist/`,
  a Next.js static export's `out/`, Eleventy's `_site/`, Zola's `public/`, anything) and
  prints the same JSON report shape as `molde check`.
- **No false failures on correct output of the common generators** in their default
  configurations. A verifier that fails a correct Hugo site on day one is uninstalled.
- **The same check ids and meaning** as `molde check` wherever a check runs in both, so
  an agent or script that reads one reads the other.
- **Fast:** the same parallel engine as `check` (1,901 pages in under half a second).
- **Optional publish gate:** like `check`, publish only a passing build (§6).

**Non-goals (v1).**
- Provenance: which template or content file made a page. Foreign generators don't record it.
- Locale checks (`hreflang.mismatch`, `html.lang`), size budgets, `jsonld.type_missing`
  and `description.missing`. These depend on molde's site model; some could come later
  through a small config file (§9).
- Fetching other sites, rendering, running JavaScript.
- Fixing anything.

## 3. Command line

```
molde verify <DIR> [--base-url URL] [--out O] [--og] [--format json]
```

- `<DIR>`: the built site, read only.
- `--base-url`: the site's public address, as in `site.yaml`. It turns on the checks that
  need one: the sitemap, canonical and `og:url`, robots.txt, and absolute links to the
  site's own address. Without it they are skipped and the report says so (§5).
  - **Not inferred** from the sitemap or a canonical tag: a wrong guess would make every
    page fail, and a broken sitemap would hide itself.
- `--out O`: the publish gate (§6). Without it, verify only reads.
- `--og`: also require a full Open Graph block (`og.missing`) on every indexable page.
  Off by default (§4).
- **Exit codes** as `check`: 0 pass, 1 fail, 2 a wrong command line.

## 4. What runs, and how a page is defined without routes

**The page set is every `.html` file under `<DIR>`.**
- **A page is indexable** unless it carries a robots noindex meta. That is the rule `check` already applies to static HTML.
- **Path to URL:** `index.html` is `/x/`; any other `.html` file is its own path, as in molde's URL conventions.

| Check | In verify | Notes |
|---|---|---|
| `links.internal` | always | as in `check`, with own-address links when `--base-url` is set |
| `titles.missing` / `.duplicate` / `.multiple` | always | |
| `html.double_escaped` | always | |
| `jsonld.invalid` | always | |
| `sitemap.*` | with `--base-url` and a `sitemap.xml` in `<DIR>` | the same mapping from `<loc>` to file; a page is excused from the sitemap by a noindex meta |
| `canonical.missing` / `.mismatch` / `.not_in_sitemap` / `.duplicate` | with `--base-url` | `missing` and `not_in_sitemap` on indexable pages only |
| `og.mismatch` / `og.duplicate` / `og.title_mismatch` | with `--base-url` | they compare what is there |
| `og.missing` | with `--og` | molde requires OG on titled routes; with no routes, it's opt-in |
| `description.duplicate` | always | two indexable pages with the same meta description |
| `robots.sitemap_missing` / `.blocks_all` | with `--base-url` and a `robots.txt` | |
| `robots.missing` | never | it needs a route's `index: false` |

**A missing sitemap** with `--base-url` is reported (`sitemap.missing`), because a site
that wants checking against a base URL wants a sitemap. This is the one check verify has
that `check` doesn't need, since molde always writes one. Review question: should it be
a warning instead?

## 5. The report

The same shape as `check`:

```json
{"verdict": "fail", "published": false,
 "build": {"pages": 1901, "assets": 312, "ms": 410, "dir": "public"},
 "skipped": ["sitemap", "canonical", "og", "robots"],
 "errors": [{"layer": "output", "check": "links.internal", "page": "posts/a/index.html",
             "attribute": "href", "broken_href": "/posts/b/", "hint": "…"}]}
```

- **No `route` or `template`** on errors. That is allowed: the compatibility section makes
  them "when present".
- **`skipped`** (new, also useful in `check` later) lists the check families that didn't
  run and why they didn't (no `--base-url`, no `sitemap.xml`). An agent then knows what is
  left to check itself (the benchmark found agents re-check by hand what they believe a
  checker misses).
- **Hints are written for any generator:** "a page links to a file the build doesn't
  have" rather than "render {{ std_head }}". `check` keeps its molde hints.

## 6. The publish gate (`--out O`)

With `--out`, a passing verify copies `<DIR>` into `O/builds/<id>/` and swaps the
`O/current` link, as `check` does. A failing one copies nothing and leaves `current` as
it was.
- **Build ids and locking** are the same as `check`'s, so `molde diff` and `molde serve`
  work on the result.
- **Dedup** is by content hash, so re-verifying an unchanged build reuses its id.
- **The copy:** on large sites (43k pages) copying costs seconds. Hard links where the
  file system allows, else a copy. Review question: is copying worth it at all, or should
  the gate only print the verdict and let the deploy step decide?

## 7. What verify must not get wrong on real generators

Each of these has to be handled, and tested against real output (§8):

- **Hugo**:
  - alias pages (a `<meta http-equiv="refresh">` page with `noindex` and a canonical pointing to the target) must pass: noindex excuses the page from the sitemap, and the canonical is not required on a noindex page;
  - `/404.html`, taxonomies, RSS (`index.xml`, not HTML).
- **Next.js static export**: the not-found page written three times (`404.html`, `404/index.html`, `_not-found/index.html`), `_next/` assets with hashes, `trailingSlash` on or off (`/a.html` or `/a/index.html`), `.txt` RSC payloads.
- **Astro**: hashed assets under `_astro/`, `@astrojs/sitemap`'s `sitemap-index.xml` + `sitemap-0.xml` (with `robots.txt` pointing at `sitemap-index.xml`, not `sitemap.xml`).
- **Eleventy, Zola**: plain output; Zola's `sitemap.xml` and `robots.txt`.
- **Sites under a path** (GitHub Pages `https://user.github.io/repo/`): links written as
  `/repo/x/` resolve against `<DIR>` only after removing the base path. `check` already
  maps sitemap locs with a base path; links need the same mapping (§R5).
- **A sitemap index** under any name: `robots.sitemap_missing` should accept any
  `Sitemap:` line that names a sitemap present in the build, not only `sitemap.xml`.
  `check` keeps its exact rule, since molde writes `sitemap.xml`.

## 8. How it is tested

- **The benchmark's ports as a corpus.** docs/bench/agent-cost has the same site in Hugo,
  Astro, Next.js, Zola and Eleventy (the idiomatic and parity ports), each built by its
  real generator.
  - **False positives:** each correct build must pass verify with `--base-url`.
  - **True positives:** each T4/T4b planted-defect workspace must fail on exactly the
    planted categories that verify can see (links, titles, canonical/og, sitemap,
    escaping), and only those.
- **Real sites:** proximosdividendos, whichindex and dividendpedia, built by molde: verify
  on their output must report what `check` reports, apart from provenance.
- **Unit tests** for the page and URL mapping, `skipped`, the gate, and each generator
  quirk in §7 as a fixture.

## 9. Later

- A `molde.verify.yaml` for what can't be inferred: budgets, the expected locales, the
  `@type`s pages must carry, extra paths to ignore.
- The GitHub Action that runs verify and comments the report on the pull request.
- An MCP tool `verify`.
- `skipped` in `check`'s report too.

## Risks

- **R1** False failures on correct output of a common generator (§7, §8): the main risk. Every quirk listed needs a fixture before release.
- **R2** A wrong `--base-url` makes every page fail: the flag is never guessed (§3), and the report's first error should say so when no page's canonical shares its host.
- **R3** Opinionated checks (canonical required, OG) fail sites that chose otherwise: OG is opt-in; canonical is required only with `--base-url`.
- **R4** The report shape drifts from `check`'s: the same ids and fields; `route`/`template` absent rather than empty.
- **R5** Base-path deployments: link resolution maps the base path (§7).
- **R6** Huge outputs: the same parallel engine; the gate's copy uses hard links (§6).
- **R7** Symlinks and files outside `<DIR>`: verify never follows a symlink out of `<DIR>` (the security model's rule for `--out`).
- **R8** Case-insensitive file systems: as `check`, a link that differs only in case passes on macOS; documented.
- **R9** Non-UTF-8 HTML: read lossily, as `check` does for static files.
- **R10** Generators that write the sitemap under another name: §7, a sitemap named in robots.txt.
- **R11** Scope creep into a crawler: no network, ever.
- **R12** It splits molde's message: README order, generator first or verifier first, is a product decision for the maintainer.

## Review 1 (19 Oct 2026): what v2 must change

**Method.** The reviewer:
- built Hugo 0.168 (Ananke theme), Eleventy's base blog, Astro's blog starter, Next.js 16 (`trailingSlash` on and off) and Zola from their official starters;
- wrote a prototype of this design (the existing check functions plus about 90 lines of head checks without the site model);
- ran it with `--base-url`.

**Finding 1: v1 fails every generator's default output.**

| Output | Pages | Errors |
|---|---|---|
| Hugo + Ananke | 31 | 43 |
| Eleventy starter | 13 | 30 |
| Astro starter | 8 | 14 |
| Next.js export | 8 | 2 |
| Zola | 6 | 2 |

The causes:
- **404 pages** carry no noindex (Hugo, Zola, Eleventy).
- **Hugo's alias pages and `page/1/` pagers carry no noindex** either, only a canonical to another page and a meta refresh. §7's premise was false.
- **Canonicals pointing at another page are legitimate** (list pagers).
- **Sitemaps legitimately leave pages out** (tag pages, pagers).
- **Sitemap discovery is fixed to `sitemap.xml`.** Astro writes `sitemap-index.xml`. With no sitemap at all, `canonical.not_in_sitemap` fires on every page.
- **Most defaults carry no canonical at all** (Eleventy, Next.js, theme-dependent Hugo and Zola).
- **`titles.duplicate` counts noindex pages,** such as Next.js's three not-found copies.
- **`og.title_mismatch` is an opinion on list pages.**

**v2's rules:**
- **A page classifier,** applied by every check: document, redirect or alias (a meta refresh, or a canonical to another page), 404 by convention (`404.html`, `404/index.html`), and non-document HTML (no `<html`, `<head` or doctype, such as a verification file).
- **Never require a canonical by default.** Check the ones that exist: duplicate, off-host, target missing, target noindex, and a non-canonical URL in the sitemap.
- **The sitemap is found** from robots.txt, then `sitemap.xml`, then `sitemap-index.xml`, with no cross-checks when there is none. Completeness becomes a warning, or opt-in.
- **Uniqueness of titles and descriptions** counts indexable documents only.
- **The `og.*` checks** run only with `--og`.

**Finding 2: §8's corpus can't find any of this.** The benchmark's ports were written to molde's requirements: Hugo with `disableKinds`, its own sitemap and robots templates; Next.js with the not-found copies pruned; `trailingSlash` set explicitly everywhere. They pass, which proves nothing about defaults. v2's corpus is each generator's official starter plus one realistic configuration (Hugo with a theme, taxonomies, pagination and aliases; Next.js with `trailingSlash: false`; Astro with `build.format: 'file'`), with the built outputs committed as fixtures.

**Finding 3: the URL model is molde's, but hosts differ.**
- **Clean URLs** (`/a` served from `a.html`, Next.js `trailingSlash: false`) need a `--clean-urls` flag used by links, sitemap and canonical alike. It can't be the default, because S3 and CloudFront don't serve them.
- **URLs are compared normalized:** an empty path is `/`, the host in any case, relative canonicals resolved.

**Finding 4: base paths** (`https://user.github.io/repo/`) need the prefix stripped *and required*. A root-relative link outside the base path works on disk and 404s in production. No existing link checker catches this.

**Finding 5: report contract.**
- **Ids:** if verify accepts a canonical to another page, `canonical.mismatch` can't mean the same in both commands. That needs new ids or the same rule in both.
- **Naming:** `sitemap.missing` → `sitemap.not_found`, as a warning.
- **Shape:** `skipped` as `[{checks, reason}]`; `"command": "verify"`; soft checks in `warnings`.

**Finding 6: v1 needs what §9 deferred.**
- `--ignore` patterns (`/cdn-cgi/`, `/api/*`, rewrites);
- an SPA fallback;
- honouring `<base href>`;
- meta-refresh targets checked;
- `.htm` and `.HTML` files;
- an empty folder, or one with no HTML, fails;
- the project root (holding `package.json`, `hugo.toml`) refused, naming the likely output folder;
- stale output (Hugo doesn't clean `public/`) documented.

**Finding 7: cut the publish gate from v1.** Foreign sites deploy through their own CI, so the exit code is the gate. The copy needs its own symlink policy, and the design's dedup claim doesn't match `build.rs`.

**Effort, re-estimated:** 8–11 working days: refactor 1–2, command and report 1, generator semantics 3–4, fixture corpus and tests 2–3, docs 1. Then upkeep as generator defaults change.

**Where the value is, per the review.** lychee, linkinator and html-proofer cover links. None checks offline, on the build folder, with stable ids, that the sitemap, canonicals, noindex and robots.txt agree with each other. Today that takes crawling the live site. To be worth building, verify must:
- pass the official starters with zero errors;
- understand base paths and clean URLs;
- group one root cause into one error.

A links-only verify is a slower lychee.

The prototype, the starter builds and the review's notes are kept outside the repository.
