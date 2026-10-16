# Cross-framework benchmark: the same site, six ways (measured 2026-10-15)

Each framework reproduced the same golden output (8 files, frozen in `harness/golden/` from an
earlier version of molde's example site:
data-driven index, 4 stock pages, sitemap, robots.txt, stylesheet — full canonical/OG head) as close to
byte-identical as each framework allows, then measured everything against a shared spec
(`spec.md`, `compare.py`). molde ran through the identical harness as the control. The
golden output came from molde's own build, so molde's fidelity of 0 is by construction: the
fidelity row shows how close each other framework gets to it. The workload is a data-driven
site built from a JSON dataset, not a Markdown blog or an image-heavy site.
Raw per-framework data: `*.metrics.json` and `harness/results.json`.
Machine: Apple M4 Pro, 24 GB, macOS 27; warm npm cache; medians of 11 runs per cell.

## Results

| | **molde 0.1.0-rc.1** | Zola 0.23.6 | Hugo 0.168 | Eleventy 3.1.6 | Astro 7.3.8 | Next.js 16.4.0 |
|---|---|---|---|---|---|---|
| Install | 0 s prebuilt, ~32 s from source (5 MB binary)¹ | 1.7s (41 MB binary) | 1.4s (86 MB binary) | 0.5s (22 MB, 130 pkgs) | 1.1s (167 MB, 286 pkgs) | 2.0s (337 MB, 54 pkgs) |
| Authored | **4 files / 44 LOC** | 7 / 89 | 7 / 74 | 3 / 69 | 4 / 94 | 7 / 156 |
| Build, 5 pages | **6.8 ms** | 39 ms (17 + 25 gen)⁶ | 50 ms | 288 ms | 559 ms | 1,361 ms |
| Build, 1,901 pages | **170 ms²** | 744 ms (419 + 391 gen)⁶ | 302 ms | 781 ms | 965 ms | 2,181 ms |
| Fidelity (diff lines) | 0 (control) | **0** | **0** | **0** | **0** | 75 (irreducible) |
| Output residue | none | 404.html⁷ | none³ | none | none⁴ | 260 B/page + 14 dead JS chunks (~430 KB) + forced 404s |
| Sitemap | built-in (+coverage check) | manual override (built-in format wrong, but overridable) | manual (built-in format wrong) | manual | manual⁵ | manual pre-build script |
| Head (title/canonical/OG) | built-in (`std_head` + checks) | manual | manual | manual | manual | manual (`next/head` strings) |
| Built-in conformance checks | **links, titles, sitemap, canonical/OG, JSON-LD, budgets; publish gated** | links in Markdown only⁸ | none built in | none built in | none built in | none built in |
| Precompressed `.gz` | built-in (in the timed build) | no | no | no | no | no |

¹ the release archive is a single prebuilt binary; building it from source with a cold
  `cargo build --release --locked` takes about 32 s.
² includes gzip (maximum compression) of every page, a feature no other row has; pages are
  rendered and compressed on every core. The conformance checks are a separate gated
  command: `molde check` (render + every check + gated publish) on the same 1,901 pages
  takes **477 ms**, and 9.7 ms on the 5-page site.
³ after `disableHugoGeneratorInject` + disabling RSS/taxonomies/section kinds.
⁴ after `compressHTML: false`; loop whitespace needed `set:html` string-building.
⁵ `@astrojs/sitemap` rejected: emits a different format (xhtml ns, own ordering).
⁶ Zola has no data→pages mechanism (Hugo's content adapters have no equivalent): every build
  must run `gen_content.py`, emitting one `.md` per stock. Split = zola build + generation;
  splits are independent medians and need not sum to the total.
⁷ 404.html always emitted; no config switch. Nothing injected into authored pages.
⁸ `zola check` validates internal/external links — but only in markdown content. It cannot see
  template-emitted hrefs (every link on this site), runs as a separate command, and gates nothing.

## Findings

1. **Text-templating engines reach byte-parity almost free; component renderers pay.**
   Hugo, Zola and Eleventy (untrimmed template semantics ≈ minijinja) match on nearly the
   first build. Astro matches only by escaping its own component model (`set:html`
   string-building for loop whitespace). Next.js cannot match at all: HTML is a
   hydration substrate there, not an output format: 260 bytes/page of residue and 14
   JS chunks shipped for a site with no client code. Next.js is built for applications,
   and this static, data-only site is not its use case.

2. **None of the others has built-in, template-aware checks that gate the publish.** Each
   of them builds a broken href, a duplicate title or a sitemap that disagrees with the
   page set without complaint. Link checkers such as lychee or htmltest can be added in
   CI, and `zola check` covers links in Markdown content; none of these knows which
   route or template made a page, and none stops a publish by itself. Every
   feature the target needed beyond rendering (sitemap, canonical/OG) was manual work,
   and where a built-in existed (Hugo/Astro sitemaps) its format wasn't controllable
   enough to use. Conformance shipped with the capability remains molde's
   differentiator: `molde check` renders, checks and publishes 1,901 pages in under half a
   second (477 ms).

3. **molde is the fastest of the six on this workload, at both sizes, while also
   compressing every page.** 6.8 ms for 5
   pages and 170 ms for 1,901, against Hugo's 50 and 302 ms — and molde's time includes
   maximum-level gzip of every page. Its output is deterministic (same site, same bytes),
   which is what makes the fidelity control and `molde diff` possible.

4. **Dependency surface spans two orders of magnitude** for the same 8 output files: 5 MB
   (molde) → 337 MB (Next). For a fleet of properties, that surface is patch/audit/rot
   area.

5. **Fixed overhead decides the iteration loop.** Per-page cost converges at a fraction of
   a millisecond everywhere; the floor does not: molde ~7 ms, Zola ~40 ms, Hugo ~50 ms,
   the Node frameworks 290–1,360 ms. For an agent whose loop is build → check per edit,
   the floor is the iteration latency.

## Reproduce it

```sh
brew install hugo zola
cargo build --release --locked
cd docs/bench/cross-framework/harness && BENCH_REINSTALL=1 python3 run.py
```

`run.py` re-runs the whole experiment on your machine: frozen golden target, all six
authored site sources (each with a `BUILD.md`), toolchains installed into a scratch
workspace, medians of 11 clean full builds per cell, the fidelity gate, the 1,901-page
timing and `molde check`. See `harness/README.md`.

## Method notes / limits

- One port per framework; a framework expert might shave LOC or milliseconds.
- molde is the fidelity control: the golden output was minted from molde's own build, so
  its 0 is by construction. The other rows show how close each framework gets to it.
- molde's timed `build` includes maximum-level gzip of every page; no other row compresses.
  Its checks are measured separately (`molde check`, footnote 2).
- `install_seconds` used warm caches (npm cache, Homebrew bottles already downloaded);
  cold-network installs would grow every row, the Node rows most.
- The machine was not idle (load average about 3.5 at the start); absolute milliseconds
  move with that, the ratios much less. Min and max per cell are in `results.json`.
- Small site = 4 stocks; big = generated 1,900 stocks (deterministic generator:
  `harness/gen_data_big.py`).
- Fidelity compared with `compare.py` (raw + cosmetic-normalized diff line counts).
- npm/npx process startup is inside all Node build timings (consistent across rows);
  Hugo's cache dir and Astro's Vite cache persist across a row's timed runs, so their
  medians reflect warm-cache full builds.
- Not measured: memory, incremental or watch-mode builds, dev servers, cold caches,
  Linux or CI runners, Markdown- or image-heavy sites, page weight in the browser.

## Zola notes

Zola is the other Rust SSG; its row deserves its own notes:

1. **Byte-parity on the first successful build** — raw diff 0 on every file. Tera's
   untrimmed whitespace semantics are identical to minijinja's, a strong confirmation of
   finding 1.
2. **The data→pages gap is structural, not cosmetic.** Zola has no way to build pages from
   a data file, so the honest pipeline needs a generator script emitting one `.md` per stock
   on every build: 391 of the 744 ms big build, and 1 of the 7 authored files. On Zola's
   binary time alone (419 ms), molde is ~2.5× faster while also gzipping every page. With
   the generator included, the gap is ~4.4×, but a better generator could narrow that.
3. **Zola is the only incumbent with any checker — and it still can't see this site.**
   `zola check` validates links in markdown content, but every link here is emitted by a
   template, where it checks nothing; it runs as a separate command and gates no publish.
4. **Smallest incumbent footprint**: a 41 MB single binary, the only row near molde's
   weight class (5 MB). Case-preserving URLs needed explicit `path` front matter (the
   default slugify lowercases `/stocks/KO/`); 404.html is always emitted with no off switch.

Fastest of the six on this workload: **molde at both sizes**, 6.8 ms and 170 ms, against
Hugo's 50 and 302 ms and Zola's 419 ms (binary alone), on identical output. Ports by people
who know each framework better are welcome; each port's `BUILD.md` says how it was made.
