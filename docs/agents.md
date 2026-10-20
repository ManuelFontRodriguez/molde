# molde in five minutes

A short guide for agents and people working on a molde site. `molde guide` prints it.
The full reference is `docs/reference.md`; open it only for what this page doesn't cover.

## First: run the check

```sh
molde check --site . --out out
```

It builds the site, runs every check and prints **one JSON report** listing every
problem. It publishes the build to `out/current` only when the verdict is `pass`. Run it
before you change anything (it is your baseline, and it costs nothing if nothing
changed), and again after each change. The report is complete: fix everything it lists,
in one batch, then run it again.

```json
{"verdict": "fail", "published": false, "build": {"pages": 5, "dir": "out/builds/…"},
 "errors": [{"check": "links.internal", "page": "index.html", "broken_href": "/books/x/",
             "route": "/index.html", "template": "index.html.j2", "hint": "…"}]}
```

Every error names the page, the **route** and the **template** that made it, and a
`hint`. Edit the template or `routes.yaml`, not the output. Exit code 0 = pass, 1 = fail.

## A site is a folder of data

```
site/
├── routes.yaml      # which pages exist: required
├── data.json        # the content, one JSON snapshot: required
├── templates/       # minijinja (Jinja2) templates: required
├── site.yaml        # base_url and other capabilities: optional
└── static/          # copied into every build as is: optional
```

Pages come from `data.json` through `routes.yaml`. Nothing is written by hand into the
output, and the output is never edited.

## routes.yaml

```yaml
routes:
  - path: /index.html                    # a single page
    template: index.html.j2
    title: "My site"                     # gives <title> and Open Graph via std_head

  - path: /books/{id}/index.html         # one page per item of data.json's "books" array
    template: book.html.j2
    foreach: books
    title: "{{ item.title }} by {{ item.author }}"
    description: "{{ item.summary }}"    # optional: meta description

  - path: /genres/{genre|slug}/index.html  # one page per distinct value of a field
    template: genre.html.j2
    foreach: books
    group_by: genre                      # item = {"genre": …, "items": [the books]}
    title: "{{ item.genre }} books"

  - path: /books/index.html              # a list split into pages
    template: list.html.j2
    paginate: {over: books, per_page: 20, path: "/books/page/{n}/index.html"}
    title: "Books{% if pagination.n > 1 %}, page {{ pagination.n }}{% endif %}"

  - path: /404.html
    template: 404.html.j2
    title: Page not found
    index: false                         # rendered, never in the sitemap, noindex meta
```

- `/x/index.html` is served at `/x/`. Paths start with `/` and don't end with `/`.
- `{field}` is filled from each item (a string or an integer). `{field|slug}` makes a
  URL-safe slug; templates link to it with the same `slug` filter:
  `href="/genres/{{ b.genre | slug }}/"`.
- `title` is a pattern rendered per page. Make it different per page, or
  `titles.duplicate` fires. `index`, `description` and `lastmod` are patterns too.
- An unknown key (a typo) fails loading the site, on purpose.

## site.yaml

```yaml
base_url: https://example.com   # turns on sitemap.xml, robots.txt, std_head and the head checks
```

Without `base_url`, `{{ std_head }}` renders empty and there is no sitemap.

## Templates

minijinja, the Rust Jinja2 dialect. In a template:

| Name | What |
|---|---|
| `data` | the whole of data.json |
| `item` | the current element, on `foreach` routes |
| `pagination` | on `paginate` routes: `n`, `pages`, `items` (this page's slice), `prev`, `next`, `links` (`[{n, path}]` for a pager) |
| `page.path`, `page.url` | this page's public path (`/books/dune/`) and full URL |
| `page.title`, `page.description`, `page.index` | the strings `std_head` prints and the page's `index` value, for extra tags (`twitter:title`) |
| `std_head` | the head block: `<title>`, description, canonical and Open Graph, already escaped |

- **Put `{{ std_head }}` inside `<head>`** and don't write `<title>`, canonical or `og:*`
  tags by hand: the checks compare them with what `std_head` writes.
- **Autoescape is on** for `*.html.j2`: `{{ item.name }}` with `AT&T` writes `AT&amp;T`,
  which the browser shows as `AT&T`. Don't escape by hand (`replace("&", "&amp;")`
  escapes twice: `html.double_escaped`). Other outputs (`.json.j2`, `.txt.j2`) are plain
  text; build JSON with `tojson`.
- **JSON-LD:** `<script type="application/ld+json">{{ {"@type": "Book", "name": item.title} | tojson }}</script>`.
  `tojson` output is safe inside `<script>`, and it is how data enters an inline script:
  `var x = {{ x | tojson }};`, never `"{{ x }}"`.
- **No Python string methods:** `s.split(',')`, `s.startswith('a')` are errors; write
  `s | split(',')`, `s is startingwith('a')`, `'a' in s`.
- **A loop's `if` filter doesn't see the body's `namespace`:** test inside the body
  (`{% for … %}{% if ns.n < 1 %}…{% endif %}{% endfor %}`).
- **Missing fields render empty**, without an error (`{{ item.typo }}` is blank).
  `{{ missing.x }}` is an error. `null` prints as nothing; test it with `is none`.
- **Sorting:** `sort(attribute="name")` ignores ASCII case; items whose field is `null`
  sort first, so filter them out before sorting.
- **No Python methods:** use filters (`| upper`, `| items`), not `.upper()`.
- Partials: `{% include "_header.html.j2" %}` (sees the full context);
  macros: `{% import "_util.html.j2" as u %}`. Name HTML partials `*.html.j2`, or
  autoescape is off in them.
- Extra filters: `slug`, `year`, and with `locale` in site.yaml, `date_format` and
  `num_format`. Not available: `urlencode`, `{% break %}`.

## static/

Copied as is (`static/style.css` is `/style.css`). A route always wins over a static file
at the same path. With `base_url`, molde writes `robots.txt` (naming the sitemap) and
`sitemap.xml` itself; a `static/robots.txt` replaces the generated one and must still
name the sitemap.

## What the check covers

Every internal `href`, `src`, `srcset` and CSS `url()` (relative, starting with `/`, or absolute to the site's own `base_url`) resolves to a file; the sitemap
lists exactly the indexable pages; every page has one non-empty, unique `<title>`; indexable
pages have a canonical and, on routes with a `title`, the Open Graph block; no page has
two of either; the canonical and `og:url` match the page, and `og:title` doesn't
contradict the title; descriptions are present and unique where a route declares them; JSON-LD
parses (and carries the `@type`s a route's `jsonld:` names); nothing is escaped twice;
noindex pages carry their meta; robots.txt names the sitemap and lets crawlers in; size
budgets hold. Template, data and config errors stop
the build with the file and, where known, the line.

**It doesn't read meaning:** a wrong value in the text, a hard-coded string that should
come from the data, or the right element on the wrong pages all pass. It doesn't run
JavaScript or render CSS. When a task touches those, check the output yourself
(`out/current/…`), and only those.

## The other commands

```sh
molde build  --site . --out out          # build and publish, no checks
molde routes --site .                    # every route and its pages, as JSON
molde render /books/dune/ --site .       # one page to stdout, no build
molde diff   --out out [--full]          # what changed between the last two builds
molde gc     --out out --keep 5          # delete old builds
molde mcp    --site . --out out          # the same tools over MCP, for agents
```

- **Output:** `out/builds/<id>/` holds every build; `out/current` is a link to the
  published one. Refer to it from the site folder (`out/current/index.html`).
- **Testing a data change without touching data.json:** copy the site folder to a
  temporary directory, edit the copy's data.json, and run `molde check` on the copy.
