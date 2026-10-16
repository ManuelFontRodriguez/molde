# Design records

Each document here records why a bigger capability works the way it does: the
alternatives considered, the risks found in review and how each was answered. They were
written before or while the capability was built, so some passages describe the state
of molde at that time (code that has since been replaced, follow-up work that has since
shipped).

For how molde behaves today, the [reference](../reference.md) is the authority. Where a
design record and the reference disagree, the reference is right; please report it.

- [web-standards.md](web-standards.md): the `<head>` owner: canonical URLs, Open Graph,
  descriptions, indexability and the sitemap.
- [i18n.md](i18n.md): locales, localized paths, translation groups and hreflang.
- [cldr-formatting.md](cldr-formatting.md): dates and numbers for 529 locales from
  bundled Unicode CLDR data.
