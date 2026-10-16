"""Reference solutions for the idiomatic Hugo port (judge-side only; see molde.py)."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from _idiomatic import source, undo  # noqa: E402

EXPECTED_FAIL = {"T10-overedit": "negative control: changes visible content when nothing needed changing"}

# T0: the frozen port's own files; the workspace's data.json is mounted as Hugo's data/stocks.json
# (Hugo has no other way to read a data file that sits in the project root) and style.css moves to static/.
T0_FILES = ["layouts/baseof.html", "layouts/home.html", "layouts/page.html", "layouts/robots.txt",
            "layouts/_partials/head.html", "content/stocks/_content.gotmpl", "content/stocks/_index.md"]
T0_MOUNTS = """
[[module.mounts]]
  source = 'data.json'
  target = 'data/stocks.json'
"""

SITEMAP = """{{- printf "<?xml version=\\"1.0\\" encoding=\\"utf-8\\" standalone=\\"yes\\"?>" | safeHTML }}
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  {{- range where site.Pages "Kind" "in" (slice "home" "page" "term") }}
    {{- if .IsHome }}
      {{- range .Paginator.Pagers }}
  <url><loc>{{ .URL | absURL }}</loc></url>
      {{- end }}
    {{- else }}
  <url><loc>{{ .Permalink }}</loc></url>
    {{- end }}
  {{- end }}
</urlset>
"""

# T6 head: the canonical/og:url/title of a list page come from its pager, not from the home page.
HEAD_T6 = """{{- $title := .Title }}{{ $url := .Permalink }}
{{- if .IsHome }}
  {{- with .Paginator }}
    {{- if gt .PageNumber 1 }}{{ $title = printf "%s (page %d)" $title .PageNumber }}{{ end }}
    {{- $url = .URL | absURL }}
  {{- end }}
{{- end -}}
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{{ $title }}</title>
<link rel="canonical" href="{{ $url }}">
<meta property="og:type" content="website">
<meta property="og:title" content="{{ $title }}">
<meta property="og:url" content="{{ $url }}">
<link rel="stylesheet" href="{{ "style.css" | relURL }}">
"""

SOLUTIONS = {
    "T0": [(r, None, source("hugo", r)) for r in T0_FILES]
          + [("hugo.toml", None, source("hugo", "hugo.toml") + T0_MOUNTS),
             ("static/style.css", None, "@MOVE style.css")],
    "T1": [
        ("content/stocks/_content.gotmpl", '      "sector" $stock.sector\n', '      "sector" $stock.sector\n      "dividend_yield" $stock.dividend_yield\n'),
        ("layouts/page.html", "  <p>Sector: {{ .Params.sector }}</p>\n", "  <p>Sector: {{ .Params.sector }}</p>\n  <p>Dividend yield: {{ .Params.dividend_yield }}</p>\n"),
        ("layouts/home.html", "— {{ .Params.sector }}</li>", "— {{ .Params.sector }} — {{ .Params.dividend_yield }}</li>"),
    ],
    # Sectors are a section of their own, generated from the data by a second content adapter.
    "T2": [
        ("content/sectors/_index.md", None, "---\ntitle: Sectors\nbuild:\n  render: never\n  list: always\n---\n"),
        ("content/sectors/_content.gotmpl", None, """{{- $sectors := slice }}
{{- range hugo.Data.stocks.stocks }}{{ $sectors = $sectors | append .sector }}{{ end }}
{{- range $sectors | uniq }}
  {{- $slug := replace (lower .) " " "-" }}
  {{- $.AddPage (dict
    "path" $slug
    "slug" $slug
    "title" (printf "%s stocks" .)
    "sitemap" (dict "priority" -1)
    "params" (dict "name" .)
  ) }}
{{- end }}
"""),
        ("layouts/sectors/page.html", None, """{{ define "main" }}
  <h1>{{ .Params.name }}</h1>
  <ul>
    {{ range where site.RegularPages "Params.sector" .Params.name }}
      <li><a href="{{ .RelPermalink }}">{{ .Params.name }}</a></li>
    {{ end }}
  </ul>
  <p><a href="{{ site.Home.RelPermalink }}">← All stocks</a></p>
{{ end }}
"""),
        ("layouts/page.html", "<p>Sector: {{ .Params.sector }}</p>",
         "<p>Sector: <a href=\"{{ relURL (printf \"sectors/%s/\" (replace (lower .Params.sector) \" \" \"-\")) }}\">{{ .Params.sector }}</a></p>"),
    ],
    "T3": [("hugo.toml", "enableRobotsTXT = true", "enableRobotsTXT = true\n\n[permalinks.page]\n  stocks = '/acciones/:slug/'")],
    # T3: Hugo's own way to move a section is a permalink pattern for its pages (above).
    "T4": undo("idiomatic-hugo", "T4", special={4: [("content/_index.md", None, '---\ntitle: "Demo — dividend stocks"\n---\n')]}),
    "T4b": undo("idiomatic-hugo", "T4b"),
    "T5": [
        ("content/stocks/_content.gotmpl", '    "title" (printf "%s (%s)" $stock.name $stock.ticker)\n',
         '    "title" (printf "%s (%s)" $stock.name $stock.ticker)\n    "description" (printf "%s (%s): %s." $stock.name $stock.ticker $stock.sector)\n'),
        ("hugo.toml", "enableRobotsTXT = true", "enableRobotsTXT = true\n\n[params]\n  description = 'Upcoming dividends for every stock we track.'"),
        ("layouts/_partials/head.html", '<meta property="og:url" content="{{ .Permalink }}">\n',
         '<meta property="og:url" content="{{ .Permalink }}">\n{{ with .Description | default site.Params.description }}<meta property="og:description" content="{{ . }}">{{ end }}\n'),
    ],
    "T6": [
        ("hugo.toml", "enableRobotsTXT = true", "enableRobotsTXT = true\n\n[pagination]\n  pagerSize = 2"),
        ("layouts/_partials/head.html", None, HEAD_T6),
        ("layouts/sitemap.xml", None, SITEMAP),
        ("layouts/home.html", """  {{ with site.GetPage "/stocks" }}
    <ul>
      {{ range .Pages }}""", """  {{ $pager := .Paginator }}
  {{ with site.GetPage "/stocks" }}
    <ul>
      {{ range $pager.Pages }}"""),
        ("layouts/home.html", "    </ul>\n  {{ end }}\n",
         """    </ul>
  {{ end }}
  <nav>
    {{ with $pager.Prev }}<a href="{{ .URL }}" rel="prev">← Previous</a>{{ end }}
    {{ with $pager.Next }}<a href="{{ .URL }}" rel="next">Next →</a>{{ end }}
  </nav>
"""),
    ],
    "T7": [
        ("layouts/page.html", "  <p><a href=\"{{ site.Home.RelPermalink }}\">",
         """  <script type="application/ld+json">{{ dict "@context" "https://schema.org" "@type" "Corporation" "name" .Params.name "tickerSymbol" .Params.ticker | jsonify | safeJS }}</script>
  <p><a href="{{ site.Home.RelPermalink }}">"""),
    ],
    "T8": [("static/style.css", "background: #fce8b2; }", "background: #fce8b2; color: red; font-weight: bold; }")],
    "T9": [
        ("layouts/home.html", "  {{ with site.GetPage", '  <input type="search" id="q" placeholder="Search by name or ticker" aria-label="Search stocks">\n  {{ with site.GetPage'),
        ("layouts/home.html", "<li><a", '<li data-search="{{ printf "%s %s" .Params.name .Params.ticker | lower }}"><a'),
        ("layouts/home.html", "{{ end }}\n{{ end }}\n", """{{ end }}
  <script>
    const items = document.querySelectorAll('li[data-search]');
    document.getElementById('q').addEventListener('input', (e) => {
      const q = e.target.value.trim().toLowerCase();
      items.forEach((li) => { li.hidden = !li.dataset.search.includes(q); });
    });
  </script>
{{ end }}
"""),
    ],
    "T11": [
        ("layouts/home.html", "range .Pages", "range sort .Pages \"Params.name\""),
    ],
    "T12": [
        ("layouts/home.html", "  {{ with site.GetPage", """  {{ with site.GetPage "/stocks" }}
    <p id="sectors">
      {{ range .Pages.GroupByParam "sector" }}<button type="button" data-sector="{{ .Key }}">{{ .Key }}</button> {{ end }}
    </p>
  {{ end }}
  {{ with site.GetPage"""),
        ("layouts/home.html", "<li><a", '<li data-sector="{{ .Params.sector }}"><a'),
        ("layouts/home.html", "{{ end }}\n{{ end }}\n", """{{ end }}
  <script>
    const items = document.querySelectorAll('li[data-sector]');
    document.querySelectorAll('#sectors button').forEach((b) => {
      b.addEventListener('click', () => {
        items.forEach((li) => { li.hidden = li.dataset.sector !== b.dataset.sector; });
      });
    });
  </script>
{{ end }}
"""),
    ],
    "T10": [],
    "T10-overedit": [("layouts/page.html", "<p>Sector: {{ .Params.sector }}</p>", "<p>Industry: {{ .Params.sector }}</p>")],
}
