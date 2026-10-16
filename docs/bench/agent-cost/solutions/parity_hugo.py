"""Reference solutions for the parity Hugo port (judge-side only; see molde.py).

Style: Hugo's own mechanisms (content adapters, layouts, .Paginate). Everything derives
from hugo.Data.stocks, so a changed data file changes the site.
T4/T4b: inverse of the plants in runner/setup.py (judge-side).
"""

import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from _idiomatic import undo  # noqa: E402

EXPECTED_FAIL = {"T10-overedit": "negative control: changes visible content when nothing needed changing"}

SLUG = '{{ replace (lower .Params.stock.sector) " " "-" }}'

SECTOR_ADAPTER = """{{/* One page per sector, from data/stocks.json. */}}
{{ $seen := slice }}
{{ range hugo.Data.stocks.stocks }}
  {{ if not (in $seen .sector) }}
    {{ $seen = $seen | append .sector }}
    {{ $slug := replace (lower .sector) " " "-" }}
    {{ $.AddPage (dict
      "path" $slug
      "url" (printf "/sectors/%s/" $slug)
      "kind" "page"
      "layout" "sector"
      "title" (printf "%s stocks" .sector)
      "params" (dict "sector" .sector "stocks" (where hugo.Data.stocks.stocks "sector" .sector))
    ) }}
  {{ end }}
{{ end }}
"""

SECTOR_LAYOUT = """<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8">
<link rel="stylesheet" href="/style.css">
<title>{{ .Title }}</title>
<link rel="canonical" href="{{ .Permalink }}">
<meta property="og:title" content="{{ .Title }}">
<meta property="og:url" content="{{ .Permalink }}">
<meta property="og:type" content="website">
</head>
<body>
<h1>{{ .Params.sector }}</h1>
<ul>
{{ range .Params.stocks }}
  <li><a href="/stocks/{{ .ticker }}/">{{ .name }}</a></li>
{{ end }}
</ul>
<p><a href="/">← all stocks</a></p>
</body>
</html>
"""

HOME_PAGINATED = """{{ $p := .Paginate (where site.RegularPages "Section" "stocks") 2 }}<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8">
<link rel="stylesheet" href="/style.css">
{{ $title := .Title }}{{ if gt $p.PageNumber 1 }}{{ $title = printf "%s (page %d)" .Title $p.PageNumber }}{{ end }}
<title>{{ $title }}</title>
<link rel="canonical" href="{{ $p.URL | absURL }}">
<meta property="og:title" content="{{ $title }}">
<meta property="og:url" content="{{ $p.URL | absURL }}">
<meta property="og:type" content="website">
</head>
<body>
<h1>Stocks (cycle {{ hugo.Data.stocks.cycle_id }})</h1>
<ul>
{{ range $p.Pages }}
  <li><a href="/stocks/{{ .Params.stock.ticker }}/">{{ .Params.stock.name }}</a> — {{ .Params.stock.sector }}</li>
{{ end }}
</ul>
{{ if $p.HasPrev }}<a href="{{ $p.Prev.URL }}">previous</a>{{ end }}
{{ if $p.HasNext }}<a href="{{ $p.Next.URL }}">next</a>{{ end }}
</body>
</html>
"""

SOLUTIONS = {
    "T1": [
        ("layouts/page.html", "<p>{{ .Params.stock.sector }}</p>",
         "<p>{{ .Params.stock.sector }}</p>\n<p>Dividend yield: {{ .Params.stock.dividend_yield }}</p>"),
        ("layouts/home.html", "— {{ .sector }}</li>", "— {{ .sector }} — yield {{ .dividend_yield }}</li>"),
    ],
    "T2": [
        ("content/sectors/_content.gotmpl", None, SECTOR_ADAPTER),
        ("layouts/sector.html", None, SECTOR_LAYOUT),
        ("layouts/page.html", "<p>{{ .Params.stock.sector }}</p>",
         '<p><a href="/sectors/' + SLUG + '/">{{ .Params.stock.sector }}</a></p>'),
        ("layouts/sitemap.xml", "{{- end }}\n</urlset>",
         "{{- end }}\n{{- $seen := slice }}\n{{- range hugo.Data.stocks.stocks }}\n{{- if not (in $seen .sector) }}{{ $seen = $seen | append .sector }}\n"
         '  <url><loc>{{ site.BaseURL }}sectors/{{ replace (lower .sector) " " "-" }}/</loc></url>\n{{- end }}\n{{- end }}\n</urlset>'),
    ],
    "T3": [
        ("content/stocks/_content.gotmpl", '(printf "/stocks/%s/" .ticker)', '(printf "/acciones/%s/" .ticker)'),
        ("layouts/home.html", 'href="/stocks/{{ .ticker }}/"', 'href="/acciones/{{ .ticker }}/"'),
        ("layouts/sitemap.xml", "{{ site.BaseURL }}stocks/{{ .ticker }}/", "{{ site.BaseURL }}acciones/{{ .ticker }}/"),
    ],
    "T5": [
        ("layouts/home.html", '<meta property="og:type" content="website">',
         '<meta property="og:type" content="website">\n<meta property="og:description" content="Upcoming dividends for every stock we track.">'),
        ("layouts/page.html", '<meta property="og:type" content="website">',
         '<meta property="og:type" content="website">\n<meta property="og:description" content="{{ .Params.stock.name }} ({{ .Params.stock.ticker }}): {{ .Params.stock.sector }}.">'),
    ],
    "T6": [
        # Hugo paginates pages, not raw data: give the adapter's pages the data order as weight.
        ("content/stocks/_content.gotmpl", "{{ range hugo.Data.stocks.stocks }}", "{{ range $i, $s := hugo.Data.stocks.stocks }}"),
        ("content/stocks/_content.gotmpl", '"kind" "page"', '"kind" "page"\n    "weight" (add $i 1)'),
        ("layouts/home.html", None, HOME_PAGINATED),
        ("layouts/sitemap.xml", "{{- end }}\n</urlset>",
         "{{- end }}\n{{- $pages := div (add (len hugo.Data.stocks.stocks) 1) 2 }}\n{{- if gt $pages 1 }}{{ range seq 2 $pages }}\n"
         "  <url><loc>{{ site.BaseURL }}page/{{ . }}/</loc></url>\n{{- end }}{{ end }}\n</urlset>"),
    ],
    "T11": [
        ("layouts/home.html", "{{ range hugo.Data.stocks.stocks }}\n  <li>",
         "{{ $rows := slice }}\n{{ range hugo.Data.stocks.stocks }}{{ $rows = $rows | append (merge . (dict \"sortkey\" (lower .name))) }}{{ end }}\n"
         "{{ range sort $rows \"sortkey\" }}\n  <li>"),
    ],
    "T12": [
        ("layouts/home.html", "<ul>\n{{ range hugo.Data.stocks.stocks }}\n  <li>",
         "{{ $sectors := slice }}\n{{ range hugo.Data.stocks.stocks }}{{ if not (in $sectors .sector) }}{{ $sectors = $sectors | append .sector }}{{ end }}{{ end }}\n"
         "<p>{{ range $sectors }}<button type=\"button\" data-sector=\"{{ . }}\">{{ . }}</button> {{ end }}</p>\n"
         "<ul>\n{{ range hugo.Data.stocks.stocks }}\n  <li data-sector=\"{{ .sector }}\">"),
        ("layouts/home.html", "</ul>\n</body>",
         "</ul>\n<script>\ndocument.querySelectorAll('button[data-sector]').forEach((b) => b.addEventListener('click', () => document.querySelectorAll('li[data-sector]').forEach((li) => { li.hidden = li.dataset.sector !== b.dataset.sector; })));\n</script>\n</body>"),
    ],
    "T4": undo("parity-hugo", "T4"),
    "T4b": undo("parity-hugo", "T4b"),
    "T10": [],
    "T10-overedit": [("layouts/page.html", "<p>{{ .Params.stock.sector }}</p>", "<p>Sector: {{ .Params.stock.sector }}</p>")],
}
