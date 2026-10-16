1. Add a new field to the JSON schema (e.g., `price`, `location`, or `tags`) and render it on each item page and as a column/badge on the index page.
2. Add client-side search and filtering on the index page (text search box plus dropdown/checkbox filters by category or tag) that filters the item list without a page reload.
3. Add sorting controls to the index page (by name, date added, or price, ascending/descending), defaulting to newest first.
4. Generate SEO metadata per item page: unique `<title>`, meta description, Open Graph/Twitter card tags with the item's image, plus a `sitemap.xml` covering all pages.
5. Paginate the index page (e.g., 24 items per page with `/page/2/`-style URLs) and add per-category/tag listing pages generated from the same JSON.
