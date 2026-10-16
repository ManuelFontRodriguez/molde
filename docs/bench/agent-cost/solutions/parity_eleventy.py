"""Reference solutions for the parity Eleventy port (judge-side only; see molde.py).

Style: Eleventy's pagination over data, global data files, the config's sitemap template.
T4/T4b: inverse of the plants in runner/setup.py (judge-side).
"""

import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from _idiomatic import undo  # noqa: E402

EXPECTED_FAIL = {"T10-overedit": "negative control: changes visible content when nothing needed changing"}

SECTORS_JS = """// Sectors, derived from the stock data (one entry per distinct sector, in data order).
const demo = require("./demo.json");

module.exports = function () {
  const bySector = new Map();
  for (const s of demo.stocks) {
    if (!bySector.has(s.sector)) bySector.set(s.sector, []);
    bySector.get(s.sector).push(s);
  }
  return [...bySector].map(([name, stocks]) => ({
    name,
    slug: name.toLowerCase().replace(/ /g, "-"),
    stocks,
  }));
};
"""

SECTORS_NJK = """---
pagination:
  data: sectors
  size: 1
  alias: sector
permalink: "/sectors/{{ sector.slug }}/"
---
<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8">
<link rel="stylesheet" href="/style.css">
<title>{{ sector.name }} stocks</title>
<link rel="canonical" href="https://demo.example/sectors/{{ sector.slug }}/">
<meta property="og:title" content="{{ sector.name }} stocks">
<meta property="og:url" content="https://demo.example/sectors/{{ sector.slug }}/">
<meta property="og:type" content="website">
</head>
<body>
<h1>{{ sector.name }}</h1>
<ul>
{% for s in sector.stocks %}
  <li><a href="/stocks/{{ s.ticker }}/">{{ s.name }}</a></li>
{% endfor %}
</ul>
<p><a href="/">← all stocks</a></p>
</body>
</html>
"""

INDEX_PAGINATED = """---
pagination:
  data: demo.stocks
  size: 2
permalink: "{% if pagination.pageNumber == 0 %}/{% else %}/page/{{ pagination.pageNumber + 1 }}/{% endif %}"
---
{% set pageTitle %}Demo — dividend stocks{% if pagination.pageNumber > 0 %} (page {{ pagination.pageNumber + 1 }}){% endif %}{% endset -%}
<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8">
<link rel="stylesheet" href="/style.css">
<title>{{ pageTitle }}</title>
<link rel="canonical" href="https://demo.example{{ page.url }}">
<meta property="og:title" content="{{ pageTitle }}">
<meta property="og:url" content="https://demo.example{{ page.url }}">
<meta property="og:type" content="website">
</head>
<body>
<h1>Stocks (cycle {{ demo.cycle_id }})</h1>
<ul>
{% for s in pagination.items %}
  <li><a href="/stocks/{{ s.ticker }}/">{{ s.name }}</a> — {{ s.sector }}</li>
{% endfor %}
</ul>
{% if pagination.href.previous %}<a href="{{ pagination.href.previous }}">previous</a>{% endif %}
{% if pagination.href.next %}<a href="{{ pagination.href.next }}">next</a>{% endif %}
</body>
</html>
"""

SOLUTIONS = {
    "T1": [
        ("src/stocks.njk", "<p>{{ s.sector }}</p>", "<p>{{ s.sector }}</p>\n<p>Dividend yield: {{ s.dividend_yield }}</p>"),
        ("src/index.njk", "— {{ s.sector }}</li>", "— {{ s.sector }} — yield {{ s.dividend_yield }}</li>"),
    ],
    "T2": [
        ("src/_data/sectors.js", None, SECTORS_JS),
        ("src/sectors.njk", None, SECTORS_NJK),
        ("src/stocks.njk", "<p>{{ s.sector }}</p>",
         '<p><a href="/sectors/{{ s.sector | lower | replace(" ", "-") }}/">{{ s.sector }}</a></p>'),
        ("eleventy.config.js", '"{% endfor %}</urlset>\\n"',
         '"{% endfor %}{% for sec in sectors %}  <url><loc>https://demo.example/sectors/{{ sec.slug }}/</loc></url>\\n{% endfor %}</urlset>\\n"'),
    ],
    "T3": [
        ("src/stocks.njk", 'permalink: "/stocks/{{ s.ticker }}/"', 'permalink: "/acciones/{{ s.ticker }}/"'),
        ("src/stocks.njk", 'rel="canonical" href="https://demo.example/stocks/{{ s.ticker }}/"',
         'rel="canonical" href="https://demo.example/acciones/{{ s.ticker }}/"'),
        ("src/stocks.njk", 'property="og:url" content="https://demo.example/stocks/{{ s.ticker }}/"',
         'property="og:url" content="https://demo.example/acciones/{{ s.ticker }}/"'),
        ("src/index.njk", 'href="/stocks/{{ s.ticker }}/"', 'href="/acciones/{{ s.ticker }}/"'),
        ("eleventy.config.js", "https://demo.example/stocks/{{ s.ticker }}/", "https://demo.example/acciones/{{ s.ticker }}/"),
    ],
    "T5": [
        ("src/index.njk", '<meta property="og:type" content="website">',
         '<meta property="og:type" content="website">\n<meta property="og:description" content="Upcoming dividends for every stock we track.">'),
        ("src/stocks.njk", '<meta property="og:type" content="website">',
         '<meta property="og:type" content="website">\n<meta property="og:description" content="{{ s.name }} ({{ s.ticker }}): {{ s.sector }}.">'),
    ],
    "T6": [
        ("src/index.njk", None, INDEX_PAGINATED),
        ("eleventy.config.js", '"{% endfor %}</urlset>\\n"',
         '"{% endfor %}{% for n in range(2, (((demo.stocks | length) + 1) // 2) + 1) %}  <url><loc>https://demo.example/page/{{ n }}/</loc></url>\\n{% endfor %}</urlset>\\n"'),
    ],
    "T11": [
        ("src/index.njk", "{% for s in demo.stocks %}", "{% for s in demo.stocks | sort(false, false, \"name\") %}"),
    ],
    "T12": [
        ("src/_data/sectorNames.js", None,
         "// Distinct sector names, in data order.\nconst demo = require(\"./demo.json\");\n\n"
         "module.exports = () => [...new Set(demo.stocks.map((s) => s.sector))];\n"),
        ("src/index.njk", "<ul>\n{% for s in demo.stocks %}\n  <li>",
         "<p>{% for sector in sectorNames %}<button type=\"button\" data-sector=\"{{ sector }}\">{{ sector }}</button> {% endfor %}</p>\n"
         "<ul>\n{% for s in demo.stocks %}\n  <li data-sector=\"{{ s.sector }}\">"),
        ("src/index.njk", "</ul>\n</body>", "</ul>\n<script>\ndocument.querySelectorAll('button[data-sector]').forEach((b) => b.addEventListener('click', () => document.querySelectorAll('li[data-sector]').forEach((li) => { li.hidden = li.dataset.sector !== b.dataset.sector; })));\n</script>\n</body>"),
    ],
    "T4": undo("parity-eleventy", "T4"),
    "T4b": undo("parity-eleventy", "T4b"),
    "T10": [],
    "T10-overedit": [("src/stocks.njk", "<p>{{ s.sector }}</p>", "<p>Sector: {{ s.sector }}</p>")],
}
