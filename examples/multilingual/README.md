# A multilingual site

A small city guide in English, Spanish and German: 35 pages from five cities. It shows
what a site in several languages needs and what molde checks for it.

- **Locales** (`site.yaml`): English at the root, Spanish under `/es/`, German under
  `/de/`.
- **Localized paths and fields** (`routes.yaml`, `data.json`): `/cities/lisbon/`,
  `/es/ciudades/lisboa/`, `/de/staedte/lissabon/`. Valencia has no German name, so it
  gets no German page, and no German page links to one.
- **hreflang**: every page lists its other language versions, written by molde from
  the routes, and checked.
- **Translations** (`translations/`): every visible string comes from `t()`. A key
  missing in one language fails `translations.missing`.
- **Dates and numbers per locale**: `date_format` and `num_format`, from bundled CLDR
  data. Seville's population is `684,234` in English and `684.234` in Spanish and
  German; its founding date is `23 de noviembre de 1248` in Spanish.
- **Country pages** with `group_by: country` and `{country|slug}`. "Côte d'Ivoire"
  becomes `cote-d-ivoire`, and its name is translated in each language. The country
  slug comes from the data's English name in every language (`/de/laender/spain/`),
  because `group_by` can't group by a localized field yet.
- **Links between pages** are spelled out in `templates/_util.html.j2`, which must
  mirror the paths in `routes.yaml`; `links.internal` fails the check if they drift.
  Language versions of the current page come from `alternates`.
- **Pagination**: the city list, two per page, in each language.
- **Budgets**: every page must stay under 2 kB gzipped (the largest is about 0.8 kB).
- **JSON-LD**: every city page must carry a `City` object.

```sh
molde check --site examples/multilingual --out /tmp/guide-out
molde serve --out /tmp/guide-out
```

The test suite builds this site and compares every file with `tests/golden/multilingual/`.
