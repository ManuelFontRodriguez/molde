"""Reference solutions for the molde port, one per task: a known-correct answer that the
judge must accept. They prove each task is feasible in molde and that the oracle does
not reject correct work (task validation, the positive half). They are judge-side
material and never reach an agent.

Each task: a list of (file, old, new) exact replacements (old=None creates or replaces
the whole file), applied after runner/setup.py.
"""

T0_FILES = {
    "routes.yaml": """routes:
  - path: /index.html
    template: index.html.j2
    title: "Demo — dividend stocks"
  - path: /stocks/{ticker}/index.html
    template: stock.html.j2
    foreach: stocks
    title: "{{ item.name }} ({{ item.ticker }})"
""",
    "site.yaml": "base_url: https://demo.example\n",
    "templates/index.html.j2": """<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8">
<link rel="stylesheet" href="/style.css">
{{ std_head }}
</head>
<body>
<h1>Stocks (cycle {{ data.cycle_id }})</h1>
<ul>
{% for s in data.stocks %}  <li><a href="/stocks/{{ s.ticker }}/">{{ s.name }}</a> — {{ s.sector }}</li>
{% endfor %}</ul>
</body>
</html>
""",
    "templates/stock.html.j2": """<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8">
<link rel="stylesheet" href="/style.css">
{{ std_head }}
</head>
<body>
<h1>{{ item.name }} <small>{{ item.ticker }}</small></h1>
<p>{{ item.sector }}</p>
{% if item.upcoming %}<p>Next dividend: {{ item.upcoming.amount }} {{ item.upcoming.currency }} — ex {{ item.upcoming.ex_date }}</p>
{% if not item.upcoming.confirmed %}<p class="badge">Estimated — not yet declared</p>{% endif %}
{% else %}<p>No upcoming dividend declared.</p>{% endif %}
<p><a href="/">← all stocks</a></p>
</body>
</html>
""",
}

# Tasks molde cannot pass, with the reason. The judge must reject these reference
# solutions; verify_solutions.py checks that it does, and they are pre-registered as
# expected failures for the molde arms.
# History: T2 was listed on 15 Oct (no grouping, then group_by without a slug: "Consumer
# Defensive" has a space, refused in a path). molde 9badb4e added `{field|slug}` in paths
# and the `slug` filter, so T2's solution below now passes and T2 is no longer listed.
EXPECTED_FAIL = {
    "T10-overedit": "negative control: changes visible content when nothing needed changing",
}

SOLUTIONS = {
    "T0": [(k, None, v) for k, v in T0_FILES.items()] + [("static/style.css", None, "@MOVE style.css")],
    "T1": [
        ("templates/stock.html.j2", "<p>{{ item.sector }}</p>", "<p>{{ item.sector }}</p>\n<p>Dividend yield: {{ item.dividend_yield }}</p>"),
        ("templates/index.html.j2", "— {{ s.sector }}</li>", "— {{ s.sector }} — yield {{ s.dividend_yield }}</li>"),
    ],
    "T2": [
        ("routes.yaml", None, """routes:
  - path: /index.html
    template: index.html.j2
    title: "Demo — dividend stocks"
  - path: /stocks/{ticker}/index.html
    template: stock.html.j2
    foreach: stocks
    title: "{{ item.name }} ({{ item.ticker }})"
  - path: /sectors/{sector|slug}/index.html
    template: sector.html.j2
    foreach: stocks
    group_by: sector
    title: "{{ item.sector }} stocks"
"""),
        ("templates/sector.html.j2", None, """<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8">
<link rel="stylesheet" href="/style.css">
{{ std_head }}
</head>
<body>
<h1>{{ item.sector }}</h1>
<ul>
{% for s in item.items %}  <li><a href="/stocks/{{ s.ticker }}/">{{ s.name }}</a></li>
{% endfor %}</ul>
<p><a href="/">← all stocks</a></p>
</body>
</html>
"""),
        ("templates/stock.html.j2", "<p>{{ item.sector }}</p>",
         '<p><a href="/sectors/{{ item.sector | slug }}/">{{ item.sector }}</a></p>'),
    ],
    "T3": [
        ("routes.yaml", "path: /stocks/{ticker}/index.html", "path: /acciones/{ticker}/index.html"),
        ("templates/index.html.j2", 'href="/stocks/{{ s.ticker }}/"', 'href="/acciones/{{ s.ticker }}/"'),
    ],
    "T4": [   # undo each plant (inverse-plant solutions: they prove the clean state passes)
        ("templates/index.html.j2", 'href="/stocks/{{ s.ticker }}x/"', 'href="/stocks/{{ s.ticker }}/"'),
        ("routes.yaml", 'title: "Stock page"', 'title: "{{ item.name }} ({{ item.ticker }})"'),
        ("templates/stock.html.j2",
         '<title>Stock page</title>\n<meta property="og:title" content="Stock page">\n'
         '<meta property="og:url" content="https://demo.example/">\n<meta property="og:type" content="website">',
         "{{ std_head }}"),
        ("routes.yaml", '    title: "Demo — dividend stocks"\n    index: false\n', '    title: "Demo — dividend stocks"\n'),
        ("templates/stock.html.j2", '<h1>{{ item.name | replace("&", "&amp;") }} <small>', "<h1>{{ item.name }} <small>"),
    ],
    "T4b": [
        ("templates/stock.html.j2", "{{ std_head }}\n<link rel='canonical' href='https://demo.example/stocks/'>", "{{ std_head }}"),
        ("templates/index.html.j2", "{{ std_head }}\n<meta property='og:title' content='Dividend calendar'>", "{{ std_head }}"),
        ("templates/stock.html.j2", "{% if true %}<p class=\"badge\">", "{% if not item.upcoming.confirmed %}<p class=\"badge\">"),
        ("templates/index.html.j2", "<h1>Stocks (cycle 2026-09-11T09:00:00Z-demo)</h1>", "<h1>Stocks (cycle {{ data.cycle_id }})</h1>"),
        ("templates/stock.html.j2", '<p>Next dividend: {{ "1.3500" if item.ticker == "PEP" else item.upcoming.amount }}', "<p>Next dividend: {{ item.upcoming.amount }}"),
    ],
    "T5": [
        ("routes.yaml", '    title: "Demo — dividend stocks"', '    title: "Demo — dividend stocks"\n    description: "Upcoming dividends for every stock we track."'),
        ("routes.yaml", '    title: "{{ item.name }} ({{ item.ticker }})"', '    title: "{{ item.name }} ({{ item.ticker }})"\n    description: "{{ item.name }} ({{ item.ticker }}): {{ item.sector }}."'),
    ],
    "T6": [
        ("routes.yaml", '    template: index.html.j2\n    title: "Demo — dividend stocks"',
         '    template: index.html.j2\n    paginate:\n      over: stocks\n      per_page: 2\n      path: "/page/{n}/index.html"\n'
         '    title: "Demo — dividend stocks{% if pagination.n > 1 %} (page {{ pagination.n }}){% endif %}"'),
        ("templates/index.html.j2", "{% for s in data.stocks %}", "{% for s in pagination.items %}"),
        ("templates/index.html.j2", "</ul>",
         '</ul>\n{% if pagination.prev %}<a href="{{ pagination.prev }}">previous</a>{% endif %}\n'
         '{% if pagination.next %}<a href="{{ pagination.next }}">next</a>{% endif %}'),
    ],
    "T7": [
        ("templates/stock.html.j2", "{{ std_head }}",
         '{{ std_head }}\n<script type="application/ld+json">{{ {"@context": "https://schema.org", "@type": "Corporation", '
         '"name": item.name, "tickerSymbol": item.ticker} | tojson }}</script>'),
    ],
    "T8": [
        ("static/style.css", "border-radius: 0.25rem; background: #fce8b2; }",
         "border-radius: 0.25rem; background: #fce8b2; color: red; font-weight: bold; }"),
    ],
    "T9": [
        ("templates/index.html.j2", "<ul>",
         '<input id="q" type="search" placeholder="Search by name or ticker">\n<ul id="stocks">'),
        ("templates/index.html.j2", '<li><a href="/stocks/{{ s.ticker }}/">',
         '<li data-q="{{ (s.name ~ " " ~ s.ticker) | lower }}"><a href="/stocks/{{ s.ticker }}/">'),
        ("templates/index.html.j2", "</body>",
         "<script>\ndocument.getElementById('q').addEventListener('input', e => {\n"
         "  const q = e.target.value.trim().toLowerCase();\n"
         "  for (const li of document.querySelectorAll('#stocks li')) li.hidden = q && !li.dataset.q.includes(q);\n"
         "});\n</script>\n</body>"),
    ],
    "T11": [
        ("templates/index.html.j2", "{% for s in data.stocks %}", "{% for s in data.stocks | sort(attribute=\"name\") %}"),
    ],
    "T12": [
        ("templates/index.html.j2", "<ul>",
         '<p id="sectors">{% for g in data.stocks | groupby("sector") %}'
         '<button type="button" data-sector="{{ g.grouper }}">{{ g.grouper }}</button> {% endfor %}</p>\n<ul id="stocks">'),
        ("templates/index.html.j2", '<li><a href="/stocks/{{ s.ticker }}/">',
         '<li data-sector="{{ s.sector }}"><a href="/stocks/{{ s.ticker }}/">'),
        ("templates/index.html.j2", "</body>",
         "<script>\nfor (const b of document.querySelectorAll('#sectors button')) {\n"
         "  b.addEventListener('click', () => {\n"
         "    for (const li of document.querySelectorAll('#stocks li')) li.hidden = li.dataset.sector !== b.dataset.sector;\n"
         "  });\n}\n</script>\n</body>"),
    ],
    "T10": [],
    # negative control for T10: an edit that changes visible content must fail T10
    "T10-overedit": [("templates/stock.html.j2", "<p>{{ item.sector }}</p>", "<p>Sector: {{ item.sector }}</p>")],
}
