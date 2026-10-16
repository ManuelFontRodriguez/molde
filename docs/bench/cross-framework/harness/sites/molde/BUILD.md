# molde port (the control)

- **Version**: molde 0.1.0-rc.1, built from this repo (`cargo build --release --locked`,
  ~32 s cold; run.py does this automatically if `target/release/molde` is
  missing). Single ~5 MB binary; all crates are compiled in — zero
  install-time dependencies, but not zero dependencies (see Cargo.lock).
- **Sources**: frozen snapshot of `examples/demo` at vendoring time, so future edits to the example can't silently change the
  benchmarked site. 4 authored files / 44 LOC: `routes.yaml` (8),
  `site.yaml` (1), `templates/index.html.j2` (15), `templates/stock.html.j2`
  (20). `data.json` is staged by run.py from `../../data.json`.
- **Build**: `molde build --site . --out out` — render + gzip
  (`Compression::best`) of every page + atomic publish. Conformance is the
  separate gated command: `molde check` (render + links/titles/sitemap/
  canonical-OG checks + publish-only-on-pass) measured 477 ms on the
  1,901-page dataset (the build plus five post-render passes over the written pages).
- **Timing note**: molde dedups byte-identical builds, so run.py wipes the
  whole `out/` dir before each timed run to force a genuine full build
  (render work happens before the dedup decision either way).
- **Head/sitemap/robots**: built-in (`std_head` slot + sitemap/robots
  emission with coverage checks) — golden's canonical/OG/sitemap/robots come
  from the framework, not hand-authored templates.
