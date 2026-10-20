# molde

**A static site generator built for AI agents.** You describe a site as data (routes,
templates and a JSON snapshot); molde renders it, checks it, and returns **one JSON
report** with every problem its checks find, where it comes from and how to fix it. An
agent learns everything wrong with a change in a single call, and a failing site never
goes live.

```sh
molde check --site my-site --out out
```

```json
{
  "verdict": "fail",
  "published": false,
  "build": { "pages": 9, "…": "…" },
  "errors": [
    {
      "layer": "output",
      "check": "links.internal",
      "page": "index.html",
      "attribute": "href",
      "broken_href": "/books/dnue/",
      "route": "/index.html",
      "template": "index.html.j2",
      "hint": "no file in the build matches this href — add a route that emits it, or fix the href"
    }
  ]
}
```

![An agent's edit breaks a link: molde check fails with the page, template and a hint, publishes nothing, and passes again after the fix](docs/media/molde-check.gif)

- **Verified builds.** Internal links, titles, the sitemap, canonical and Open Graph
  tags, JSON-LD, hreflang, translations and output size budgets are checked on every
  build. A build publishes only when every check passes; otherwise the last good one
  stays live.
- **Deterministic.** The same site and the same molde produce the same bytes. Every
  build is kept, so `molde diff` shows exactly what a change did to every page.
- **Agent-native.** `molde mcp` serves the site to an agent over the Model Context
  Protocol: check, list and explain routes, render a page, diff two builds.
- **Fast and small.** One self-contained binary, no runtime, no node_modules, and its
  dependencies audited with `cargo deny`. 1,901 pages build in 170 ms and build plus
  every check in under half a second on a laptop (Apple M4 Pro); see the
  [benchmark](docs/bench/cross-framework/report.md) against Hugo, Zola, Eleventy, Astro
  and Next.js.
- **Multilingual.** Localized paths, hreflang and translations, with dates and numbers
  formatted for 529 locales from bundled Unicode CLDR data.

It works without an agent too: it is an ordinary command-line tool, and the report is
plain JSON for CI, a script or a person. molde runs on your machine and sends nothing
anywhere: no telemetry, no update checks, no account.

(molde is Spanish and Portuguese for a mould or template, the kind you pour something into. It is
not related to the `mold` linker.)

## Install

molde runs on **Linux and macOS** (Windows: use WSL).

**A release binary.** Download the archive for your platform from the
[releases page](https://github.com/ManuelFontRodriguez/molde/releases), check it and
put `molde` on your `PATH`:

```sh
VERSION=0.2.0
TARGET=aarch64-apple-darwin        # or x86_64-apple-darwin, x86_64-unknown-linux-musl, aarch64-unknown-linux-musl
curl -fLO "https://github.com/ManuelFontRodriguez/molde/releases/download/v$VERSION/molde-$VERSION-$TARGET.tar.gz"
curl -fLO "https://github.com/ManuelFontRodriguez/molde/releases/download/v$VERSION/SHA256SUMS"
shasum -a 256 --check --ignore-missing SHA256SUMS   # Linux without shasum: sha256sum --check --ignore-missing SHA256SUMS
tar -xzf "molde-$VERSION-$TARGET.tar.gz"
sudo mv "molde-$VERSION-$TARGET/molde" /usr/local/bin/
molde --version
```

The Linux binaries are static and run on any distribution. On macOS, an archive
downloaded with a browser is quarantined by Gatekeeper; downloading with `curl` as above
avoids that (or run `xattr -d com.apple.quarantine molde`).

**From source** (Rust 1.89 or newer, from [rustup.rs](https://rustup.rs)):

```sh
cargo install --locked --git https://github.com/ManuelFontRodriguez/molde --tag v0.2.0
```

## Your first site in five minutes

molde's repository carries a small example site (a library of books). Clone the
repository, even if you installed a release binary, copy the example and run the check:

```sh
git clone --depth 1 https://github.com/ManuelFontRodriguez/molde
cp -r molde/examples/demo my-site
molde check --site my-site --out out      # renders, checks, publishes to out/current
molde serve --out out                     # open http://127.0.0.1:8080
```

`--site` and `--out` default to `site` and `out`, so inside a folder laid out that way
`molde check` alone is enough. `out/` holds every build (`molde gc` prunes old ones);
keep it out of version control (add `out/` to `.gitignore`).

Two more examples come with it: [examples/multilingual](examples/multilingual) (a city
guide in three languages, with translations, localized paths, hreflang, pagination and
dates and numbers per locale) and [examples/broken](examples/broken) (five mistakes
and the report that catches them).

A site is a folder:

```
my-site/
├── routes.yaml          # which pages exist: path patterns, one page per data item
├── data.json            # the content, as plain JSON
├── templates/           # minijinja (Jinja2 syntax) templates
├── static/              # copied as is (CSS, images, fonts)
└── site.yaml            # optional: base URL, locales, size budgets
```

`routes.yaml` turns data into pages:

```yaml
routes:
  - path: /books/{id}/index.html        # one page per item of data.json's "books"
    template: book.html.j2
    foreach: books
    title: "{{ item.title }} by {{ item.author }}"

  - path: /genres/{genre|slug}/index.html   # one page per genre found in the data
    template: genre.html.j2
    foreach: books
    group_by: genre
```

The templates are Jinja2. Open [examples/demo/templates](examples/demo/templates) to see
what a page gets (`item`, `data`, and `std_head` for the standard head tags); the
[template contract](docs/reference.md#template-contract) lists the rest, such as `page`.

Now break something on purpose (link to a page that doesn't exist, or remove a title)
and run `molde check` again: the report names the page, the route and the template, and
the previous build stays live. While you edit, `molde serve --out out --watch --site
my-site --reload` rechecks on every save and refreshes the browser when a build passes.

## What molde checks

| Area | Checks |
|---|---|
| Links | every `href`, `src`, `srcset` and CSS `url()` that points inside the site, relative, root-relative or absolute to its own address, resolves to a file (`links.internal`) |
| Titles and descriptions | present, single, not duplicated across pages (`titles.*`, `description.*`) |
| Search engines | `sitemap.xml` matches the indexable pages and has valid dates; one canonical and one full Open Graph block per page, agreeing with the page's URL and title, robots tags, and a robots.txt that names the sitemap and lets crawlers in (`sitemap.*`, `canonical.*`, `og.*`, `robots.*`) |
| Structured data | JSON-LD parses and carries the `@type`s a route promises (`jsonld.*`) |
| Languages | `lang`, hreflang, localized paths, translations missing or unknown (`html.lang`, `hreflang.mismatch`, `paths.*`, `translations.*`) |
| Text | text escaped twice, which shows `&amp;` to readers (`html.double_escaped`) |
| Size | output size budgets per file pattern, raw or gzipped (`budget.*`) |
| Sources | template syntax and render errors, invalid config and data, with file and line (`template.*`, `config.*`) |

Each check, its fields and its hint are in the [reference](docs/reference.md#the-check-report),
along with [what the checks do not see](docs/reference.md#what-the-checks-do-not-see):
they read structure, not meaning. [examples/broken](examples/broken) shows a report for
five typical mistakes.

### A check, not a linter

A linter such as ESLint reads your source code, one file at a time, for patterns and style.
`molde check` reads what you ship: the built site, all of it at once.
- **Many site bugs exist in no source file.** A link breaks because a slug in `data.json`
  changed, or two pages get the same title because two items share a name: every template
  is correct, and only the output is wrong.
- **What it checks belongs to the whole build:** the sitemap lists exactly the indexable
  pages, no two pages share a title, every canonical matches its page. A linter, or an
  HTML validator that checks one page at a time, can't see these.
- **It works like a test of the release:** a build that fails isn't published. Each error
  still points back to the source, by the route and template that made the page.

Use both: a linter keeps the code clean, and the check tells you the site you are about to
publish is right.

### A site molde didn't build

`molde verify` runs the same kind of check on any generator's output: Hugo, Astro, a
Next.js static export, Eleventy, Zola.

```sh
molde verify public --base-url https://example.com/
```

It checks that the links, the canonicals, the sitemap, noindex and robots.txt agree with
the pages and with each other, offline, on the folder you are about to deploy, and it
prints the same JSON report. It also understands base paths (a GitHub Pages site at
`/repo/`) and clean URLs. Errors name the page, but not the template (only molde knows
that). It is tested on each generator's official starter. See
[Verify any site](docs/reference.md#verify-any-site).

## In CI or a script

`molde check` exits 0 when every check passes and the build is published, 1 when a check
fails (nothing is published), and 2 for a wrong command line. The report goes to stdout:

```sh
molde check --site my-site --out out > report.json || {
  jq -r '.errors[] | "\(.check) \(.page // "") \(.hint // "")"' report.json
  exit 1
}
```

**On GitHub,** the molde action does the same in one step. It downloads the release (its
SHA-256 checked), runs the command, writes the report to the job summary and keeps one
comment on the pull request up to date:

```yaml
permissions:
  contents: read
  pull-requests: write          # for the comment; without it, the summary still has the report
steps:
  - uses: actions/checkout@v4
  - run: hugo --cleanDestinationDir   # or any generator; for a molde site, skip this
  - uses: ManuelFontRodriguez/molde@v0.2.0
    with:
      command: verify               # or check, with site: and out:
      dir: public
      base-url: https://example.com/
```

It fails the job when the check fails (`fail-on-error: "false"` to only report), and sets
the outputs `verdict`, `errors` and `report`, the path of the JSON report. Every input is
in [action.yml](action.yml).

- **The molde it runs is the release the action is pinned to** (`@v0.2.0`). Pinned to a
  commit SHA, set `version: 0.2.0` too; `version: latest` asks for the newest on purpose.
- **The report is treated as the pull request author's text:** everything from it is
  escaped in the comment and the log, so a page title can't post an image, mention
  someone or forge an annotation. The action edits only its own comment, never one
  someone else wrote, and molde never sees the token.

## When not to use molde

- **A blog or docs site written in Markdown.** molde renders data, not content files; Hugo,
  Zola or Eleventy fit better, unless you already turn your content into JSON.
- **An application with client-side state.** Use a framework built for it, such as
  Next.js or Astro.
- **A site you edit in a visual editor or CMS.** molde is for sites generated from data,
  usually by a program or an agent.
- **Native Windows.** molde builds and runs on Linux and macOS (and on Windows through
  WSL).

## Use it with an agent

Start the agent with `molde guide`. It prints [a short guide](docs/agents.md) (about 2k
tokens) with the site layout, `routes.yaml`, templates, what the check covers and how
to read its report. The full [reference](docs/reference.md) is for details.

Register the MCP server with your agent, for example Claude Code:

```sh
claude mcp add molde -- molde mcp --site /path/to/my-site --out /path/to/out
```

Other clients take the same command in their configuration, usually as:

```json
{
  "mcpServers": {
    "molde": {
      "command": "molde",
      "args": ["mcp", "--site", "/path/to/my-site", "--out", "/path/to/out"]
    }
  }
}
```

Use absolute paths: the client may start the server from any folder.

The agent gets five tools: `check` (the complete report in one call), `list_routes`,
`explain_route`, `render_page` (a page's HTML without a browser) and `diff_build` (what a
change did). `check` publishes to `--out` when the site passes, exactly like
`molde check`. What the agent server can and cannot reach is in the
[security model](docs/reference.md#security-model).

## Commands

```sh
molde check  --site S --out O          # build + every check -> one JSON report; publish on pass
molde build  --site S --out O          # build + publish, no checks
molde routes --site S                  # every route and its pages, as JSON
molde render /books/dune/ --site S      # one page from source to stdout, no build
molde diff   --out O [--full]          # what changed between the last two builds (or --from/--to)
molde gc     --out O [--keep 10]       # delete old builds, keep the newest N and the live one
molde serve  --out O [--watch --site S --reload]   # local preview, recheck on save
molde mcp    --site S --out O          # the agent server (MCP over stdio)
molde events --site S --out O          # POST /hint -> a verified rebuild
molde guide                            # a short guide for agents and people
molde verify DIR --base-url URL        # check any generator's built output
```

## Documentation

- [docs/reference.md](docs/reference.md): the site format, the template contract, every
  check and its fields, locales, the security model and what each version promises.
- [docs/design/](docs/design/): the reasoning behind the bigger capabilities (locales,
  date and number formatting, web standards): the alternatives considered and the
  risks each design was checked against.
- [CHANGELOG.md](CHANGELOG.md): what changed in each release, and
  [ROADMAP.md](ROADMAP.md): what comes next and what molde leaves out on purpose.
- [SUPPORT.md](SUPPORT.md): where to ask a question or report a problem;
  [CONTRIBUTING.md](CONTRIBUTING.md): how to propose a change;
  [SECURITY.md](SECURITY.md): how to report a vulnerability privately; and the
  [code of conduct](CODE_OF_CONDUCT.md).
- [docs/bench/cross-framework/report.md](docs/bench/cross-framework/report.md): how molde
  compares with five other generators on the same site.

## How molde works

| Part | What it does |
|---|---|
| **router** | Routes are data (`routes.yaml`): path patterns, `foreach` over the snapshot's collections, `paginate` for long lists, per-locale paths. |
| **engine** | minijinja templates (Jinja2 syntax), with a pinned contract: autoescape rules, filters, what `data`, `item` and `page` hold. |
| **store** | Every build is an immutable folder with precompressed `.html.gz` siblings; publishing is an atomic symlink swap, so a server never sees half a site. |
| **conformance** | Links, titles, sitemap, canonical and Open Graph, descriptions, robots, JSON-LD, size budgets, locales, source errors. Publishing is gated on a pass. |
| **events** | `molde events`: a webhook `POST /hint` runs the verified pipeline, rate-limited; the request body is never read. |
| **context** | `molde mcp`: the site served to agents (check, list and explain routes, render a page, diff builds). |

## Principles

1. **Builds are pure functions.** The same site and the same molde give the same bytes;
   golden outputs are the review unit.
2. **A capability ships with its checks.** Anything that can be wrong in the output
   comes with the check that catches it, or it doesn't ship.
3. **Stale but consistent beats fresh but broken.** A failing check never publishes;
   the last good build stays live.
4. **Full rebuilds by default.** Partial rebuilds only if scale ever demands them, and
   then inferred, never hand-maintained.
5. **Webhooks are hints, never commands.** A hint can only trigger the same verified
   pipeline a person would run.
6. **Every design is attacked before it is built.** A new capability gets an
   adversarial review of its design before any code.

## Status

molde is at **0.2.0**. It
builds three sites in production, every day, from their data:
[proximosdividendos.info](https://proximosdividendos.info) (about 2,900 pages),
[whichindex.com](https://whichindex.com) (about 12,200) and
[dividendpedia.com](https://dividendpedia.com) (about 43,100). The command line, the site
format and the check report are the supported surface, and the
[compatibility section](docs/reference.md#compatibility) says what may change between
versions. The Rust library API is not stable yet.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. The Unicode CLDR data compiled into molde is under the
[Unicode License v3](src/i18n/LICENSE-UNICODE).

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in molde by you, as defined in the Apache-2.0 license, shall be dual licensed
as above, without any additional terms or conditions.
