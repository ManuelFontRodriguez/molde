"""Workspace setup per task, applied after the port is staged and before the agent
starts (runs in the preparation container, which has the bench folder; the agent's
container never does).

Every edit is an exact string replacement that must match once, so a plant that no
longer applies fails loudly instead of silently planting nothing. After setup, the
judge checks that the planted workspace fails the oracle on exactly the planted
categories (runner/verify_plants.py), so each plant is shown to be real.
"""

import json
from pathlib import Path

# ---------------------------------------------------------------- data changes

def add_dividend_yield(data):
    for i, s in enumerate(data["stocks"]):
        s["dividend_yield"] = f"{3.1 + 0.85 * i:.2f}%"
    return data


DATA_CHANGES = {"T1": add_dividend_yield}

# ---------------------------------------------------------------- planted defects
# T4: six defects molde's checks detect. T4b: five defects molde's checks cannot see
# (docs/reference.md, "What the checks do not see"), plus a wrong value.
# Each entry: (file, old, new, category). Categories name the oracle rule each must fail.

PLANTS = {
    # Re-planted 15 Oct after molde 89954dc (new link and escaping checks), so that no two
    # plants mask each other (the first version put the canonical plant on noindex pages).
    "molde": {
        "T4": [
            # 1. broken href on the home page
            ("templates/index.html.j2", 'href="/stocks/{{ s.ticker }}/"', 'href="/stocks/{{ s.ticker }}x/"', "R2_links"),
            # 2. duplicate title: every stock page gets the same title
            ("routes.yaml", 'title: "{{ item.name }} ({{ item.ticker }})"', 'title: "Stock page"', "R4_titles"),
            # 3. the stock template writes its head by hand: no canonical, og:url pointing at
            #    the home page (the title stays the duplicated "Stock page", so plant 2 survives)
            ("templates/stock.html.j2", "{{ std_head }}",
             '<title>Stock page</title>\n<meta property="og:title" content="Stock page">\n'
             '<meta property="og:url" content="https://demo.example/">\n<meta property="og:type" content="website">',
             "R5_canonical_og"),
            # 4. a page missing from the sitemap: the home page's route is marked noindex.
            #    (In molde the sitemap is generated from the routes, so a route flag is the only
            #    way to leave a page out; `molde check` treats that as intended, by design. It is
            #    on the home page so it masks no other plant: plant 3 is on the stock pages.)
            ("routes.yaml", '    title: "Demo — dividend stocks"\n', '    title: "Demo — dividend stocks"\n    index: false\n', "R3_sitemap"),
            # 5. double escaping of the stock name: an & is escaped by hand before autoescape
            ("templates/stock.html.j2", "<h1>{{ item.name }} <small>", '<h1>{{ item.name | replace("&", "&amp;") }} <small>', "R6_escaping"),
        ],
        # T4b: defects that molde's checks do not see (reference.md, "What the checks do not
        # see", as of 89954dc): single-quoted head tags, and the words themselves.
        "T4b": [
            # 1. a second canonical, single-quoted, pointing elsewhere
            ("templates/stock.html.j2", "{{ std_head }}", "{{ std_head }}\n<link rel='canonical' href='https://demo.example/stocks/'>", "R5_canonical_og"),
            # 2. an extra single-quoted og:title on the home page that contradicts its title
            ("templates/index.html.j2", "{{ std_head }}", "{{ std_head }}\n<meta property='og:title' content='Dividend calendar'>", "R5_canonical_og"),
            # 3. the badge shows on every upcoming dividend, confirmed or not
            ("templates/stock.html.j2", "{% if not item.upcoming.confirmed %}<p class=\"badge\">", "{% if true %}<p class=\"badge\">", "task_badge"),
            # 4. a stale hard-coded string: the home page heading names a fixed cycle
            ("templates/index.html.j2", "<h1>Stocks (cycle {{ data.cycle_id }})</h1>", "<h1>Stocks (cycle 2026-09-11T09:00:00Z-demo)</h1>", "task_cycle"),
            # 5. a hand-edited value: one stock's amount is written into the template
            ("templates/stock.html.j2", "<p>Next dividend: {{ item.upcoming.amount }}", '<p>Next dividend: {{ "1.3500" if item.ticker == "PEP" else item.upcoming.amount }}', "task_amount"),
        ],
    },
    # The idiomatic ports (frozen from the factory runs, 16 Oct). The same categories as
    # molde's plants, each written the way that framework's code would carry the defect.
    # T4b's head plants are the same defect (a second canonical pointing elsewhere, an
    # og:title contradicting the title); they are single-quoted where the framework can
    # emit raw HTML, and only molde's checks are blind to the quoting.
    "idiomatic-hugo": {
        "T4": [
            ("layouts/home.html", '<li><a href="{{ .RelPermalink }}">', '<li><a href="{{ .RelPermalink }}x/">', "R2_links"),
            ("content/stocks/_content.gotmpl", '"title" (printf "%s (%s)" $stock.name $stock.ticker)', '"title" "Stock page"', "R4_titles"),
            ("layouts/_partials/head.html", '<link rel="canonical" href="{{ .Permalink }}">',
             '{{ if .IsHome }}<link rel="canonical" href="{{ .Permalink }}">{{ end }}', "R5_canonical_og"),
            ("layouts/_partials/head.html", '<meta property="og:url" content="{{ .Permalink }}">',
             '<meta property="og:url" content="{{ site.Home.Permalink }}">', "R5_canonical_og"),
            ("content/_index.md", None, '---\ntitle: "Demo — dividend stocks"\nsitemap:\n  disable: true\n---\n', "R3_sitemap"),
            ("layouts/page.html", "<h1>{{ .Params.name }} <small>", '<h1>{{ .Params.name | replaceRE "&" "&amp;" }} <small>', "R6_escaping"),
        ],
        "T4b": [
            ("layouts/_partials/head.html", '<link rel="canonical" href="{{ .Permalink }}">',
             '<link rel="canonical" href="{{ .Permalink }}">\n{{ if not .IsHome }}<link rel=\'canonical\' href=\'https://demo.example/stocks/\'>{{ end }}', "R5_canonical_og"),
            ("layouts/_partials/head.html", '<meta property="og:url" content="{{ .Permalink }}">',
             '<meta property="og:url" content="{{ .Permalink }}">\n{{ if .IsHome }}<meta property=\'og:title\' content=\'Dividend calendar\'>{{ end }}', "R5_canonical_og"),
            ("layouts/page.html", '{{ if not .confirmed }}<span class="badge">', '{{ if true }}<span class="badge">', "task_badge"),
            ("layouts/home.html", "<h1>Stocks (cycle {{ hugo.Data.stocks.cycle_id }})</h1>", "<h1>Stocks (cycle 2026-09-11T09:00:00Z-demo)</h1>", "task_cycle"),
            ("layouts/page.html", "Next dividend: {{ .amount }}", 'Next dividend: {{ if eq $.Params.ticker "PEP" }}1.3500{{ else }}{{ .amount }}{{ end }}', "task_amount"),
        ],
    },
    "idiomatic-astro": {
        "T4": [
            ("src/pages/index.astro", "<a href={stockPath(stock)}>", "<a href={stockPath(stock) + 'x/'}>", "R2_links"),
            ("src/pages/stocks/[ticker].astro", "<BaseLayout title={`${stock.name} (${stock.ticker})`}>", '<BaseLayout title="Stock page">', "R4_titles"),
            ("src/layouts/BaseLayout.astro", "<link rel=\"canonical\" href={canonicalURL} />",
             "{Astro.url.pathname === '/' && <link rel=\"canonical\" href={canonicalURL} />}", "R5_canonical_og"),
            ("src/layouts/BaseLayout.astro", "<meta property=\"og:url\" content={canonicalURL} />",
             "<meta property=\"og:url\" content={new URL('/', Astro.site)} />", "R5_canonical_og"),
            ("src/pages/sitemap.xml.ts", "const paths = ['/', ...stocks.map(stockPath)];", "const paths = [...stocks.map(stockPath)];", "R3_sitemap"),
            ("src/pages/stocks/[ticker].astro", "<h1>{stock.name} <small>", "<h1>{stock.name.replaceAll('&', '&amp;')} <small>", "R6_escaping"),
        ],
        "T4b": [
            ("src/layouts/BaseLayout.astro", "<link rel=\"canonical\" href={canonicalURL} />",
             "<link rel=\"canonical\" href={canonicalURL} />\n    {Astro.url.pathname !== '/' && <Fragment set:html=\"<link rel='canonical' href='https://demo.example/stocks/'>\" />}", "R5_canonical_og"),
            ("src/layouts/BaseLayout.astro", "<meta property=\"og:url\" content={canonicalURL} />",
             "<meta property=\"og:url\" content={canonicalURL} />\n    {Astro.url.pathname === '/' && <Fragment set:html=\"<meta property='og:title' content='Dividend calendar'>\" />}", "R5_canonical_og"),
            ("src/pages/stocks/[ticker].astro", '{!upcoming.confirmed && <span class="badge">', '{true && <span class="badge">', "task_badge"),
            ("src/pages/index.astro", "<h1>Stocks (cycle {cycleId})</h1>", "<h1>Stocks (cycle 2026-09-11T09:00:00Z-demo)</h1>", "task_cycle"),
            ("src/pages/stocks/[ticker].astro", "Next dividend: {upcoming.amount}", "Next dividend: {stock.ticker === 'PEP' ? '1.3500' : upcoming.amount}", "task_amount"),
        ],
    },
    "idiomatic-nextjs": {
        "T4": [
            ("app/page.tsx", "<Link href={stockPath(stock.ticker)}>", '<Link href={stockPath(stock.ticker) + "x/"}>', "R2_links"),
            ("app/stocks/[ticker]/page.tsx", "const title = `${stock.name} (${stock.ticker})`;", 'const title = "Stock page";', "R4_titles"),
            ("app/stocks/[ticker]/page.tsx", '    alternates: { canonical: url },\n    openGraph: { title, url, type: "website" },',
             '    openGraph: { title, url: "/", type: "website" },', "R5_canonical_og"),
            ("app/sitemap.ts", "    { url: `${siteUrl}/` },\n", "", "R3_sitemap"),
            ("app/stocks/[ticker]/page.tsx", "{stock.name} <small>", '{stock.name.replaceAll("&", "&amp;")} <small>', "R6_escaping"),
        ],
        # React writes double-quoted attributes; React 19 hoists a <link> or <meta> rendered
        # in a page into the head.
        "T4b": [
            ("app/stocks/[ticker]/page.tsx", "    <main>\n      <h1>",
             '    <main>\n      <link rel="canonical" href="https://demo.example/stocks/" />\n      <h1>', "R5_canonical_og"),
            ("app/page.tsx", "    <main>\n      <h1>Stocks (cycle {cycleId})</h1>",
             '    <main>\n      <meta property="og:title" content="Dividend calendar" />\n      <h1>Stocks (cycle {cycleId})</h1>', "R5_canonical_og"),
            ("app/stocks/[ticker]/page.tsx", "{!upcoming.confirmed && (", "{true && (", "task_badge"),
            ("app/page.tsx", "<h1>Stocks (cycle {cycleId})</h1>", "<h1>Stocks (cycle 2026-09-11T09:00:00Z-demo)</h1>", "task_cycle"),
            ("app/stocks/[ticker]/page.tsx", "Next dividend: {upcoming.amount}", 'Next dividend: {stock.ticker === "PEP" ? "1.3500" : upcoming.amount}', "task_amount"),
        ],
    },
    # The parity ports (the frozen cross-framework builds): the same categories, each written
    # the way that port's files would carry the defect. Every removal plant keeps neighbouring
    # text in `new`, so undo() can invert it by swapping.
    "parity-hugo": {
        "T4": [
            ("layouts/home.html", '<a href="/stocks/{{ .ticker }}/">', '<a href="/stocks/{{ .ticker }}x/">', "R2_links"),
            ("content/stocks/_content.gotmpl", '"title" (printf "%s (%s)" .name .ticker)', '"title" "Stock page"', "R4_titles"),
            ("layouts/page.html",
             '<link rel="canonical" href="{{ .Permalink }}">\n<meta property="og:title" content="{{ .Title }}">\n<meta property="og:url" content="{{ .Permalink }}">',
             '<meta property="og:title" content="{{ .Title }}">\n<meta property="og:url" content="https://demo.example/">', "R5_canonical_og"),
            ("layouts/sitemap.xml", '  <url><loc>{{ site.BaseURL }}</loc></url>\n{{- range', '{{- range', "R3_sitemap"),
            ("layouts/page.html", "<h1>{{ .Params.stock.name }} <small>", '<h1>{{ .Params.stock.name | replaceRE "&" "&amp;" }} <small>', "R6_escaping"),
        ],
        "T4b": [
            ("layouts/page.html", '<link rel="canonical" href="{{ .Permalink }}">',
             '<link rel="canonical" href="{{ .Permalink }}">\n<link rel=\'canonical\' href=\'https://demo.example/stocks/\'>', "R5_canonical_og"),
            ("layouts/home.html", '<meta property="og:url" content="{{ .Permalink }}">',
             '<meta property="og:url" content="{{ .Permalink }}">\n<meta property=\'og:title\' content=\'Dividend calendar\'>', "R5_canonical_og"),
            ("layouts/page.html", '{{ if not .confirmed }}<p class="badge">', '{{ if true }}<p class="badge">', "task_badge"),
            ("layouts/home.html", "<h1>Stocks (cycle {{ hugo.Data.stocks.cycle_id }})</h1>", "<h1>Stocks (cycle 2026-09-11T09:00:00Z-demo)</h1>", "task_cycle"),
            ("layouts/page.html", "Next dividend: {{ .amount }}", 'Next dividend: {{ if eq $.Params.stock.ticker "PEP" }}1.3500{{ else }}{{ .amount }}{{ end }}', "task_amount"),
        ],
    },
    "parity-zola": {
        "T4": [
            ("templates/index.html", '<a href="/stocks/{{ s.ticker }}/">', '<a href="/stocks/{{ s.ticker }}x/">', "R2_links"),
            ("gen_content.py", """f'title = {json.dumps(s["name"] + " (" + s["ticker"] + ")")}',""", """'title = "Stock page"',""", "R4_titles"),
            ("templates/stock.html",
             '<link rel="canonical" href="{{ page.permalink | safe }}">\n<meta property="og:title" content="{{ page.title }}">\n<meta property="og:url" content="{{ page.permalink | safe }}">',
             '<meta property="og:title" content="{{ page.title }}">\n<meta property="og:url" content="https://demo.example/">', "R5_canonical_og"),
            ("templates/sitemap.xml", "{%- for sitemap_entry in entries %}\n  <url><loc>{{ sitemap_entry.permalink | safe }}</loc></url>\n{%- endfor %}",
             '{%- for sitemap_entry in entries %}\n{%- if sitemap_entry.permalink != "https://demo.example/" %}\n  <url><loc>{{ sitemap_entry.permalink | safe }}</loc></url>\n{%- endif %}\n{%- endfor %}',
             "R3_sitemap"),
            ("templates/stock.html", "<h1>{{ page.extra.name }} <small>", '<h1>{{ page.extra.name | replace(from="&", to="&amp;") }} <small>', "R6_escaping"),
        ],
        "T4b": [
            ("templates/stock.html", '<link rel="canonical" href="{{ page.permalink | safe }}">',
             '<link rel="canonical" href="{{ page.permalink | safe }}">\n<link rel=\'canonical\' href=\'https://demo.example/stocks/\'>', "R5_canonical_og"),
            ("templates/index.html", '<meta property="og:url" content="{{ section.permalink | safe }}">',
             '<meta property="og:url" content="{{ section.permalink | safe }}">\n<meta property=\'og:title\' content=\'Dividend calendar\'>', "R5_canonical_og"),
            ("templates/stock.html", '{% if not page.extra.upcoming.confirmed %}<p class="badge">', '{% if true %}<p class="badge">', "task_badge"),
            ("templates/index.html", "<h1>Stocks (cycle {{ data.cycle_id }})</h1>", "<h1>Stocks (cycle 2026-09-11T09:00:00Z-demo)</h1>", "task_cycle"),
            ("templates/stock.html", "Next dividend: {{ page.extra.upcoming.amount }}",
             'Next dividend: {% if page.extra.ticker == "PEP" %}1.3500{% else %}{{ page.extra.upcoming.amount }}{% endif %}', "task_amount"),
        ],
    },
    "parity-eleventy": {
        "T4": [
            ("src/index.njk", '<a href="/stocks/{{ s.ticker }}/">', '<a href="/stocks/{{ s.ticker }}x/">', "R2_links"),
            ("src/stocks.njk", "<title>{{ s.name }} ({{ s.ticker }})</title>", "<title>Stock page</title>", "R4_titles"),
            ("src/stocks.njk", '<meta property="og:title" content="{{ s.name }} ({{ s.ticker }})">', '<meta property="og:title" content="Stock page">', "R4_titles"),
            ("src/stocks.njk",
             '<link rel="canonical" href="https://demo.example/stocks/{{ s.ticker }}/">\n<meta property="og:title" content="Stock page">\n<meta property="og:url" content="https://demo.example/stocks/{{ s.ticker }}/">',
             '<meta property="og:title" content="Stock page">\n<meta property="og:url" content="https://demo.example/">', "R5_canonical_og"),
            ("eleventy.config.js", """      "  <url><loc>https://demo.example/</loc></url>\\n" +\n      "{% for s""", """      "{% for s""", "R3_sitemap"),
            ("src/stocks.njk", "<h1>{{ s.name }} <small>", '<h1>{{ s.name | replace("&", "&amp;") }} <small>', "R6_escaping"),
        ],
        "T4b": [
            ("src/stocks.njk", '<link rel="canonical" href="https://demo.example/stocks/{{ s.ticker }}/">',
             '<link rel="canonical" href="https://demo.example/stocks/{{ s.ticker }}/">\n<link rel=\'canonical\' href=\'https://demo.example/stocks/\'>', "R5_canonical_og"),
            ("src/index.njk", '<meta property="og:url" content="https://demo.example/">',
             '<meta property="og:url" content="https://demo.example/">\n<meta property=\'og:title\' content=\'Dividend calendar\'>', "R5_canonical_og"),
            ("src/stocks.njk", '{% if not s.upcoming.confirmed %}<p class="badge">', '{% if true %}<p class="badge">', "task_badge"),
            ("src/index.njk", "<h1>Stocks (cycle {{ demo.cycle_id }})</h1>", "<h1>Stocks (cycle 2026-09-11T09:00:00Z-demo)</h1>", "task_cycle"),
            ("src/stocks.njk", "Next dividend: {{ s.upcoming.amount }}",
             'Next dividend: {% if s.ticker == "PEP" %}1.3500{% else %}{{ s.upcoming.amount }}{% endif %}', "task_amount"),
        ],
    },
    "parity-astro": {
        "T4": [
            ("src/pages/index.astro", '<li><a href="/stocks/${s.ticker}/">', '<li><a href="/stocks/${s.ticker}x/">', "R2_links"),
            ("src/pages/stocks/[ticker].astro", "const title = `${stock.name} (${stock.ticker})`;", "const title = 'Stock page';", "R4_titles"),
            ("src/pages/stocks/[ticker].astro",
             '<link rel="canonical" href={url}>\n<meta property="og:title" content={title}>\n<meta property="og:url" content={url}>',
             '<meta property="og:title" content={title}>\n<meta property="og:url" content="https://demo.example/">', "R5_canonical_og"),
            ("src/pages/sitemap.xml.js", "const paths = ['/', ...data.stocks", "const paths = [...data.stocks", "R3_sitemap"),
            ("src/pages/stocks/[ticker].astro", "<h1>{stock.name} <small>", "<h1>{stock.name.replaceAll('&', '&amp;')} <small>", "R6_escaping"),
        ],
        # Astro prints attributes double-quoted, so the single-quoted tags go in through set:html.
        "T4b": [
            ("src/pages/stocks/[ticker].astro", '<link rel="canonical" href={url}>',
             "<link rel=\"canonical\" href={url}>\n<Fragment set:html=\"<link rel='canonical' href='https://demo.example/stocks/'>\" />", "R5_canonical_og"),
            ("src/pages/index.astro", '<meta property="og:url" content="https://demo.example/">',
             "<meta property=\"og:url\" content=\"https://demo.example/\">\n<Fragment set:html=\"<meta property='og:title' content='Dividend calendar'>\" />", "R5_canonical_og"),
            ("src/pages/stocks/[ticker].astro", """${u.confirmed ? '' : '<p class="badge">Estimated — not yet declared</p>'}""",
             """${true ? '<p class="badge">Estimated — not yet declared</p>' : ''}""", "task_badge"),
            ("src/pages/index.astro", "<h1>Stocks (cycle {data.cycle_id})</h1>", "<h1>Stocks (cycle 2026-09-11T09:00:00Z-demo)</h1>", "task_cycle"),
            ("src/pages/stocks/[ticker].astro", "Next dividend: ${u.amount}", "Next dividend: ${stock.ticker === 'PEP' ? '1.3500' : u.amount}", "task_amount"),
        ],
    },
    "parity-nextjs": {
        "T4": [
            ("pages/index.js", '<li><a href="/stocks/${s.ticker}/">', '<li><a href="/stocks/${s.ticker}x/">', "R2_links"),
            ("pages/stocks/[ticker].js", "const title = `${stock.name} (${stock.ticker})`;", "const title = 'Stock page';", "R4_titles"),
            ("pages/stocks/[ticker].js",
             '<link rel="canonical" href={url} />\n        <meta property="og:title" content={title} />\n        <meta property="og:url" content={url} />',
             '<meta property="og:title" content={title} />\n        <meta property="og:url" content={`${BASE_URL}/`} />', "R5_canonical_og"),
            ("scripts/prebuild.js", "const locs = ['/', ...data.stocks", "const locs = [...data.stocks", "R3_sitemap"),
            ("pages/stocks/[ticker].js", "<h1>${esc(stock.name)} <small>", "<h1>${esc(stock.name).replace(/&/g, '&amp;')} <small>", "R6_escaping"),
        ],
        # The single-quoted tags are written into the page body string (React's Head prints double quotes).
        "T4b": [
            ("pages/stocks/[ticker].js", "    `\\n<h1>${esc(stock.name)} <small>",
             "    `\\n<link rel='canonical' href='https://demo.example/stocks/'>\\n<h1>${esc(stock.name)} <small>", "R5_canonical_og"),
            ("pages/index.js", "`\\n<h1>Stocks (cycle ${esc(data.cycle_id)})</h1>",
             "`\\n<meta property='og:title' content='Dividend calendar'>\\n<h1>Stocks (cycle ${esc(data.cycle_id)})</h1>", "R5_canonical_og"),
            ("pages/stocks/[ticker].js", "${u.confirmed ? '' : '<p class=\"badge\">Estimated — not yet declared</p>'}",
             "${true ? '<p class=\"badge\">Estimated — not yet declared</p>' : ''}", "task_badge"),
            ("pages/index.js", "<h1>Stocks (cycle ${esc(data.cycle_id)})</h1>", "<h1>Stocks (cycle 2026-09-11T09:00:00Z-demo)</h1>", "task_cycle"),
            ("pages/stocks/[ticker].js", "function upcomingHtml(u) {", "function upcomingHtml(u, stock) {", "task_amount"),
            ("pages/stocks/[ticker].js", "upcomingHtml(stock.upcoming)", "upcomingHtml(stock.upcoming, stock)", "task_amount"),
            ("pages/stocks/[ticker].js", "Next dividend: ${esc(u.amount)}", "Next dividend: ${stock.ticker === 'PEP' ? '1.3500' : esc(u.amount)}", "task_amount"),
        ],
    },
}


def apply_edits(ws, edits):
    ws = Path(ws)
    for rel, old, new, _cat in edits:
        p = ws / rel
        if old is None:                      # create a new file
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(new)
            continue
        s = p.read_text()
        n = s.count(old)
        if n != 1:
            raise SystemExit(f"plant anchor matches {n} times (needs exactly 1) in {rel}: {old!r}")
        p.write_text(s.replace(old, new))


def setup(port, task, ws, data_rel):
    ws = Path(ws)
    dpath = ws / data_rel
    data = json.loads(dpath.read_text())
    if task in DATA_CHANGES:
        data = DATA_CHANGES[task](data)
    dpath.write_text(json.dumps(data, indent=1, ensure_ascii=False) + "\n")
    if task in ("T4", "T4b"):
        edits = PLANTS.get(port, {}).get(task)
        if edits is None:
            raise SystemExit(f"no {task} plants defined for port {port}")
        apply_edits(ws, edits)
    return sorted({c for *_x, c in PLANTS.get(port, {}).get(task, [])})
