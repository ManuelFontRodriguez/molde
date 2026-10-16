# Design v2: the agent-cost benchmark

2026-10-15. Replaces `DESIGN-v1.md` after adversarial review 1 (`REVIEW-1.md`). Amended the same day after review 2 (`REVIEW-2.md`): the `molde+checker` arm, the two-container judge, requirement 10, and the details in LOG.md. Every change is traced to a finding: B = blocking, I = important, M = minor. It is frozen and hashed together with `PREDICTIONS.md` before the pilot, and published before the campaign (§11).

## 1. The question

The thesis: frontend frameworks are developer environments built for people. When an agent writes the code, their layers stop paying for themselves, and primitives (data, Jinja templates, HTML) plus a complete check report are cheaper and safer to operate.

The build benchmark (`../cross-framework/`) measures the machine. This benchmark measures the **operator**: what one change costs an agent, and how often the agent ends a run believing it is done while the site is broken.

The thesis has two parts, and this design separates them (B1):

- **(a) Does a complete report help?** Compare molde with its check against molde without it, and each framework with a checker against the same framework without one.
- **(b) Do primitives beat frameworks when everyone has a report?** Compare molde with its check against Hugo, Astro and Next.js each with an equivalent checker.

Only (b) is the thesis. It is tested with the same checker on both sides: `molde+checker` against each framework `+checker` arm (review 2 B4: `site-check` is stronger than `molde check`, so comparing `molde-check` with them mixes two checkers with two generators). If (a) holds and (b) doesn't, the honest result is "a complete report helps every framework", and that is what gets published.

## 2. Arms

All arms use the same agent, model, tools, requirements text and prompts.

| Arm | Generator | Feedback available to the agent |
|---|---|---|
| `molde-check` (**primary molde arm**) | molde | `molde check`: build, every check, one JSON report |
| `molde-build` (ablation) | molde | `molde build` only. A shim refuses `check`, `mcp` and `events`, and the docs omit the check report sections. |
| `molde+checker` (**like-for-like comparator**, review 2 B4) | molde | `molde build` plus `site-check`, the same checker the framework arms get; no `molde check` |
| `molde-mcp` (secondary) | molde | `molde mcp` attached; reported apart, never pooled with `molde-check` |
| `hugo`, `astro`, `nextjs` | idiomatic ports (§4) | none: only the framework's own build |
| `hugo+checker`, `astro+checker`, `nextjs+checker` | idiomatic ports | the framework's build, plus `site-check`, a standalone checker with molde's report format (§6) |
| Parity panel: `zola`, `eleventy`, and the parity versions of `hugo`, `astro`, `nextjs` | the existing byte-parity ports | none |

The parity panel is reported apart (B4): those ports were bent to match a golden produced by molde, so they are harder to edit than normal code.

## 3. What the agent is told

**Requirements (B1).** `requirements.md` is one text, identical for every arm, stating the definition of done:

- every internal link and asset resolves;
- the sitemap lists exactly the site's pages;
- every page has one unique title;
- canonical and `og:url` equal the page's URL;
- text is escaped exactly once;
- JSON-LD parses;
- the site stays data-driven.

The oracle grades these stated rules, so they are the task's definition of done, not molde's worldview. Anything the oracle grades beyond them is reported separately as "unstated".

**Arm notes.** `arms/<arm>.md` is a short note in the same shape for every arm: "This site is built with X. Build: `<command>`. Output: `<dir>`. Check: `<command>` or 'none'. Docs: `<path>` or 'none'." It's passed with `--append-system-prompt-file`, because Claude Code doesn't read AGENTS.md (B3); the transcript confirms it arrived.

**Docs (I1):**

- molde agents get `docs/reference.md` as a local file. The model has never seen molde and can't work without it. In `molde-build`, the check sections are removed.
- Incumbents get no docs file: their docs are in the model's training data, and with no internet (§8) a URL would be a dead pointer.

That asymmetry is real and stated. molde pays for reading its docs inside its cost, and every headline is reported both with and without the tokens spent reading docs.

## 4. Ports

- **Idiomatic ports (B4).** Hugo, Astro and Next.js written the normal way:
  - Hugo: built-in sitemap and data-driven content adapters;
  - Astro: `@astrojs/sitemap` and normal components;
  - Next.js: App Router, `app/sitemap.ts`, the Metadata API.

  They are written by isolated agents with **no molde context**, from the spec and `data.json`, in the same container (a T0 run used as a port factory: §5). Each must pass the oracle. Each is frozen, published and open to experts' pull requests. Formatting differences from the golden don't matter, because the oracle parses HTML and never diffs bytes.
- **Data:** every port keeps all its data inside its workspace (B4 noted that the Next.js parity port read its data from outside).
- **Parity ports:** unchanged from the build benchmark, with the Next.js data moved inside the workspace.

## 5. Tasks

**Selection rule, fixed now (B5).** Three no-context agents each propose five "common changes to a data-driven listing site". Every proposal made by at least two of them that can be implemented and judged is added verbatim. The tasks below are the author's; the added ones are marked when they arrive.

| # | Task | Notes |
|---|---|---|
| T0 | Build the 8-file site from `spec.md` and `data.json` in an empty workspace with the toolchain installed. | Co-equal headline (B4). Pass = oracle. The golden diff is reported, not required. |
| T1 | Add a dividend-yield field (added to `data.json` by setup) to stock pages and the index table | |
| T2 | Add `/sectors/{sector}/` pages, linked from stock pages, in the sitemap | |
| T3 | Move stock pages from `/stocks/{TICKER}/` to `/acciones/{TICKER}/` | 15 repetitions. Redirects are out of scope (M). |
| T4 | Fix six planted defects: a broken href, a duplicate title, a missing canonical, a page missing from the sitemap, a double escape, a wrong `og:url` | 15 repetitions. Planted per port, documented per port, template chosen by a fixed seed. Detection rate reported per defect (B6). |
| T4b | The same, with defects **molde can't see**: a relative link, a single-quoted href, a CSS `url()` to a missing file, a wrong value copied into one page, a stale string in a template | 15 repetitions. Pre-registered: molde may do worse here (B6). |
| T5 | Add `og:description` from a new description pattern | |
| T6 | Paginate the index at 2 per page with previous/next links | |
| T7 | Add a JSON-LD `Corporation` block to stock pages | The oracle parses the JSON and checks `@type` itself, not through molde. |
| T8 | **Control:** make the "Estimated" badge red and bold | The CSS is at an equivalent place in every port (I6). Two-sided: the cost ratio must fall within [0.7, 1.4]. |
| T9 | **Expected loss:** a client-side search box filtering the index | Judged by a headless browser in the judge container (I5). |
| T10 | **Nothing to do:** "make sure the site meets the requirements", when it already does | Measures over-editing and the cost of verifying (I6). |

**Prompt variants:** P1 "Make this change." on everything. P2 "…and make sure the site meets the requirements before you finish." on T1–T6 for the four main arms only (M: P2 on two tasks was too small).

## 6. Judging, outside the agent's reach (B2, I5)

The judge runs in a **separate container** after the agent's container has stopped. It gets a read-only copy of the final workspace. It has the oracle, the golden files and the task answers; the agent never had any of them.

1. **Rebuild** with the arm's official build command, in a clean copy. For molde that is always a fresh `molde build`, never `out/current` (B1: a failed check leaves the last good build there).
2. **The oracle** (`oracle/oracle.py`, plain Python, no molde code) grades:
   - the stated requirements (§3);
   - the task's assertions (page counts, content present, the new URLs, no old URLs);
   - the unstated qualities, reported apart.

   The page set is defined precisely: HTML files, excluding 404 pages, `_next/` and non-HTML files.
3. **The data-mutation test (I5):** change a value and add a row in `data.json`, rebuild, and require that the pages follow. This catches answers hard-coded into templates.
4. **A second, independent judge:** lychee (offline, internal links) and an HTML validator on every output. Disagreements between the two judges are reported.
5. **Oracle validation, before the pilot:**
   - it passes on all correct ports, including the incumbents' idiomatic output with its own markup quirks;
   - it fails on every planted defect, in every port;
   - its hash is frozen.

`site-check` (the checker the `+checker` arms get) is a **different program** from the oracle, written separately. It checks what `molde check` checks and prints a report in molde's format. It is not the oracle, so an agent with a checker is still not looking at its exam.

## 7. What is measured

The data comes from the `result` event of `stream-json` (B3, I3): `num_turns`, `usage`, `modelUsage`, `total_cost_usd`, `duration_ms`, `duration_api_ms`, `is_error`, `subtype`, `terminal_reason`. Per-message usage is never summed (it repeats across content blocks).

**Outcome classes:**

- `pass` / `fail` (oracle);
- `censored` (hit the turn or budget limit);
- `infra_fail` (auth, rate-limit exhaustion, an API error): **re-run**, never counted against an arm (I3).

**Primary measure (B5): new tokens per attempt** = input + cache creation + output, over **all** attempts, failed ones included.

- Unit: the task.
- Statistic: per-task ratio of medians, `molde-check` against each other arm, pooled over T1–T6 with a task-level bootstrap and Holm correction.
- Cache reads are reported separately: they grow with conversation length, not with work.
- The dollar figure is labelled "API-equivalent estimate".

**Secondary measures:**

- success rate;
- **false-done rate**, defined in one sentence on the report's first line: the agent ended normally and said it was done, but the oracle failed (M: plain name). It is split into:
  - **(a)** a failing signal was shown to the agent (a red check report) and it still said done;
  - **(b)** no signal existed (B6).
- tool calls by type and turns;
- time spent in the model (`duration_api_ms`); wall-clock is not a headline (I3).
- tool calls classified into reading docs, exploring, editing and verifying (I1).

**`claimed_done` (I4)** is structural: the run ended with `subtype: success` (not a limit, not an infrastructure error), and the final message doesn't say it was blocked or unable. The rule is validated on a blind sample of at least 40 final messages, labelled by two people without arm labels. Agreement (kappa) is published.

## 8. Isolation (B2, B3)

**The agent's container holds only:**

- the toolchain and the pre-installed dependencies (`node_modules` symlinked read-only from the image);
- its workspace;
- the arm's docs file.

No oracle, golden, other ports or bench folder.

**Network:** the agent's container sits on an internal Docker network whose only exit is a filtering proxy that allows Anthropic's API hosts and nothing else. `npm install`, `curl` and `pip` to the internet fail for every arm alike. Telemetry is off for Claude Code, Next.js and Astro.

**Claude Code flags:**

```
-p … --model <full id> --effort <pinned> --output-format stream-json --verbose
--tools "Bash,Read,Edit,Write" --strict-mcp-config [--mcp-config molde.json] --disable-slash-commands
--permission-mode bypassPermissions --max-turns 100 --no-session-persistence
--append-system-prompt-file arm.md
```

- Environment: `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`, `DISABLE_AUTOUPDATER=1`, `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`, plus a fresh config folder per run (the container is deleted after every run).
- The tool set excludes sub-agents and to-do lists, which would add uncontrolled calls.
- A run is rejected and re-run if `system/init` shows unexpected plugins, skills, hooks or MCP servers, or if `modelUsage` shows a model other than the pinned one.
- The user is non-root.
- The Bash tool has a timeout, so an agent that starts a dev server in the foreground can't hang a run; such timeouts are counted.

**The credential** reaches Claude Code on file descriptor 3 (`CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR`), never in an environment variable, so `env` and `printenv` cannot show it and the agent's commands do not inherit the descriptor. As a backstop, token-shaped strings are scrubbed from every saved file, and before anything is published every transcript is searched for `sk-ant-` and for `env`/`printenv` calls. Probes verify that no process environment holds the credential. The credential is revoked after the campaign.

## 9. Size

- **Main arms:** `molde-check`, `molde-build`, `hugo`, `astro`, `nextjs`, `hugo+checker`, `astro+checker`, `nextjs+checker`.
  - 8 arms × (T0–T2, T5–T10: 10 tasks × 5 repetitions + T3, T4, T4b: 3 tasks × 15 repetitions) = 8 × 95 = **760 runs**.
- **P2:** 4 arms × 6 tasks × 5 = 120.
- **`molde-mcp`:** T1–T6 × 5 = 30.
- **Parity panel:** 5 arms × T1–T6 × 5 = 150.

| | Runs |
|---|---|
| Core (main arms) | 760 |
| P2 variant | 120 |
| `molde-mcp` | 30 |
| Parity panel | 150 |
| **Total** | **≈ 1,060** |

**The pilot** (discarded from the analysis): `molde-check`, `molde-build`, `hugo`, `hugo+checker`, `nextjs`, `nextjs+checker` × T3, T4 × 3 = 36 runs. It calibrates token use, the caps, the subscription's limits, the oracle and the classifier.

**If the subscription can't sustain the full campaign,** cut in this order:

1. the parity panel;
2. P2;
3. `molde-mcp`;
4. repetitions on T0–T2 and T5–T10 down to 3.

The core comparison is never cut.

**Caps:** `--max-turns 100`. The budget cap is set at 3× the pilot's 95th percentile. Hits are `censored`, not failures (I3).

**Order:** fully randomised over (arm, task, prompt, repetition) with a fixed, published seed: `20261016` for the campaign and `20261017` for the pilot (`runner/campaign.py plan --seed`). No blocks by arm, so cache warmth is shared fairly (I3). One run at a time, so Node builds don't compete for CPU.

## 10. Model

One pinned full model ID for the whole campaign: `claude-opus-5-5` (provisional), with `--effort` pinned. A robustness row with `claude-sonnet-5-5` on T3 and T4 for the main arms. This is one vendor's agent harness; the report says so plainly (I8).

## 11. Pre-registration and schedule (B5)

Frozen together **before the pilot**: committed locally and hashed (the SHA-256 lists are in LOG.md), then published unchanged before the campaign. Freezes 1 and 2 were made locally; the public pre-registration is the published version of these files, published before the campaign. The pilot ran before any public freeze and is excluded from the analysis.

- this design;
- `PREDICTIONS.md`;
- `requirements.md`, the arm notes and the task texts;
- the oracle and its self-test;
- the `claimed_done` rule;
- the seed.

| Step | When |
|---|---|
| Build everything; produce the idiomatic ports (factory runs); validate the oracle, plants and reference solutions; review 2; a smoke test outside the measured set | done 15–16 Oct |
| Freeze (local; SHA-256 in LOG.md) | done 16 Oct |
| Publish the freeze unchanged (public repository), before the campaign | after the freeze |
| Pilot (discarded; ran 16 Oct, before any public freeze). Changes after the pilot are allowed only to infrastructure, never to tasks, oracle or analysis; any change is logged and the freeze re-published | done 16 Oct |
| Campaign; then `analyze.py` reproduces every table from the published JSONL, and the privacy grep | after the pilot, sized by its usage |
| Report | after the analysis |

## 12. Publication

Published:

- every run's JSONL and its `system/init` event;
- full model ID, Claude Code version, image digest, tool list, the appended prompt and the effort level;
- lockfiles;
- per-run tables and strip plots (every run visible, not only medians);
- `analyze.py`.

Since others reproducing the runs would spend their own quota and get stochastic results, the analysis is exactly reproducible from the published data even when the runs are not.
