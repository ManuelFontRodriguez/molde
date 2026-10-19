# Round 2A: the short agent guide, before and after

**Status: REGISTERED (19 Oct 2026), before any round 2A run.** The SHA-256 of this file,
of `analysis/round2a.py`, of `runner/round2a-plan.sh` and of the arm table
(`runner/arms.py`) is recorded in LOG.md at the commit that freezes them. Nothing here
changes after the freeze; any later change is a dated entry in LOG.md.

Round 2A is a small follow-up to round 1 (REPORT.md). It is not the full round 2
(`ROUND-2.md`: sessions, a harder site, an impossible task), which comes later.

## What was known when this was written

Round 1 found that molde-check costs agents 1.08–1.62× the new tokens of Hugo, Astro
and Next.js, mostly because they read molde's 61 KB reference first.
- **Without the docs tokens:** 0.47–0.71×.
- **At real API cost:** 0.95–1.46×.

molde then gained a short guide (`docs/agents.md`, printed by `molde guide`, about 2k
tokens).
- **How it was written:** from the sections round 1's agents looked up (routes,
  templates, URL rules, the check report), not from any task's wording.
- **It was not tested** on any task before this registration.
- **Disclosed risk:** the people who wrote it had read round 1's transcripts, so it may
  fit these tasks better than a guide written cold would. The tasks are round 1's,
  unchanged, so that the comparison is like for like.

The molde under test also has the check changes made with the guide: absolute self-links, duplicate head tags, `og:title`, any quoting and robots.txt. Both molde arms run the same binary, so those changes affect them alike.

## Question

Does pointing the agent at the short guide reduce what molde costs it, and does molde
then cost less than the incumbents?

## Arms (`runner/arms.py`)

| Arm | What it gets |
|---|---|
| `molde-check` | as in round 1: the note's docs line names the full reference |
| `molde-check-guide` | the same, except the docs line says: run `molde guide` for a short guide; the full reference is there for details |
| `hugo`, `astro`, `nextjs` | as in round 1, re-run now, as concurrent controls |

The same model (`claude-opus-5-5`, effort `high`), harness, requirements, judge and
oracle as round 1. The image is built from the commit that freezes this file.

## Tasks and size (`runner/round2a-plan.sh`)

- **Tasks:** T0, T1, T2, T3, T4, T4b, T5, T6 and T10, unchanged. These are the core tasks, plus T0 (build from empty) and T10 (nothing to do), where round 1 read the whole reference in every run.
- **Size:** 5 arms × 9 tasks × 5 repetitions = **225 runs**, P1 only, in an order shuffled by seed 20261020 so that every arm meets the same service conditions.
- **T6 keeps its known defect** (requirement 10). Its pass rate is not interpreted; its cost is.

## Predictions

The core tasks are T1–T6 with T4b, as in round 1. The ratio is the pooled ratio of round 1: the geometric mean over tasks of the per-task median ratio. The interval is a 95% bootstrap interval (10,000 resamples within each arm × task cell, seed 1016).

**A1 (primary): the guide reduces molde's cost.**
- **Test:** `molde-check-guide` ÷ `molde-check`, new tokens, core tasks.
- **Holds when** the upper bound is below 1.0.
- **Our guess:** about 0.6.

**A2: the guide replaces the reference.** On T0, T4, T4b and T10, fewer than 20% of `molde-check-guide` runs read the whole reference (round 1: 100%), counted as `analysis/secondary.py` counts it.

**A3 (the headline): molde with the guide against the incumbents.**
- **Test:** `molde-check-guide` ÷ each of `hugo`, `astro`, `nextjs`, new tokens, core tasks, Holm-corrected over the three comparisons as in round 1.
- **Holds against an arm when** the corrected upper bound is below 1.0.
- **Our guess:** it holds against Hugo and Next.js, not against Astro.

**A4: the same at real API cost.** A3 with each run's `total_cost_usd` instead of new tokens. It is registered now because round 1 showed the two metrics can disagree.

**Reported, not tested:**
- docs tokens per run;
- whole-reference reads by task;
- T0 and T10, each on its own;
- pass rates (with T6's defect noted);
- diff size;
- `molde-check` ÷ incumbents now against round 1, as a drift check.

**What would make us distrust the result:**
- any run reaching the real molde from a shim (not applicable: no shim arm);
- a secret in a transcript;
- `infra_fail` above 10% in any arm;
- `molde-check` ÷ incumbents moving by more than ±0.2 from round 1. That would mean the service drifted, and the comparison then rests on the concurrent controls only.

## Analysis

`analysis/round2a.py`, frozen with this file. Standard library only. It reads
`results/round2a/*/metrics.json`.
