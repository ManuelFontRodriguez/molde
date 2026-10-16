# Site requirements

The site in this workspace is a small static website built from `data.json`. Whatever you change, the built site must meet every rule below. These rules are the definition of "done".

The site's public address is `https://demo.example`. Pages use "pretty" URLs: a page at `/stocks/KO/` is the file `stocks/KO/index.html` in the build output.

1. **The build succeeds.** The build command exits without an error.
2. **Every internal link works.** Every link or asset reference to this site (in `href`, `src` or CSS `url()`, written absolute or relative) points to a file that exists in the build output.
3. **The sitemap is exact.** `/sitemap.xml` lists the full URL of every page of the site once, and nothing else.
4. **Titles.** Every page has exactly one non-empty `<title>`, and no two pages share a title.
5. **Canonical and Open Graph.** Every page has one `<link rel="canonical">` and one `<meta property="og:url">`, both equal to the page's own full URL, and an `og:title` equal to its title.
6. **Escaping.** Text from the data appears exactly as written: for example "AT&T Inc" shows as "AT&T Inc" in the browser, never as "AT&amp;T Inc".
7. **Structured data.** If a page contains JSON-LD (`<script type="application/ld+json">`), it is valid JSON.
8. **Data-driven.** Pages come from `data.json`. If a stock is added to or changed in `data.json` and the site is rebuilt, the pages follow; nothing that comes from the data is hard-coded in templates. You may add to the data or move the file, but don't remove or change what is in it.
9. **Robots.** `/robots.txt` exists and names the sitemap.
10. **Content.** The pages keep showing what they show from the data: each stock's page its name, ticker, sector and either its upcoming dividend ("Next dividend: <amount> <currency> — ex <ex_date>") or "No upcoming dividend declared."; an "Estimated — not yet declared" badge (an element with class `badge`) exactly on the pages whose upcoming dividend is not confirmed; and the home page the data's `cycle_id` and a link to every stock's page.
