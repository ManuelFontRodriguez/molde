# `molde verify`: the check for any static site's output

**Status: v2 (20 Oct 2026), for review 2 before any code.** v1 failed review 1: a prototype
of it reported false failures on the default starter of every generator it was run on
(the review is kept at the end, as the record). Each rule below that differs from v1 names
the finding it answers (Finding 1–Finding 7). Building waits on the maintainer's decision after
molde's first launch.

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

**The release gate (Finding 1, Finding 2).** verify ships only when, on the official
starter of Hugo, Astro, Next.js (static export, `trailingSlash` on and off), Eleventy and
Zola, plus the realistic configurations (§9), it reports **exactly each fixture's recorded
errors: none extra, none missing**. A recorded error is a genuine defect of the starter,
with its reason (for example, Zola's tutorial gives every page the title "MyBlog"). A
verifier that fails a correct site on day one is uninstalled; one that hides a real defect
is worse.

**Goals.**
- `molde verify <dir>` checks a folder of built HTML and prints a report in `check`'s shape.
- **The value is the cross-page consistency** no link checker covers, checked offline on the build folder: the sitemap, the canonicals, noindex, robots.txt and the pages agreeing with one another. A links-only verify would be a slower lychee.
- **One root cause, one error:** when hundreds of errors share a cause (a wrong `--base-url`, clean URLs), report the cause.
- **The same ids and meaning as `check`** wherever a check runs in both. Where verify needs a rule `check` doesn't have, it gets a new id rather than bending an old one (Finding 5).

**Non-goals (v1 of verify).** Provenance (which template made a page); locales; size
budgets; `jsonld.type_missing`; `description.missing`; the publish gate (Finding 7, §8); fetching
anything; running JavaScript; fixing anything.

## 3. Command line

```
molde verify <DIR> [--base-url URL] [--clean-urls] [--ignore PATTERN]...
                   [--spa-fallback PATH] [--require-canonical] [--og] [--sitemap-complete]
```

| Flag | Meaning | Answers |
|---|---|---|
| `--base-url` | the site's public address, possibly with a path (`https://user.github.io/repo/`). Turns on the sitemap, canonical, robots.txt and own-address checks; never inferred | v1 §3 |
| `--clean-urls` | the host serves `/a` from `a.html` (Netlify, Vercel, Cloudflare Pages, GitHub Pages). Used by links, the sitemap and canonicals alike. Off by default, because S3 and CloudFront don't do it | Finding 3 |
| `--ignore` | a URL path glob that is not this build's to check (`/cdn-cgi/*`, `/api/*`, a rewrite). Repeatable | Finding 6 |
| `--spa-fallback` | a file the host serves for every unknown path (`/index.html`, `/200.html`). With it, an internal link to a page that isn't a file is not an error | Finding 6 |
| `--require-canonical` | an indexable page without a canonical is an error (`canonical.missing`). Off by default: most starters carry none | Finding 1 |
| `--og` | require a full Open Graph block (`og.missing`) and an og:title that agrees with the title (`og.title_mismatch`) | Finding 1 |
| `--sitemap-complete` | an indexable document missing from the sitemap is an error, not a warning | Finding 1 |

Exit codes as `check`: 0 pass, 1 fail, 2 a wrong command line. Warnings never fail.

**Refusals (Finding 6, Finding 7):**
- **An empty folder,** or one with no HTML, fails with one error (`verify.no_pages`). It is the CI hole of a build that failed silently.
- **A folder that looks like a project root** (it holds `package.json`, `hugo.toml`, `config.toml`, `astro.config.*`, `next.config.*` or `.eleventy.js`) is refused, and the error names the likely output folder (`public/`, `dist/`, `out/`, `_site/`).

## 4. The page classifier (Finding 1, Finding 6)

Built once, used by every check. Each `.html` / `.htm` file (any case) under `<DIR>` is
exactly one of:

| Kind | How it is recognised | What it is held to |
|---|---|---|
| **not a document** | no `<html`, `<head` or doctype in its first 2 KB (a search-engine verification file, a fragment) | nothing; it isn't a page |
| **not-found page** | `404.html`, `404/index.html`, `_not-found/index.html`, `200.html` | links only; never in the sitemap, uniqueness, or canonical checks |
| **redirect** | a `<meta http-equiv="refresh">`, or (Hugo aliases, pagers) a canonical to another page of the site | its target resolves (`redirect.target_missing`); never in sitemap completeness or uniqueness |
| **noindex document** | a robots noindex meta | links, titles present; excused from the sitemap and from uniqueness |
| **indexable document** | everything else | every check |

`check` keeps its own page model from routes, but adopts the noindex rule for uniqueness
(B11, `fix(check): pages with noindex may share a title`) so that the two commands agree.

## 5. Checks

| Check | verify | Notes |
|---|---|---|
| `links.internal` | always | with the URL model of §6 |
| `titles.missing`, `titles.multiple` | documents | |
| `titles.duplicate`, `description.duplicate` | indexable documents | **warnings** on list-like pages (a path ending `/page/N/`, or a taxonomy path), errors elsewhere |
| `html.double_escaped`, `jsonld.invalid` | documents | |
| `canonical.duplicate`, `og.duplicate` | documents | |
| `canonical.off_host` (new) | with `--base-url` | a canonical to another host, or outside the base path |
| `canonical.target_missing` (new) | with `--base-url` | a canonical to a page of this site that isn't in the build |
| `canonical.target_noindex` (new) | with `--base-url` | a canonical to a page carrying noindex |
| `canonical.missing` | with `--require-canonical` | |
| `canonical.mismatch` | **not in verify** | a canonical to another page is legitimate there (pagers, aliases); `check` keeps it, since molde writes canonicals itself |
| `sitemap.dangling`, `sitemap.noindex_listed` | with a sitemap (§7) | |
| `sitemap.non_canonical_listed` (new) | with a sitemap | the sitemap lists a page whose canonical points elsewhere |
| `sitemap.missing_page` | **warning**; error with `--sitemap-complete` | sitemaps legitimately leave pages out |
| `sitemap.not_found` (new) | **warning**, with `--base-url` and no sitemap | not `sitemap.missing`, which would read like `sitemap.missing_page` |
| `robots.sitemap_missing`, `robots.blocks_all` | with `--base-url` and a robots.txt | `sitemap_missing` accepts any Sitemap line naming a sitemap in the build |
| `redirect.target_missing` (new) | always | a meta refresh, or an alias's canonical, that leads nowhere |
| `og.*` | with `--og` | |

The new ids are new because their meaning is new (Finding 5). Those that make sense in `check`
too (`canonical.target_missing`, `redirect.target_missing`) arrive there in the same
release.

## 6. The URL model (Finding 3, Finding 4, Finding 6)

- **Base path, stripped and required (Finding 4).** With `--base-url https://host/repo/`, a link `/repo/x/` resolves to `x/` in the build. A root-relative link outside the base path (`/x/`) is an error, `links.outside_base`: it works on disk and 404s in production. This is the check no link checker has.
- **Clean URLs (Finding 3).** With `--clean-urls`, `/a` also resolves to `a.html`, in links, sitemap `<loc>`s and canonicals alike.
- **URLs compared normalized:** the scheme and host in any case, an empty path the same as `/`, relative canonicals resolved against the page. B12 (`fix(check): canonical and og:url are compared as URLs`) does the same in `check`.
- **`<base href>`** is honoured, in verify only: Angular and Flutter builds rely on it, and molde never writes one.
- **Meta-refresh targets** are checked (`redirect.target_missing`).
- **One cause, grouped.** If more than half the internal-link errors would resolve under `--clean-urls`, or under a base path, the report says so once, with the flag to try, instead of listing them all.

## 7. Finding the sitemap (Finding 1)

The first that exists:
1. the `Sitemap:` lines of `robots.txt` that name a file in the build;
2. `sitemap.xml`;
3. `sitemap-index.xml`;
4. `sitemap_index.xml`.

A sitemap index is followed into its children. `<loc>`s are read namespaced or in
CDATA, and `<lastmod>` accepts every W3C form the protocol allows. With no sitemap at
all, nothing compares against one: `sitemap.not_found` is a warning, and no
canonical-in-sitemap check runs.

## 8. The report (Finding 5)

`check`'s shape, plus:
- **`"command": "verify"`.**
- **`skipped`:** `[{"checks": "sitemap", "reason": "no sitemap in the build"}, …]`.
- **`warnings`** (as `check` already has with locales): never fail the verdict.
- **`pages`:** counts per kind (§4), so a reader sees what was classified as what.
- **No `route` or `template`**, which the compatibility rules allow. Hints are written for any generator, not for `std_head`.

**No publish gate in verify's first version (Finding 7):** foreign sites deploy through their
own CI, where the exit code is the gate.

## 9. How it is tested (Finding 2)

- **Fixtures, committed as built output** (built once, not in CI):
  - each generator's official starter, in its default configuration;
  - one realistic configuration each: Hugo with a theme, taxonomies, pagination and aliases; Next.js with `trailingSlash: false`; Astro with `build.format: 'file'`; a GitHub Pages build under a base path.
- **Acceptance:** on every fixture, with the flags its host needs (none, `--clean-urls`, the base path), exactly the errors its README records as genuine. `tests/verify/fixtures/README.md` lists both.
- **The corpus** is in `tests/verify/fixtures/` (built 20 Oct 2026): Hugo's quickstart (Ananke), a realistic Hugo site plain and under a GitHub Pages base path, Eleventy's base blog, Astro's blog template, Next.js's static export with `trailingSlash` off and on, and Zola's getting-started blog.
- **True positives:** each fixture copied with one planted defect per check, failing on exactly that one.
- **molde's own sites:** verify on their output reports what `check` reports, apart from provenance.
- **The prototype numbers of review 1** are the baseline v2 must improve on: 43, 30, 14, 2 and 2 errors, down to the recorded genuine ones.

## 10. Effort and order

About 8–11 working days, as review 1 estimated:
- the refactor (page classifier, link rules, head checks without the site model) 1–2;
- the command and report 1;
- generator semantics 3–4;
- fixtures and tests 2–3;
- docs 1.

**Order:**
1. the fixtures first, so every later step is measured against them;
2. then the classifier;
3. then the checks one family at a time, each PR keeping the fixtures at zero.

Then upkeep as generator defaults change: fixtures get rebuilt on each generator's major release.

## 11. Later

A `molde.verify.yaml` for what flags can't say (ignores, expected locales, required
`@type`s); the GitHub Action that runs verify and comments the report on a pull request;
an MCP tool `verify`; `skipped` in `check`'s report.

## Risks

- **R1 False failures on correct output.** The release gate (§2) and the fixtures (§9) answer it; every generator quirk becomes a fixture before release.
- **R2 A wrong `--base-url`** makes every page fail. The grouped cause (§6) says so in one error.
- **R3 Opinionated checks.** Canonical, OG and sitemap completeness are opt-in; uniqueness on list pages is a warning.
- **R4 The two commands drift apart.** The same ids where the meaning is the same, new ids where it isn't (§5); B11 and B12 align `check`.
- **R5 Upkeep.** Generator defaults change (Next.js 16 added `__next.*.txt` files). Fixtures are rebuilt on each major release, and verify's README lists the versions tested.
- **R6 Scope creep.** No network, no rendering, no fixing, ever.
- **R7 The message.** verify launches separately, after molde, on the maintainer's decision.

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
