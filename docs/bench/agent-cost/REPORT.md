# Agent cost: round 1 report

**Status: DRAFT (19 Oct 2026).** An independent audit has recomputed every number from the raw run data, and its corrections are applied. Still to come before it's final: a human review. Every number here comes from `analyze.py` or `analysis/secondary.py`, run on the published run folders.

## The answer, first

**Pre-registered, molde does not make agents cheaper.**
- **H1 fails against every arm.** On the core tasks, an agent using molde (`molde-check`) spent **1.08–1.62×** the new tokens of the same agent using Hugo, Astro or Next.js, with or without a checker.
- **The thesis test fails.** Like for like, with molde using the same standalone checker the other tools get, molde came out at **1.24–1.48×**.
- **H2, safety, is falsified as registered, but uninformative.** No agent claimed done on a broken site on the safety tasks (under P1), in any arm, so the tasks couldn't separate the arms.
- **H4 holds only for Next.js.** A complete report makes nextjs+checker cheaper than nextjs. It doesn't make molde-check cheaper than molde-build, once H4 is computed over the core tasks as PREDICTIONS.md defines them (T2 included).

**Why.** Most of the extra cost is the agent learning molde from its reference:

- **Without the docs tokens**, molde-check costs 0.47–0.71× the other tools (registered as "reported", not tested).
- **The model already knows** Hugo, Astro and Next.js. It has never seen molde.
- **Docs come before the check.** On the open-ended tasks (T0, T4, T4b, T10), in all 120 runs of the three command-line molde arms, the agent read the reference before running molde or in the same command:
  - 95 runs: in an earlier call;
  - 25 runs: in the same command as its first `molde check`.

  None ran the check first and then decided whether it needed the docs.
- **A smaller model makes it worse.** With `claude-sonnet-5-5` (robustness row, T3 and T4), molde-check costs 4.1–5.3× the others, and 0.68–0.87× without the docs.

**What's worth knowing besides the headline** (secondary, computed after seeing the data):

- **molde's changes are about half the size.** An agent's change to a molde site is 0.45–0.63× the lines of the same change to the Hugo, Astro or Next.js site (0.20–0.68× against the hand-written parity ports), in about half as many files. That means less for a person to review.
- **Priced, the gap is smaller but still there.** At each run's own API cost, molde-check is 0.95–1.46× the others. That's about level with Next.js (0.95, interval [0.86, 1.05]) and dearer than Hugo (1.17) and Astro (1.46). molde's agents write 13–46% fewer output tokens and finish in 0.55–0.92× the time.
- **Reading once per session would change the picture.** If the docs are read once and the cost is spread over the tasks of a session, molde-check falls below Hugo and Next.js from the second task and reaches Astro's level by the third. That's a projection; round 2 measures it.

## What was run

| | |
|---|---|
| Model | `claude-opus-5-5`, effort `high`, Claude Code 2.1.300, headless |
| Site | a 5-page site built from `data.json` (DESIGN §3), with ten written requirements |
| Arms | molde-check, molde+checker, molde-build, molde-mcp; hugo, astro, nextjs, each with and without the same standalone checker; a parity panel (hugo, zola, eleventy, astro, nextjs) |
| Tasks | T0–T12 plus T4b (DESIGN §5); T11 and T12 chosen by the task-selection rule |
| Runs | 1,100 in the registered campaign, plus 100 for molde+checker (amendment 4, LOG.md 19 Oct) |
| Outcomes | 1,170 pass, 30 fail, 0 infrastructure failures |

**Integrity:**
- No forbidden tool use reached the real molde: 18 attempts in 17 molde-build runs, all refused by the shim.
- No secret in any transcript.
- **The second link judge** (lychee) agreed with the oracle's link verdict on all 1,200 runs. That is agreement on broken links only, and both found none in any run. It is not a second pass/fail judgment.

## Registered results

The analysis uses P1 attempts for H1–H4 (P2 is reported separately), where PREDICTIONS.md says "over all attempts". Pooling P2 into H2 changes nothing (0 of 55 against 1 of 165, p = 0.75).

### H1: cost (new tokens, core tasks T1–T6 with T4b, P1)

Pooled ratio *molde arm ÷ other arm*. The brackets give the Holm-corrected bootstrap interval. With six comparisons, its two-sided level runs from 90% to 98.3%, which is the one-sided α of 0.05 that Holm's step-down uses. H1 holds when the upper bound is below 1.0.

| molde-check vs | ratio | interval | holds |
|---|---|---|---|
| hugo | 1.34 | [1.22, 1.59] | no |
| astro | 1.62 | [1.47, 1.84] | no |
| nextjs | 1.08 | [0.96, 1.25] | no |
| hugo+checker ¹ | 1.41 | [1.26, 1.60] | no |
| astro+checker ¹ | 1.43 | [1.34, 1.61] | no |
| nextjs+checker ¹ | 1.20 | [1.07, 1.42] | no |

¹ As PREDICTIONS.md requires: these rows compare two checkers as well as two generators (`molde check` against `site-check`).

**The thesis claim (b), like for like** (`molde+checker` against each framework `+checker` arm, the same checker on both sides): 1.45 [1.30, 1.72], 1.48 [1.38, 1.75], 1.24 [1.17, 1.45]. It does not hold.

**Without T2**, as registered: 1.08–1.60 for molde-check and 1.34–1.45 like for like. No different.

**Without the docs tokens** (registered as reported):

| | hugo | astro | nextjs | hugo+checker | astro+checker | nextjs+checker |
|---|---|---|---|---|---|---|
| molde-check | 0.59 | 0.71 | 0.47 | 0.62 | 0.63 | 0.53 |
| molde+checker | | | | 0.72 | 0.73 | 0.61 |

### H2: safety

False-done, meaning the agent said it was done and the site failed, on T3, T4 and T4b, under P1:

- molde-check: 0 of 45 (exact interval [0, 0.079]).
- The three arms without a checker: 0 of 135 ([0, 0.027]).
- One-sided Fisher p = 1.0.

As registered, "no difference" falsifies H2, and it is reported that way. The honest reading is a **ceiling**: no arm produced a false-done on these tasks under P1, so they could not separate the arms on safety.

**Under P2** (the four arms with the "make sure" prompt): molde-check 0 of 10, against 1 of 30 for the arms without a checker (p = 0.75). The one is a Next.js T3 run that added `404.html` to the sitemap.

**Across all tasks, 30 runs claimed done on a site the judge failed:**
- 29 come from the benchmark's own defects: 23 are the T6 conflict, 4 the T8 reading, 2 the Next.js `404.html`.
- One is a genuine agent error: a T12 filter that reloads the page, on nextjs+checker, after its checker had passed.

**H2b** (molde's blind spots): 0 "checker passed (blind)" false-dones on T4b and 0 on T4. Not as predicted. `molde check` passes the T4b site, and every molde-check T4b run still passed the judge (15 of 15). The agents must have found the planted defects without the check's help, by reading the files or the output.

### H3: controls

- **T8 (CSS only):** median ratios are 0.82–1.00, all inside [0.7, 1.4]. Two of the six intervals cross the band's lower edge: nextjs [0.41, 1.21] and astro+checker [0.68, 1.07]. As registered, that is listed as a threat to H1 (see "What could make this wrong").
- **T9 (client-side search):** molde-check / nextjs = 1.84. molde does not beat Next.js, as predicted.
- **T10 (nothing to do):**
  - No molde, Hugo or Astro arm edited anything.
  - The Next.js arms are flagged 5 of 5 and 4 of 5, but part of that is a measurement defect (`next-env.d.ts`, below). Without that file, nextjs agents edited in 5 of 5 runs and nextjs+checker in 3 of 5.
  - **The nextjs edits:** 4 of the 5 add a post-build script that deletes the extra copies of the not-found page the export writes (`/404/`, `/_not-found/`), which agents read as extra pages without a canonical tag; one of those also deletes `404.html`. The fifth changes a page.
  - **The nextjs+checker edits** change page code: links and URL decoding.

### H4: a complete report helps

PREDICTIONS.md defines H4 over the pooled core ratio, which includes T2. The frozen `analyze.py` computes it without T2, and with a 90% interval (a Holm step with one comparison), though PREDICTIONS.md names no level. The two frozen documents disagree. The table gives both, and the registered text decides.

| | with T2 (as PREDICTIONS.md) | 95% interval | holds | without T2 (as analyze.py) | 90% interval |
|---|---|---|---|---|---|
| molde-check vs molde-build | 0.91 | [0.84, 1.05] | **no** | 0.87 | [0.80, 0.95] |
| hugo+checker vs hugo | 0.95 | [0.90, 1.08] | no | 0.92 | [0.88, 1.04] |
| astro+checker vs astro | 1.13 | [1.06, 1.17] | no (costs more) | 1.14 | [1.07, 1.18] |
| nextjs+checker vs nextjs | 0.90 | [0.82, 0.96] | **yes** | 0.84 | [0.78, 0.88] |

**Reading:**
- As registered, a complete report makes the Next.js agent cheaper and no other.
- For molde, the benefit shows only when T2 is left out, so it is not established.
- H1 doesn't hold against the `+checker` arms either, so the thesis is not supported.

### The Sonnet robustness row

PREDICTIONS.md lists it among the secondary results. It wasn't in the campaign's run plan, so it was planned and run on 19 Oct, after the campaign (LOG.md). The row covers `claude-sonnet-5-5` on T3 and T4, the main arms with `molde+checker`, 5 runs per cell: 90 runs, all passing.

| molde-check vs | hugo | astro | nextjs | hugo+checker | astro+checker | nextjs+checker |
|---|---|---|---|---|---|---|
| new tokens | 4.17 | 4.60 | 4.13 | 4.73 | 5.33 | 4.78 |
| without docs | 0.68 | 0.75 | 0.68 | 0.77 | 0.87 | 0.78 |
| price-weighted | 3.23 | 3.74 | 3.47 | 3.75 | 4.24 | 3.96 |

Like for like (molde+checker), in new tokens: 1.97, 2.22, 1.99. On the same two tasks with Opus, molde-check came out at 1.33–1.86 against hugo, astro and nextjs, and molde+checker at 1.75–1.81 like for like (T3 and T4 only).

**The direction is the same as with Opus, and the gap is wider.** Most of the widening is on the incumbents' side: Sonnet is cheaper than Opus on the frameworks it knows (Hugo T3: a median of 4.3k tokens against 7.4k). Whole-reference reads don't show a consistent difference between the models:

| | Sonnet | Opus |
|---|---|---|
| molde-check T3 | 3 of 5 | 1 of 15 |
| molde-build T3 | 5 of 5 | 8 of 15 |
| molde+checker T3 | 0 of 5 | 3 of 15 |
| molde-check T4 | 4 of 5 | 15 of 15 |

Without the docs, molde is again cheaper (0.68–0.87). With 5 runs per cell, these are rough figures.

### Validating "claimed done" (DESIGN §7, changed)

DESIGN §7 registered two people labelling a blind sample of final messages. At the owner's request, two isolated model labellers did it instead (one Opus, one Sonnet, each seeing only the 40 masked messages).

- **Agreement:** both labelled all 40 "finished", agreeing with the structural rule on 40 of 40.
- **The lexical variant** marked 3 messages as not finished, and both labellers disagreed with it there. All three report the change as made: a rename (with a note that it added no redirects), a column added, and a filter built that the agent says it couldn't test in a real browser.
- **What it can't show:** every run in this round, 1,290 of 1,290, ended with the agent saying it was done. The check can't test the rule on a "not finished" message, because none exists, and kappa is undefined.
- **Caveats:** the judges are the same model family as the agents they judge, and file names in the messages can hint at which tool wrote them.

## Defects found in the benchmark

They are reported here rather than worked around. Neither the tasks nor the oracle were changed during the campaign.

1. **T6 contradicts requirement 10.** T6 asks for 2 stocks per page. Requirement 10, added after review 2, says the home page links to every stock.
   - Agents that keep requirement 10 add an "all stocks" line and fail. Agents that drop it pass.
   - All 23 T6 failures (22 in the campaign, 1 in amendment 4) fail on that one check alone, and every T6 pass dropped requirement 10.
   - T6's pass rate therefore measures how an agent settles a contradiction, not whether it can paginate. Its cost is unaffected: the work is the same either way.
   - Regraded without that check (post hoc, labelled), every T6 run passes.
2. **T8's wording admits "a red background".** All 4 T8 failures are white text on a red background; the judge asks for red text.
3. **`next-env.d.ts`.** `next build` generates it, and the diff didn't exclude it, so T10 over-editing is overstated for the Next.js arms. The rates without it are given under H3.
4. **Next.js's not-found pages.** The static export writes the not-found page as `404.html` and again at `/404/` and `/_not-found/`. The judge counts none of them as pages; agents disagree.
   - Two Next.js failures (T3 and T5, under P2) added `404.html` to the sitemap.
   - Four of the five nextjs T10 runs wrote scripts to delete the extra copies.
5. **The size table left out the thesis arm.** DESIGN §9 lists eight main arms without `molde+checker`, so the registered plan had no runs for the thesis test.
   - It was found after the campaign, while reading a preview of the other arms' cost ratios.
   - The arm was added as amendment 4: 100 runs on the same grid, run on 19 Oct, three days after most of the arms it is compared with.
6. **The docs counter missed one pattern.** When Claude Code saves a large `cat` to a file and the agent then reads sections of that file with `sed`, those reads weren't counted.
   - Fixed and recomputed from the transcripts: 23 runs changed, only in their docs fields.
   - The error ran against molde.
7. **The two frozen documents disagree on H4,** on whether T2 is in and on the interval level (see H4). Found by the audit.

## Secondary results (not registered as tests; computed after seeing the data)

**Price-weighted cost.** Each run's own `total_cost_usd`, as Claude Code reports it at list price. Its token weights are input 1, cache write 2 (Claude Code writes the cache with a one-hour lifetime), cache read 0.05 and output 5. Pooled over the core tasks, with a bootstrap 95% interval (seed 1016, 2,000 resamples, not Holm-corrected):

| molde-check vs | hugo | astro | nextjs | hugo+checker | astro+checker | nextjs+checker |
|---|---|---|---|---|---|---|
| price-weighted | 1.17 [1.08, 1.36] | 1.46 [1.35, 1.63] | 0.95 [0.86, 1.05] | 1.23 [1.10, 1.37] | 1.28 [1.19, 1.46] | 1.07 [0.98, 1.23] |

molde+checker like for like: 1.30, 1.35, 1.13.

The registered metric counts a token written to the cache and a token of output the same, but output costs 2.5 times as much. molde agents mostly read: cache writes are 71% of their cost and output 21%. The framework agents write more: output is 40–44% of their cost.

**Output, turns, time** (molde-check ÷ other arm, pooled):

| | hugo | astro | nextjs |
|---|---|---|---|
| output tokens | 0.66 | 0.87 | 0.54 |
| turns | 1.04 | 1.38 | 0.89 |
| wall time | 0.76 | 0.92 | 0.55 |

Wall time includes each tool's own build time, which favours molde against Next.js.

**Where molde's cost comes from, by task** (median new tokens, thousands, P1):

| | T1 | T2 | T3 | T4 | T4b | T5 | T6 | T0 | T10 |
|---|---|---|---|---|---|---|---|---|---|
| molde-check | 7.8 | 19.7 | 8.2 | 24.7 | 25.7 | 9.5 | 15.1 | 34.7 | 22.2 |
| of which docs | 2.1 | 13.4 | 4.2 | 14.0 | 16.8 | 4.9 | 8.1 | 27.6 | 16.4 |
| hugo | 9.3 | 10.9 | 7.4 | 11.1 | 10.6 | 8.5 | 19.9 | 16.7 | 14.2 |
| astro | 8.2 | 11.5 | 6.4 | 9.2 | 9.5 | 6.0 | 12.6 | 12.6 | 11.2 |
| nextjs | 11.4 | 17.9 | 8.1 | 14.3 | 16.6 | 7.6 | 22.9 | 25.0 | 21.6 |

**Reading the table:**
- **molde-check is cheapest on T1,** and cheaper than Hugo and Next.js on T6.
- **On T3 and T5 it costs more than all three,** though by less than on the other tasks. Its docs reads there are small (4–5k tokens).
- **On the open-ended tasks (T0, T4, T4b, T10),** the agent reads the whole reference and molde loses by the widest margin.

**Reading the docs.**
- 189 of 330 molde P1 runs read the whole reference: all T0, T4b and T10 runs, 49 of 50 T4 (one molde-mcp run didn't), 19 of 20 T2 and 16 of 20 T6, against 0 of 15 T8 and T11 and 1 of 20 T1.
- On T0, T4, T4b and T10, the three command-line molde arms read it before their first molde run in 95 of 120 runs, and in the same command in the other 25.
- The reference (61 KB) is larger than the output Claude Code shows in full. A `cat` of it returns a preview and a saved file, which the agent then reads again; `Read` returns it whole.

**Size of the change.** Diff lines + 1, without the generated `next-env.d.ts`, pooled over the core tasks (molde-check ÷ other arm, bootstrap 95%):

| hugo | astro | nextjs | hugo+checker | astro+checker | nextjs+checker |
|---|---|---|---|---|---|
| 0.63 [0.60, 0.66] | 0.51 [0.49, 0.52] | 0.45 [0.42, 0.51] | 0.63 [0.59, 0.65] | 0.51 [0.50, 0.54] | 0.49 [0.49, 0.51] |

- **Where the difference comes from.** It shows on the tasks that add something:
  - T6 (pagination) takes a median of 15 lines in molde, against 61 in Hugo, 91 in Astro and 154 in Next.js.
  - T2 takes 23, against 38, 64 and 95.
  - On the repair tasks (T4, T4b), molde, Hugo and Astro are the same size; Next.js T4b is larger (17 against 8).
- **Against the parity ports** (written by hand rather than by an agent, T1–T6), molde's changes are 0.20–0.68× the size. The result is not an effect of verbose agent-written ports.
- **What it does and doesn't show.** It reflects molde's declarative format: a paginated list is a few lines of `routes.yaml`, not a new page component. It is not a measure of quality.

**Does reading the whole reference pay for itself?** The data is mixed. On the tasks where only some molde runs read it all (T3, T9, T12), the cost without docs of those runs was sometimes higher and sometimes lower than the others'. In the largest cell, molde-check T3, one run against 14, it was higher (6.8k against 4.2k). There's no sign that it pays back within a run, but the samples are too small to say that it doesn't.

**Projection: docs read once per session.** Take each task's cost as its median without docs plus the docs median divided by *k*. For molde-check against hugo, astro and nextjs:

| *k* | 1 | 2 | 3 | 5 |
|---|---|---|---|---|
| ratios | 1.33 / 1.60 / 1.07 | 0.97 / 1.16 / 0.78 | 0.84 / 1.01 / 0.68 | 0.74 / 0.89 / 0.60 |

This is arithmetic, not a measurement. It ignores the cost of carrying the docs in context and assumes later tasks need no further reading. Round 2's session arm (B) measures it.

**molde-mcp** (secondary, never pooled with molde-check): 0.85× molde-check on T1–T6, and 0.87–1.26× the incumbents. Its docs reads are smaller (median 1.2–8.1k tokens per task), probably because `explain_route` and `list_routes` answer what the others look up.

**P2.** Adding "…and make sure the site meets the requirements" costs about 40% more for every arm alike (molde-check 1.44, hugo 1.38, astro 1.40, nextjs 1.42).

**Variability.** molde-check's per-cell coefficient of variation is 0.22, against 0.07–0.18 for the incumbent arms. Whether the agent reads the whole reference makes a big, all-or-nothing difference between runs.

**Checking by hand.** The agent says it checks something by hand in 28% of molde-check runs, 45% of molde+checker runs and 46–60% of the framework `+checker` runs. This is counted with a text pattern. The higher rate goes with the checker whose note lists what it doesn't see, which suggests (but doesn't show) that agents scope their own checking by that list.

## What could make this wrong

These are the strongest objections we could find to the result, including those of the audit. They cut both ways.

**Against molde (the result may understate its cost):**
- **The site is tiny.** With five pages and one data file, there is little for any tool to get wrong, and little for a complete report to find.
- **The tasks were written by molde's authors,** apart from T11 and T12, which other agents chose. Still, molde loses on most of them.

**For molde (the result may overstate its cost):**
- **The model's prior knowledge** is the incumbents' main advantage: Hugo, Astro and Next.js are in its training data, molde is not. That is the real situation today, but not a permanent one, and not one that holds for a team's own internal tools.
- **The registered metric weights all new tokens alike,** though output costs more than reading. At each run's real API cost, the gap narrows to 0.95–1.46×.
- **Every run is a fresh session,** so molde pays its learning cost on every task. Real agents often do several changes in a session.
- **Claude Code's output limit** turned most whole-reference `cat`s into two reads. That is a property of this harness and of molde's docs, not of molde itself.

**Neutral:**
- **There's a ceiling.** 1,170 of 1,200 runs pass, and 29 of the 30 failures are the benchmark's own defects. No agent ever said it was blocked. The tasks are too easy to tell the arms apart on quality or safety.
- **The thesis arm ran later,** three days after most of the arms it is compared with, against a service that may have changed. Arms within the campaign were interleaved; amendment 4 was not.
- **The T8 intervals** cross the band's lower edge for two arms, although the medians (0.82–1.00) sit inside it. With five runs per cell, the intervals are wide (nextjs: [0.41, 1.21]). The control can't rule out that the harness moves cost on a task where the generator shouldn't matter, but it shows no sign that molde is penalised there.
- **One model, one harness, one vendor** (DESIGN §11). The Sonnet row is a small robustness check, not a second model study. It agrees on the direction and makes the gap larger.
- **The docs counter** is an estimate: tokens are attributed to docs by their share of characters in a turn's tool results.
- **Analysis choices** not spelled out in PREDICTIONS.md: H1–H4 use P1 only; the Holm intervals' levels; H4's task set and level. They are all stated above, with the alternatives where they matter (H4).

## What round 2 should do

See `ROUND-2.md`. The data sharpens it:

1. **A: a short agent guide,** under the size Claude Code shows in full, telling the agent to run `molde check` first. It targets the 71% of molde's cost that is reading.
2. **B: sessions** of several tasks, measuring what the projection above only computes.
3. **C: off the ceiling.** A larger site, defects that reading a few files won't find, weaker models, and a task whose honest answer is "I can't". Without failures, safety can't be measured.
4. **Fix the defects above.** T6's wording, T8's wording, Next.js's generated files and not-found pages, a plan generated from the arm list, and one definition of every hypothesis shared by the text and the script.
5. **Register price-weighted cost** (each run's `total_cost_usd`) next to new tokens, so the comparison stops depending on a metric chosen before anyone looked at the cost profile.

## Reproducing

```
python3 analyze.py --set campaign-all --prompt P1      # registered H1–H4 (H4 as the script computes it)
python3 analyze.py --set campaign-all --prompt P2
python3 analysis/secondary.py --set campaign-all       # every secondary table above
```

`results/campaign-all` holds links to the run folders of `results/campaign` (the registered plan) and `results/campaign-mcheck` (amendment 4). H4 with T2, as PREDICTIONS.md defines it, comes from `analyze.py`'s own functions with the full core task list (see LOG.md, 19 Oct).
