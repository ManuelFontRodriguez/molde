# Reproducible harness

Everything needed to re-run the cross-framework benchmark yourself:

```
brew install hugo zola                     # Zola and Hugo come from Homebrew
cargo build --release --locked             # in the repository root: the molde binary
BENCH_REINSTALL=1 python3 run.py           # all six frameworks
python3 run.py molde zola                  # or any subset
```

Environment variables: `BENCH_RUNS` (timed runs per cell, default 11), `BENCH_WORK`
(scratch workspace, default `/tmp/molde-bench-run`), `BENCH_REINSTALL=1` (time
`brew reinstall` as Zola's and Hugo's install cost), `MOLDE_BIN` (the molde binary,
default `target/release/molde`).

Per framework: authored sources are copied from `sites/<fw>/` into a scratch
workspace under `/tmp/molde-bench-run/` (local `npm ci` for the Node rows;
Zola and Hugo come from Homebrew), then the script runs medians of 11 clean full
builds on the small dataset, checks fidelity against `golden/`
with `../compare.py`, and repeats the timing with the generated 1,900-stock
dataset, plus `molde check` on both. Results print per row and land in
`results.json` (with min and max per cell) and in the `*.metrics.json` files.

## What's here

- `golden/` — the **frozen** target (8 files). It was minted from molde's
  `examples/demo` build when the harness was vendored (it is the target the
  original runs measured against), and it is deliberately never regenerated
  at run time: freezing it means future molde changes can't quietly move the
  goalposts — molde HEAD is measured against it like any other row, and can
  fail it.
- `data.json` — small dataset (4 stocks). `gen_data_big.py` — deterministic
  1,900-stock generator for the big runs (timing only, never diffed).
- `sites/<framework>/` — the authored sources for each port, each with a
  `BUILD.md` recording exact versions, commands, and the tricks that row
  needed. These are the files counted in report.md's "Authored" row.
- `run.py` — the orchestrator described above (`BENCH_RUNS` sets the runs per cell).

## Reading the numbers

Absolute milliseconds are machine-dependent (report.md's numbers: Apple M4 Pro).
What should reproduce anywhere are the *ratios* and the *facts*: fidelity totals,
what each framework makes you author by hand, and what it validates (see the
Conformance row). molde's timed `build` includes gzip of every page; its
conformance checks are a separate gated command, timed as `molde check`.

Platform note: `run.py` takes Zola and Hugo from your `PATH` (Homebrew on macOS; on
Linux, install them your usual way). The Node rows are pinned via each site's
`package.json` and lockfile.
