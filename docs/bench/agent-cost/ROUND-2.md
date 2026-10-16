# Round 2: draft

**Status: DRAFT, undecided. Nothing here is registered or run.** Round 1 is
the frozen design (DESIGN.md, PREDICTIONS.md, freeze 1 in LOG.md), run and reported
as registered. Round 2 is a second, separately pre-registered round that the report
labels as designed **after** seeing the pilot. It answers what the pilot raised; it never
replaces round 1's numbers.

## What the pilot showed (36 runs, calibration only)

1. **The cost of molde is learning it, not using it.** Every molde run read the whole
   reference (about 65 KB, 16–27k tokens). With those tokens, molde-check used 1.6–2.1x the
   tokens of the other arms on T3 and T4; without them, 0.58–0.74x. The other agents read
   no docs: the model already knows Hugo and Next.js.
2. **A ceiling.** Every arm passed every run, and no agent ever claimed done on a broken
   site. The safety measure (false-done) had nothing to measure.

## Proposed changes, each with the question it answers

### A. Onboarding cost: a short agent guide (a molde product change)

Question: how much of molde's cost is a documentation problem that molde can fix?

- molde ships a task-oriented guide for agents, about 2–3k tokens: the site layout, the
  routes file, the template variables, the commands, and how to read a failing report.
  The full reference stays for details. The arm note points to the guide first.
- molde is re-pinned to the version that adds it; the change is a normal molde pull request,
  reviewed like any other, and is useful outside the benchmark.
- Reported: the docs tokens per run, before and after, and H1 with and without them.

### B. Sessions, not single changes

Question: what does a change cost once the agent already knows the tool?

- A session gives one agent 4–5 tasks in a row on the same site (for example T1, T5, T3, T7,
  then T4), in one conversation. The docs are read once; the cost per change after the
  first is the steady-state cost.
- This is how agents are used on real sites: one session, several edits.

### C. Off the ceiling

Question: when do agents claim done on a broken site, and does the report change that?

- **A larger site.** The 1,901-page site from the build benchmark, or the three-language
  example, instead of five pages: defects spread over hundreds of pages can't be checked
  by reading a few files.
- **Harder tasks.** A rename whose old URLs are still in the content data; a new locale
  with hreflang; a data change that breaks escaping on some pages only; defects planted on
  2 of 300 pages.
- **Weaker and cheaper models.** A row with `claude-sonnet-5-5` and one with
  `claude-haiku-5-5`, which agent loops use at scale, and where false-done is likelier.
  The pilot's cost (about 0.17% of a five-hour window per Opus run) leaves room.

### D. The same discipline

- Round 2 gets its own PREDICTIONS file, frozen and hashed before its own pilot, with
  the same oracle machinery, the same judge and the same analysis script.
- The report shows both rounds side by side, round 1 first.

## Open questions

1. Which of A–C go into round 2 (all, or A and B first)?
2. Is a short agent guide a change molde wants anyway (it is also the README's first
   impression for agents)?
3. The size budget: round 1 is ~1,060 runs; round 2 as sketched is a few hundred more.
