# `molde verify` fixtures

The built output of each generator's official starter, plus realistic configurations, as
docs/design/verify.md §9 asks. **Built once, on 20 Oct 2026, and committed as files**: CI
never builds them. They're the release gate: verify must report, on each, exactly the
errors listed here, no more and none missing (`tests/verify.rs`
`every_fixture_reports_exactly_its_recorded_errors` holds the same list). Every listed error is a genuine defect of
the starter, with its reason.

The node-based fixtures were built in the official Node container
(`node:22-bookworm@sha256:0e5f906573693feaa1e21057ebdcfdb5bd5021f050b2dc7c9deceb629c7da2a8`,
Node 22.23.3), because the current Astro needs Node ≥ 22.22. Hugo 0.168.0 and Zola 0.23.6
were the locally installed builds (the same versions as the agent-cost benchmark).

| Fixture | Generator | Source | Pages | verify flags | Known genuine errors |
|---|---|---|---|---|---|
| `hugo-quickstart` | Hugo 0.168.0 | Hugo's quickstart (`hugo new site`, Ananke theme `18867db318d8`, 2026-09-27, one post) | 7 | `--base-url https://example.org/` | none |
| `hugo-realistic` | Hugo 0.168.0 | the quickstart plus 12 posts with tags, `pagerSize = 4` and one alias (`/old/post-3/`) | 29 | `--base-url https://example.org/` | none |
| `hugo-realistic-basepath` | Hugo 0.168.0 | `hugo-realistic` built with `-b https://user.github.io/repo/` | 29 | `--base-url https://user.github.io/repo/` | `links.outside_base`, one error for 21 pages: the content links `/posts/my-first-post/` root-relative, which 404s under the base path |
| `eleventy-base-blog` | Eleventy 3.1.7 | github.com/11ty/eleventy-base-blog `cf0cc1f1b597` (2026-10-15) | 13 | `--base-url https://example.com/` | none |
| `astro-blog` | Astro 7.4.1 | `npm create astro@latest -- --template blog` | 8 | `--base-url https://example.com/` | none (its `sitemap-index.xml` is found by name) |
| `nextjs-export` | Next.js 16.4.2 | `create-next-app@latest` defaults, `output: "export"`, `trailingSlash: false` | 3 | `--base-url https://example.com/ --clean-urls` | none |
| `nextjs-export-trailing-slash` | Next.js 16.4.2 | the same, `trailingSlash: true` | 4 | `--base-url https://example.com/` | none |
| `zola-getting-started` | Zola 0.23.6 | the "getting started" blog of Zola's docs (base, index, blog and blog-page templates; two posts), `build_search_index = true` | 5 | `--base-url https://example.com/` | none; every page's `<title>` is the tutorial's hard-coded "MyBlog", a `titles.duplicate` warning (an error with `--unique`) |

**Next.js:** create-next-app 16.4 enables `cacheComponents` and `partialPrefetching`. Both
imply partial prerendering, which static export rejects ("PPR cannot be enabled in export
mode"), so these fixtures drop them and set only `output` and `trailingSlash`.

## Layout

- `built/<fixture>/`: the output, as the generator wrote it. This is what verify reads.
- `sources/<generator>/`: the sources, without dependencies or build output, so anyone can rebuild:
  - Hugo: `hugo --cleanDestinationDir`, plus `-b https://user.github.io/repo/ -d public-basepath` for the base-path fixture. The Ananke theme isn't included: `git clone https://github.com/theNewDynamic/gohugo-theme-ananke themes/ananke && git -C themes/ananke checkout 18867db318d8`.
  - Eleventy: `npm ci && npx @11ty/eleventy`.
  - Astro: `npm ci && npx astro build`, in the Node image above.
  - Next.js: `npm ci && npx next build`, in the Node image above, once per `trailingSlash` value.
  - Zola: `zola build`.
- `licenses/`: the generators' and starters' licences. The outputs carry their code (Next.js's runtime, Ananke's CSS, Astro's and Eleventy's templates), all MIT.

**What changed from the starters:**
- the `AGENTS.md` files the Astro and Next.js templates ship are removed (instructions for agents working on those projects, not on molde);
- Hugo's posts, Zola's templates and the Next.js config are as described in the table.

**Lockfiles are pinned on purpose**, as the benchmark fixtures' are
(docs/bench/cross-framework/harness/README.md). A Dependabot alert on
`sources/*/package-lock.json` is dismissed as "not used": they are never installed outside
a fixture rebuild. Rebuild the fixtures on each generator's major release, and update the
table.
