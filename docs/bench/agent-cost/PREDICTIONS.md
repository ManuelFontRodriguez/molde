# Predictions and analysis plan

**Status: FROZEN on 16 Oct 2026** (made locally; SHA-256 in LOG.md). Frozen together with DESIGN.md, the tasks, the oracle, the classifier and the seed before the pilot, and published before the campaign (DESIGN.md §11); the pilot ran before any public freeze. The SHA-256 of this file is recorded in LOG.md at that moment. Nothing in this file changes after the freeze, apart from one editorial pass for publication that removed private context and replaced molde commit identifiers with descriptions and the pinned source-tree hashes below; no hypothesis, statistic or threshold changed. Any later change is a new, dated entry in LOG.md, and the original stays as written.

The pilot's runs are excluded from every analysis below.

## Definitions

**Attempt.** One run of one arm on one task, prompt and repetition, whose outcome is `pass`, `fail` or `censored`. Runs classified `infra_fail` are re-run and excluded (DESIGN.md §7).

**New tokens.** `input_tokens + cache_creation_input_tokens + output_tokens` from the run's final `result` event.

**False-done.** The agent ended normally and said it was done (`claimed_done`, defined structurally in `runner/metrics.py`), but the judge failed the site. Split into:

- **signal ignored:** the agent's own checker reported a failure during the run;
- **no signal:** no failing report was ever shown.

**Primary molde arm.** `molde-check`. `molde-mcp` is secondary and never pooled with it, and the better of the two is never chosen after the fact.

**Core tasks.** T1–T6, including T4b (seven tasks).

## Hypotheses

**H1 (cost).** On the core tasks, `molde-check` uses fewer new tokens per attempt than each incumbent arm.

- **Statistic:** for each task, the ratio *median(new tokens, molde-check) / median(new tokens, other arm)*, over all attempts. Pooled over the core tasks as the geometric mean of the per-task ratios.
- **Interval:** 95% bootstrap interval, resampling attempts within each (arm, task) cell, 10,000 resamples, seed `1016`.
- **Comparisons:** `molde-check` against each of `hugo`, `astro`, `nextjs`, `hugo+checker`, `astro+checker`, `nextjs+checker` (six), with Holm correction.
- **H1 holds against an arm** when the corrected upper bound of the pooled ratio is below 1.0.
- **The strong form ("at most half")** holds when it is below 0.5.
- **The thesis claim (b) in DESIGN.md §1** is tested like for like (review 2 B4): `molde+checker` against `hugo+checker`, `astro+checker` and `nextjs+checker`, the same checker for all four. It needs the pooled ratio's corrected upper bound below 1.0 against all three. `molde-check` (molde as shipped, with its own, different checker) against the `+checker` arms is reported too, labelled as a comparison of two checkers as well as two generators.

**H2 (safety).** On T3, T4 and T4b pooled, `molde-check`'s false-done rate is lower than the pooled rate of the three arms without a checker.

- **Test:** Fisher's exact test, one-sided, α = 0.05. Exact Clopper-Pearson intervals reported for every arm.
- **"No difference"** (p ≥ 0.05) falsifies H2. That outcome is reported plainly, not explained away.

**H2b (molde's blind spots), pre-registered against molde.** On T4b, `molde-check` is expected to have **more** false-dones of the kind "checker passed (blind)" than on T4. The reason: `molde check` passes the T4b site, verified on molde as of 15 Oct and again on the pinned molde (verdict pass on the planted workspace: a second single-quoted canonical, a contradicting single-quoted `og:title`, a badge on the wrong pages, a stale heading, a wrong amount). Reported whatever the result.

**H3 (controls).**

- **T8 (CSS only):** the `molde-check` / incumbent median ratio of new tokens lies within [0.7, 1.4] for every incumbent arm, as a 95% interval. A ratio outside it means the harness or the docs cost moves costs on tasks where the generator shouldn't matter, and is reported as a threat to H1.
- **T9 (client-side search):** `molde-check` does not beat `nextjs` (ratio ≥ 1.0).
- **T10 (nothing to do):** reported for over-editing rate and verification cost; no direction predicted.

**H4 (a complete report helps).**

- `molde-check` uses fewer new tokens per attempt than `molde-build` (pooled core ratio, upper bound < 1.0).
- Each `+checker` arm uses fewer than its framework without a checker.

If H4 holds but H1 doesn't hold against the `+checker` arms, the published conclusion is "a complete report helps every framework", not the thesis.

## Expected failures, declared now

- **None for molde at the pinned version.** T2 was declared an expected failure on 15 Oct (molde as of 15 Oct, before the slug filter: `group_by` couldn't slug a value in the path). molde then added `{field|slug}` and the `slug` filter (16 Oct); the reference solution now passes, mutation test included (verified 16 Oct on the pinned molde). The benchmark pins molde 0.1.0-rc.1, the public repository's first release; source tree hashes (git, survive any commit): `src` cbce30eb3127d577fed9f8cf1075963128a7e6ac, `Cargo.toml` a70ce314535d0d2583fe37e776c1ec084184f3e8, `Cargo.lock` dc34f5a27a5bdd134097469ecad75fec62be1d76, `build.rs` 1e388cda3c88dd8033a281572e14b70e525bbea5.
- T2 keeps its own row in the report, and every pooled statistic is still also reported without T2 (review 2 I11), because the capability is new in the version under test.

## Secondary results (reported, not tested)

- success rate per arm and task;
- tool calls by phase;
- turns;
- `duration_api_ms`;
- the per-defect detection table for T4 and T4b;
- costs with and without tokens spent reading docs;
- T0 (build from empty) per arm, with the golden diff beside the pass rate;
- P2 against P1 for the four main arms on T1–T6;
- the `claude-sonnet-5-5` robustness row on T3 and T4.

## What would make us distrust our own result

- The oracle and the second judge (lychee) disagree on more than 2% of runs.
- T8 falls outside its equivalence band (H3).
- Any run shows forbidden tool use or the token in a transcript. Those runs are excluded and listed.
- `infra_fail` exceeds 10% of runs in any one arm.
