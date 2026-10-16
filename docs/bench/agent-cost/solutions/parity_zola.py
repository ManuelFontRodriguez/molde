"""Reference solutions for the parity Zola port (judge-side only; see molde.py).

Zola has no pages-from-data feature, so the port's gen_content.py (run by every build)
writes the content files; the solutions extend that generator where a task needs more
pages (T6) or front matter (T2: a "sectors" taxonomy, Zola's own way to group pages).
T4/T4b: inverse of the plants in runner/setup.py (judge-side).
"""

import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from _idiomatic import undo  # noqa: E402

EXPECTED_FAIL = {"T10-overedit": "negative control: changes visible content when nothing needed changing"}

HEAD = """<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8">
<link rel="stylesheet" href="/style.css">
<title>{title}</title>
<link rel="canonical" href="{url}">
<meta property="og:title" content="{title}">
<meta property="og:url" content="{url}">
<meta property="og:type" content="website">
</head>
<body>
"""

TAXONOMY_LIST = HEAD.format(title="Sectors", url="{{ current_url | safe }}") + """<h1>Sectors</h1>
<ul>
{% for term in terms %}
  <li><a href="{{ term.permalink | safe }}">{{ term.name }}</a></li>
{% endfor %}
</ul>
<p><a href="/">← all stocks</a></p>
</body>
</html>
"""

TAXONOMY_SINGLE = HEAD.format(title="{{ term.name }} stocks", url="{{ term.permalink | safe }}") + """<h1>{{ term.name }}</h1>
<ul>
{% for p in term.pages %}
  <li><a href="{{ p.permalink | safe }}">{{ p.extra.name }}</a></li>
{% endfor %}
</ul>
<p><a href="/">← all stocks</a></p>
</body>
</html>
"""

INDEX_PAGINATED = """{% set cur = section | default(value=page) -%}
{% set data = load_data(path="data.json") -%}
{% set n = cur.extra.n | default(value=1) -%}
{% set half = ((data.stocks | length) + 1) / 2 -%}
{% set last = half | round(method="floor") | int -%}
<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8">
<link rel="stylesheet" href="/style.css">
<title>{{ cur.title }}</title>
<link rel="canonical" href="{{ cur.permalink | safe }}">
<meta property="og:title" content="{{ cur.title }}">
<meta property="og:url" content="{{ cur.permalink | safe }}">
<meta property="og:type" content="website">
</head>
<body>
<h1>Stocks (cycle {{ data.cycle_id }})</h1>
<ul>
{% for s in data.stocks %}{% if loop.index0 >= (n - 1) * 2 and loop.index0 < n * 2 %}
  <li><a href="/stocks/{{ s.ticker }}/">{{ s.name }}</a> — {{ s.sector }}</li>
{% endif %}{% endfor %}
</ul>
{% if n > 1 %}<a href="{% if n == 2 %}/{% else %}/page/{{ n - 1 }}/{% endif %}">previous</a>{% endif %}
{% if n < last %}<a href="/page/{{ n + 1 }}/">next</a>{% endif %}
</body>
</html>
"""

GEN_PAGES = '''
# Pagination of the home page: 2 stocks per page; page 1 is the home page itself.
pages_dir = site / "content" / "page"
shutil.rmtree(pages_dir, ignore_errors=True)
pages_dir.mkdir(parents=True)
(pages_dir / "_index.md").write_text("+++\\nrender = false\\n+++\\n")
for n in range(2, (len(data["stocks"]) + 1) // 2 + 1):
    (pages_dir / str(n)).mkdir()
    (pages_dir / str(n) / "index.md").write_text(
        "+++\\n"
        f'title = "Demo — dividend stocks (page {n})"\\n'
        f'path = "page/{n}"\\n'
        'template = "index.html"\\n'
        f"[extra]\\nn = {n}\\n"
        "+++\\n"
    )
'''

SOLUTIONS = {
    "T1": [
        ("templates/stock.html", "<p>{{ page.extra.sector }}</p>",
         "<p>{{ page.extra.sector }}</p>\n<p>Dividend yield: {{ page.extra.dividend_yield }}</p>"),
        ("templates/index.html", "— {{ s.sector }}</li>", "— {{ s.sector }} — yield {{ s.dividend_yield }}</li>"),
        ("gen_content.py", """        f'sector = {json.dumps(s["sector"])}',\n""",
         """        f'sector = {json.dumps(s["sector"])}',\n        f'dividend_yield = {json.dumps(s["dividend_yield"])}',\n"""),
    ],
    "T2": [
        ("config.toml", "generate_feeds = false", 'generate_feeds = false\ntaxonomies = [{ name = "sectors" }]'),
        ("gen_content.py", """        'template = "stock.html"',\n""",
         """        'template = "stock.html"',\n        "[taxonomies]",\n        f'sectors = [{json.dumps(s["sector"])}]',\n"""),
        ("templates/sectors/list.html", None, TAXONOMY_LIST),
        ("templates/sectors/single.html", None, TAXONOMY_SINGLE),
        ("templates/stock.html", "<p>{{ page.extra.sector }}</p>",
         '<p><a href="{{ get_taxonomy_url(kind="sectors", name=page.extra.sector) | safe }}">{{ page.extra.sector }}</a></p>'),
    ],
    "T3": [
        ("gen_content.py", """f'path = "stocks/{s["ticker"]}"'""", """f'path = "acciones/{s["ticker"]}"'"""),
        ("templates/index.html", 'href="/stocks/{{ s.ticker }}/"', 'href="/acciones/{{ s.ticker }}/"'),
    ],
    "T5": [
        ("templates/index.html", '<meta property="og:type" content="website">',
         '<meta property="og:type" content="website">\n<meta property="og:description" content="Upcoming dividends for every stock we track.">'),
        ("templates/stock.html", '<meta property="og:type" content="website">',
         '<meta property="og:type" content="website">\n<meta property="og:description" content="{{ page.extra.name }} ({{ page.extra.ticker }}): {{ page.extra.sector }}.">'),
    ],
    "T6": [
        ("templates/index.html", None, INDEX_PAGINATED),
        ("gen_content.py", "\nfor s in data[\"stocks\"]:", GEN_PAGES + "\nfor s in data[\"stocks\"]:"),
    ],
    "T11": [
        # Tera's sort is case-sensitive, so rank the pages in the generator and let the section sort by weight.
        ("gen_content.py", '(stocks_dir / "_index.md").write_text("+++\\nrender = false\\n+++\\n")',
         '(stocks_dir / "_index.md").write_text("+++\\nrender = false\\nsort_by = \\"weight\\"\\n+++\\n")\n'
         'rank = {s["ticker"]: n for n, s in enumerate(sorted(data["stocks"], key=lambda s: s["name"].casefold()), 1)}'),
        ("gen_content.py", """        'template = "stock.html"',\n""",
         """        'template = "stock.html"',\n        f'weight = {rank[s["ticker"]]}',\n"""),
        ("templates/index.html", "{% for s in data.stocks %}\n  <li><a href=\"/stocks/{{ s.ticker }}/\">{{ s.name }}</a> — {{ s.sector }}</li>",
         "{% set stocks = get_section(path=\"stocks/_index.md\") -%}\n{% for p in stocks.pages %}\n  <li><a href=\"/stocks/{{ p.extra.ticker }}/\">{{ p.extra.name }}</a> — {{ p.extra.sector }}</li>"),
    ],
    "T12": [
        ("templates/index.html", "<ul>\n{% for s in data.stocks %}\n  <li>",
         "<p>{% for sector, _ in data.stocks | group_by(attribute=\"sector\") %}<button type=\"button\" data-sector=\"{{ sector }}\">{{ sector }}</button> {% endfor %}</p>\n"
         "<ul>\n{% for s in data.stocks %}\n  <li data-sector=\"{{ s.sector }}\">"),
        ("templates/index.html", "</ul>\n</body>", "</ul>\n<script>\ndocument.querySelectorAll('button[data-sector]').forEach((b) => b.addEventListener('click', () => document.querySelectorAll('li[data-sector]').forEach((li) => { li.hidden = li.dataset.sector !== b.dataset.sector; })));\n</script>\n</body>"),
    ],
    "T4": undo("parity-zola", "T4"),
    "T4b": undo("parity-zola", "T4b"),
    "T10": [],
    "T10-overedit": [("templates/stock.html", "<p>{{ page.extra.sector }}</p>", "<p>Sector: {{ page.extra.sector }}</p>")],
}
