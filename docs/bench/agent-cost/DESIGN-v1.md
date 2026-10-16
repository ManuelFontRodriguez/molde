# Design: the agent-cost benchmark (and the build benchmark, rerun next to it)

Draft, 2026-10-15. Not built and not run. Per molde's own rule ("every design is attacked before it is built"), this design needs an adversarial review before any code is written.

## 1. Why, and what it must prove

The thesis: frontend frameworks are developer environments built for people. When an agent writes the code, their layers (React, TypeScript, a bundler, plugins) stop paying for themselves. Primitives (data, Jinja, HTML) plus a build that reports every error at once are cheaper and safer to operate.

The existing benchmark (the build benchmark in `../cross-framework/`) measures the **machine**: install size, lines authored, build milliseconds, fidelity and checks. It cannot test the thesis. The thesis is about the **operator**: what one change costs when an agent makes it, and how often the agent ships a broken site while believing it is done.

molde's design log named this "cheapest site to operate with an agent, measured in tool calls and tokens per change: a benchmark category with no incumbent". It also said the bench spec "already collects `agent_tool_calls`". **It doesn't any more:** today's `spec.md` and `*.metrics.json` have no agent fields. So this is a new experiment, not a column to fill in.

**The two benchmarks, side by side:**

| | Build benchmark (exists) | Agent benchmark (new) |
|---|---|---|
| Question | What does the machine cost? | What does a change cost, and how safe is it? |
| Unit | One clean full build | One task completed by one fresh agent |
| Main numbers | ms, MB, packages, lines | Success rate, **silent-failure rate**, tool calls, tokens, cost, wall-clock per successful change |
| Where | The host machine (Apple M4 Pro, 24 GB) | One Linux container, which also reruns the build benchmark on Linux |

Running both from the same container image answers two expected critiques ("macOS only", "the machine wasn't idle") and puts both reports on one footing.

## 2. Hypotheses, written down before any run

Commit these to the repo **before** the first full run, and publish the result whichever way it goes. This follows the falsifiable-criteria style of molde's design log.

- **H1 (cost):** on the core tasks (T1–T6), molde's median tool calls per *successful* change are at most **half** the median of the five other generators. The same comparison is reported for tokens.
- **H2 (safety):** on the tasks where breakage is easy to miss (T3 rename, T4 planted defects), molde's **silent-failure rate** is lower than every other generator's. A silent failure is a run where the agent says it is done and the independent check fails.
- **H3 (honesty controls):** on T8 (CSS only) molde is **no better** than the median, and on T9 (client-side interactivity) it does **not** beat Next.js. If molde "wins" the controls, suspect the harness.

Scoring:

- H1 and H2 hold and H3 behaves: the thesis is supported.
- Only H2 holds: say "safer, not cheaper".
- Neither holds: publish that, and drop the cost claim.

## 3. Arms (who gets tested)

Seven arms, each starting from the existing port in `harness/sites/<fw>/`, which already reproduces the golden site:

1. molde, CLI only (Bash)
2. **molde with MCP** (`molde mcp` attached through `--mcp-config`)
3. Hugo
4. Zola
5. Eleventy
6. Astro
7. Next.js

**Fair-context rule.** Every workspace gets one `AGENTS.md` with the same structure:

- "This site is built with X."
- The build command.
- The output folder.
- Where the docs are.

molde's points at a bundled copy of `docs/reference.md`, because the model has never seen molde. The others point at their official docs URL, and the model already knows them from training. That gives the incumbents a real advantage, and it is the honest one: it's the world molde is released into. Docs tokens read by the molde agent count against molde.

## 4. Tasks

Ten tasks (T0–T9). T0 builds the site from the spec; the rest are changes people really make to a data-driven site. The bias guard: **a fresh agent with no molde context also proposes tasks** from the brief "common changes to a data-driven listing site"; we keep at least two of its tasks. All task texts are published.

| # | Task (prompt essence) | What tends to break silently |
|---|---|---|
| T0 | **Build it:** reproduce the 8-file site from `spec.md` and `data.json`, starting from an empty workspace that has only the framework installed. This is the first benchmark's own task, so both benchmarks share one finish line. Pass = the oracle passes; the golden diff (`compare.py`) is reported beside it, not required, because Next.js can't reach byte parity (75 lines) and the golden was produced by molde. | Sitemap, head tags, escaping |
| T1 | Add a new data field (dividend yield, added to `data.json` in setup) to every stock page and to the index table | Null handling, escaping |
| T2 | Add `/sectors/{sector}/` pages listing their stocks; link each stock page to its sector; include them in the sitemap | Sitemap coverage, dangling links |
| T3 | Move stock pages from `/stocks/{TICKER}/` to `/acciones/{TICKER}/` | Old links, canonical, `og:url`, sitemap |
| T4 | "Users report problems with the site. Find and fix them." Six planted defects, identical across ports: a broken href, a duplicate title, a missing canonical on one template, a page missing from the sitemap, a double-escaped `AT&amp;amp;T`, a wrong `og:url` | Everything; the agent must find them |
| T5 | Add an `og:description` from a new description pattern on every page | Missing on one template |
| T6 | Paginate the index at 2 stocks per page with previous/next links | Page 1 vs `/page/1/`, sitemap |
| T7 | Add a JSON-LD `Corporation` block to every stock page | Invalid JSON, escaping |
| T8 | **Control:** make the "Estimated" badge red and bold (CSS only) | Nothing; parity expected |
| T9 | **Expected loss:** add a client-side search box that filters the index list | Natural for Next.js; molde needs vanilla JS |

**Two prompt variants for every task:**

- **P1, natural:** "Make this change."
- **P2, careful:** "…and before you finish, make sure the site has no broken links and that the sitemap and head tags are correct."

P2 is fair to the incumbents: it tells their agents to verify, so we measure what verification costs them, not whether they think of it.

## 5. The judge: an independent oracle (not `molde check`)

molde must not grade itself. Write `harness/agent/oracle.py` in plain Python with `html.parser`, with no molde code. After each run the **harness**, not the agent, rebuilds the site with the framework's official build command and judges that output.

**Generic invariants, on every task:**

1. Every internal `href` and `src` resolves to a file in the output.
2. The set of URLs in the sitemap equals the set of HTML pages, excluding 404 pages.
3. Each page has exactly one `<title>`, and titles are unique.
4. canonical == `og:url` == the page's own expected URL.
5. Correct escaping: `AT&amp;T` appears; `&amp;amp;` never does.
6. The build exits 0.

**Task assertions:** for example, T1 "the yield appears on all 4 stock pages and in the index"; T3 "no file under `/stocks/`, 4 under `/acciones/`, and no link to `/stocks/` anywhere".

The oracle parses HTML and doesn't diff it, so Next.js's residue (minified head, `data-next-head`) doesn't count against it.

**Validate the oracle before trusting it** (a mutation test):

- It must **pass** on all six golden-equivalent outputs.
- It must **fail** on each of the six T4 defects planted one at a time, in every port.
- Optional, and a nice touch: also run `lychee` over every output as a second, third-party link check.

## 6. What is recorded per run

Claude Code's headless mode reports most of this directly. `claude -p … --output-format stream-json` streams every tool call and ends with a summary of turns, token usage and cost [I: field names to confirm in the pilot]. The harness adds the rest.

| Field | Source |
|---|---|
| `success` (all oracle checks pass) | oracle |
| `claimed_done` | agent's final message, classified by the harness with a simple rule plus a manual spot check |
| `silent_failure` = claimed_done and not success | derived |
| `assertions_passed / total` | oracle |
| `tool_calls` (by tool), `turns` | stream-json |
| `input_tokens`, `output_tokens`, `cache_read`, `cache_write` | stream-json summary |
| `cost_usd` | stream-json summary |
| `wall_clock_s` | harness |
| `build_invocations` | count of Bash calls matching the build command |
| `diff_lines`, `files_touched` | `git diff --stat` in the workspace |
| model ID, Claude Code version, framework version, task, prompt variant, repetition | harness |

**Headline metrics:**

- **cost per successful change** (tokens, and $) = sum over runs / number of successes;
- **silent-failure rate**;
- **median tool calls per success**.

Report medians with bootstrap 95% intervals, never single runs.

## 7. Size and budget

- **Agents vary run to run**, so use 5 repetitions per cell.
- **Full run:** 7 arms × 9 tasks × 2 prompts × 5 repetitions = **630 runs**. That's too many to start with, so:
  - **Pilot first:** 4 arms (molde CLI, molde MCP, Hugo, Next.js) × 2 tasks (T3, T4) × P1 × 3 repetitions = **24 runs**. It calibrates the real token cost, the oracle and the classifier.
  - **Then the core run:** 7 arms × T1–T6, T8, T9 × P1 × 5 = **280 runs**. P2 only on T3 and T4 (7 × 2 × 5 = 70 more). T7 if the budget allows.
- **Budget caps** (computed ceilings, not estimates): every run gets `--max-budget-usd`, and a run that hits the cap counts as a failure. The real cost will be lower; the pilot's measured `cost_usd` is the only number to plan with, since per-token prices are not assumed.
- **Model:** one pinned model for the whole run, since a mid-run change invalidates the comparison. The model most target users run in Claude Code day to day. A second model on the pilot tasks only, as a robustness check, is optional.

## 8. Isolation (privacy and safety)

This part is not optional.

- **`--bare`** for every agent. Without it, each benchmark agent would load the host user's settings, hooks, CLAUDE.md files and memory. That would contaminate the results *and* leak the host user's private context into 600 transcripts that are meant to be published. `--bare` refuses OAuth, so it needs an **`ANTHROPIC_API_KEY`**: this is API billing, not a subscription.
- **Update (15 Oct): use the Claude subscription instead of an API key.** Inside the container, Claude Code runs with an empty config folder and a long-lived subscription token from `claude setup-token` (design v2 §8 describes how it is passed). The empty folder gives the same isolation as `--bare`, because there are no settings, hooks, CLAUDE.md files or memory to load. Measurement comes from Claude Code's own `stream-json` output (turns, tool calls, tokens), so no proxy is needed. Under a subscription, `cost_usd` is the API-price equivalent, not what is paid, so the report leads with tokens. The subscription's usage limits pace the runs over several days.
  - **No other provider for now:** the robustness check at the bottom uses a second Claude model on the pilot tasks (for example Sonnet next to Opus).
- **A Linux container.** The agents run with `--permission-mode bypassPermissions`, which is acceptable only inside a disposable container with no access to the host's home folder. Each run gets:
  - a fresh copy of the port, with `git init` and dependencies already installed (the install is the build benchmark's job, not the agent's);
  - tools `Bash, Read, Edit, Write, Glob, Grep`;
  - `WebSearch` and `WebFetch` disallowed, the same for every arm;
  - `--max-turns 80`;
  - `--no-session-persistence`.
- **Published transcripts** come from that clean container, so they contain only the benchmark workspace. Grep them for anything personal before publishing anyway.

## 9. Runner sketch

```
docs/bench/cross-framework/
├── harness/            # the build benchmark (exists)
└── agent/              # new
    ├── Dockerfile      # Rust toolchain + molde, Hugo, Zola, Node 22 + the 4 Node ports pre-installed, lychee, claude CLI
    ├── tasks/T1.md … T9.md, setup/T1.sh … (data changes, T4 defect plants per port)
    ├── agents-md/<fw>.md
    ├── oracle.py       # independent judge + its self-test (mutation test)
    ├── run_agent.py    # (arm, task, prompt, rep) -> one JSONL record + transcript
    ├── analyze.py      # medians, bootstrap CIs, H1–H3 verdicts, tables for report.md
    ├── PREDICTIONS.md  # H1–H3, committed before the full run
    └── results/        # agent-results.jsonl, transcripts/
```

One run, in outline:

```sh
cp -r sites/$FW /work/run && cd /work/run && git init -q && git add -A && git commit -qm base
bash ../agent/setup/$TASK.sh $FW            # data change or defect plant
claude -p "$(cat ../agent/tasks/$TASK.md)" --bare --model "$MODEL" \
  --output-format stream-json --verbose --max-turns 80 --max-budget-usd 2 \
  --allowedTools Bash,Read,Edit,Write,Glob,Grep --disallowedTools WebSearch,WebFetch \
  --permission-mode bypassPermissions --no-session-persistence \
  $( [ "$ARM" = molde-mcp ] && echo --mcp-config ../agent/mcp-molde.json ) > transcript.jsonl
<official build command for $FW>             # harness rebuilds; never trust the agent's output dir
python3 ../agent/oracle.py $TASK $OUTDIR > verdict.json
```

The same image then runs `harness/run.py` for the build benchmark on Linux.

## 10. Threats to validity, and the answer to each

| Critique | Answer |
|---|---|
| "molde's author picked the tasks" | A no-context agent proposes tasks too; control tasks and an expected-loss task are included; all task texts are published before the results. |
| "molde graded itself" | An independent Python oracle, mutation-tested; plus lychee. |
| "The model already knows Next.js; unfair to molde" (or the reverse) | Both are true and stated. molde pays for reading its docs, inside the cost. |
| "One model, one day" | Pin and publish the model ID and Claude Code version; optional second model on the pilot tasks. |
| "Agents are random" | 5 repetitions, medians, intervals; all transcripts published. |
| "Prompt caching distorts cost" | Report raw tokens by type next to dollars. |
| "The ports were written by the same author" | The ports already exist at byte parity (build benchmark). Agents start from them; experts are invited to improve them. |
| "Tiny site" | It is the same 8-file site as the build benchmark: say so. Optionally rerun T3 on the 1,900-stock dataset (timing grows, the logic doesn't). |

## 11. Order of work

The order is method; the calendar dates of the first draft are dropped.

| Step | What |
|---|---|
| 1 | Adversarial review of this design (molde rule), then fold the fixes in. |
| 2 | Dockerfile, oracle and its mutation test, runner, T3/T4 setup scripts. |
| 3 | **Pilot** (24 runs). Check costs, the oracle and the "claimed done" classifier. |
| 4 | Remaining tasks; commit `PREDICTIONS.md`. |
| 5 | Core run (about 350 runs; it can run unattended within the caps). Rerun the build benchmark in the same image. |
| 6 | `analyze.py`, `report.md` section, and the privacy grep of transcripts. |
| 7 | Publish the report. |

In this first draft the predictions were committed after the pilot; review 1 (B5) rejected that, and design v2 freezes them before it.

## 12. Open decisions in the first draft

1. **The spend ceiling** for the pilot and the core run, to be set after the pilot from its measured usage.
2. **The model** to pin.
3. **Run in a container**: yes. Agents with bypassed permissions run only inside a disposable container, never directly on the host.
4. **Publish whatever comes out**, including a falsified H1: yes. The credibility of the whole benchmark rests on it.
5. **Where it lives:** this design under `docs/design/agent-bench.md` and the code under `docs/bench/cross-framework/agent/`. At the time it existed only outside the repository.
