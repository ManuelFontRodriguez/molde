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

## What round 1's campaign adds (19 Oct; REPORT.md)

1. **A is the right first change, and the data says how small.** Reading is about 71% of molde's cost at real API prices. On every open-ended task (120 of 120), agents read the 61 KB reference before their first molde run or in the same command. Claude Code shows that much output only as a preview and a saved file, so most whole reads happen twice. The guide should:
   - stay under the size Claude Code shows in full (well under 25 KB; aim for about 3k tokens);
   - say "run `molde check` first";
   - cover only what agents looked up: routes.yaml, the template contract, URL conventions, static/, site.yaml and partials.
2. **Register price-weighted cost** next to new tokens: each run's `total_cost_usd` (Claude Code: input 1, one-hour cache write 2, cache read 0.05, output 5). The two metrics disagree in round 1 (new tokens 1.08–1.62×, real cost 0.95–1.46×). Round 2 should name in advance which one it tests.
3. **Register the size of the change** (diff lines without generated files) as a secondary outcome. In round 1 it is the largest and tightest difference (molde 0.45–0.63×), found after seeing the data.
4. **B needs a definition of a session** that copes with the docs staying in context: report the cache reads as well as the new tokens, so the cost of carrying the docs is counted.
5. **Fix round 1's defects** before freezing:
   - T6 states that the home page lists page 1 only;
   - T8 says "text";
   - Next.js's `next-env.d.ts` and `404.html`;
   - the plan is generated from the arm list;
   - the docs counter also covers MCP and guide reads.
6. **C is needed to say anything about safety.** On the safety tasks (T3, T4, T4b) under P1, round 1 has 0 false-dones. Of the 30 elsewhere, 29 are the benchmark's own defects and 1 is a genuine agent error.
7. **Add a task where the honest answer is "I can't".** Every one of round 1's 1,290 runs ended with "done", so the `claimed_done` rule has never been tested on a negative, and false-done can't be measured. Two candidates:
   - a requirement that conflicts with the data, such as "every stock page shows its yield" when some stocks have none and requirement 8 forbids inventing data;
   - a change the tool can't make within the rules.

   Agents that say so are right. Agents that claim done are measurable false-dones. Every arm gets the same task.

## Open questions

1. Which of A–C go into round 2 (all, or A and B first)?
2. Is a short agent guide a change molde wants anyway (it is also the README's first
   impression for agents)?
3. The size budget: round 1 is ~1,060 runs; round 2 as sketched is a few hundred more.
