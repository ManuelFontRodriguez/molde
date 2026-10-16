# Cross-framework benchmark spec

> The commands and paths for re-running the measurements are in
> `harness/README.md`; this file defines what each port must reproduce and
> what is recorded per framework.

GOAL: reproduce the target site in each framework as CLOSE TO BYTE-IDENTICAL
as the framework allows, then measure everything. The target is `golden/` in this dir.

## Target site (8 output files)
- `/index.html` — stock list; head has charset, <title>, canonical, og:title/og:url/og:type
- `/stocks/{TICKER}/index.html` — one page per stock (4 stocks in data.json): name, ticker,
  sector, upcoming dividend (amount, currency, ex date) OR "No upcoming dividend declared.",
  an "Estimated — not yet declared" badge when upcoming.confirmed is false, back-link to /
- `/robots.txt` and `/style.css` — copied/emitted verbatim
- `/sitemap.xml` — all page URLs, base https://demo.example, pretty URLs (trailing /)
- Head tags per page: title pattern "{name} ({ticker})", canonical + og:url =
  https://demo.example/<pretty path>, og:type website
- Data source: `data.json` (4 stocks; note AT&T needs correct escaping: AT&amp;T).
  The site must be DATA-DRIVEN (pages generated from data.json, not hand-written).

## Recorded per framework (<framework>.metrics.json)
{
  "framework": "...", "version": "...",
  "install_seconds": <wall time to install toolchain+deps from an empty workspace, warm package cache, median of 3>,
  "deps_disk_mb": <du of node_modules or of the binary>,
  "dep_count": <packages in the lockfile; 1 for a single binary>,
  "files_authored": <files written by hand (templates/config/code; excludes data, lockfiles, package.json, shared style.css/robots.txt copies, BUILD.md)>,
  "loc_authored": <total lines in those files>,
  "build_ms_small": <median of N clean FULL builds, 4 stocks>,
  "build_ms_big": <median of N clean FULL builds, generated 1,900-stock dataset, same templates>,
  "fidelity_raw_diff": <TOTAL raw from compare.py on the small build>,
  "fidelity_normalized_diff": <TOTAL normalized from compare.py>,
  "irreducible_residue": "<what the framework injects/changes that could not be removed>",
  "features_manual": ["<things the target needed that the framework made the author build>"],
  "conformance_checks": "<does the framework itself verify links/titles/sitemap?>",
  "notes": "<friction, surprises>"
}
(`harness/run.py` fills the numeric fields; N is `BENCH_RUNS`, default 11. The qualitative
fields are written by hand. Raw samples summary per cell is in `harness/results.json`.)

## Rules
- Full build = clean production build (delete caches/output first: .next, _site, dist, public, etc.).
- Fidelity: `python3 compare.py <output dir> harness/golden`.
- Get as close to byte-identical as possible BEFORE measuring fidelity; if the framework
  injects scripts/comments/attributes that cannot be disabled, record it as irreducible_residue.
- The big build swaps in the generated dataset (1,900 stocks -> 1,901 pages). It is timed, never diffed.
- Install into the workspace where the ecosystem allows (npm local install). Zola and Hugo
  ship as Homebrew bottles here.
- Do not gold-plate. If exact parity is impossible after a reasonable effort,
  minimize the normalized diff and document why.
