# Zola port

- **Version**: Zola 0.23.6, installed with `brew install zola` (single binary, 41 MB).
- **Build**: `python3 gen_content.py && zola build` (run.py passes `--root`).
  The generator step is mandatory: Zola has no data→pages mechanism (no
  equivalent of Hugo's content adapters), so every build first emits one
  `.md` per stock from `data.json`. On the 1,900-stock dataset that step is
  ~450 of ~850 ms.
- **Tricks needed for byte-parity** (raw diff 0, first successful build —
  Tera's untrimmed whitespace matches minijinja exactly):
  - explicit `path = "stocks/<TICKER>"` frontmatter per page; default
    slugification would lowercase `/stocks/KO/` → `/stocks/ko/`
  - `templates/sitemap.xml` override (built-in emits its own layout, but
    unlike Hugo/Astro the override is possible) and `templates/robots.txt`.
    Note: parity depends on Zola's alphabetical `entries` order coinciding
    with the dataset's order — true for both bench datasets (KO<PEP<SAN<T,
    S0001–S1900), not for arbitrarily ordered data
  - `content/stocks/_index.md` with `render = false` so `/stocks/` emits no
    page of its own
  - head (title pattern, canonical, og:*) hand-authored in templates
- **Residue**: `404.html` is always emitted; no config switch to disable.
- **Checks**: `zola check` exists but validates links in *markdown content*
  only — every link on this site is template-emitted, so it validates
  nothing here, and it gates no publish.
