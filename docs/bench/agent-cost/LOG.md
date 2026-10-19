# Lab notebook: the agent-cost benchmark

Every step, decision, failure and number, in order, as it happens. Nothing here is edited after the fact; corrections are new entries. The one exception is a single editorial pass before publication, which removed private context (names, schedules, credential handling details, money figures, local commit identifiers) and left every method step, number and finding as recorded. Times are CEST until 25 Oct 2026 and CET after.

## 2026-10-15

**Origin.** The idea comes from molde's design log (17–19 Sep 2026): measure "the cost of one change made by an agent, in tool calls and tokens" for each framework, a category no benchmark covers. The build benchmark (`../cross-framework/`) measures the machine; this one measures the operator.

**The design** is in `DESIGN.md`. It was first written outside the repo on 2026-10-15, then copied here unchanged at the start of the work.

**Correction to an earlier claim.** The design log said the build benchmark's spec "already collects `agent_tool_calls`". Checked today: `../cross-framework/spec.md` and the six `*.metrics.json` have no agent fields. The number was recorded once during the September port work and never kept. So this is a new experiment, not a missing column.

**Base version.** molde as of 15 Oct, the latest at the time (before the changes listed under "molde changed" below). The pinned molde for the campaign is stated in the 16 Oct entry.

**Decisions taken today** (by the author unless marked):

- **Agent:** Claude Code 2.1.300, headless (`claude -p --output-format stream-json`), the same for every framework.
- **Auth:** a claude.ai subscription, through a long-lived token from `claude setup-token`. It is never written to an image or a log. The first design passed it as an environment variable; on 16 Oct this was replaced by file descriptor 3 (see below). No API key and no other provider is used.
- **Isolation:** every agent runs in a Linux container with an empty Claude Code config folder, so no user settings, hooks, CLAUDE.md files or memory can reach it. This replaces `--bare`, which refuses subscription logins.
- **Model** (provisional, by Claude, to be confirmed): `claude-opus-5-5` as the main model, `claude-sonnet-5-5` as the robustness row on the pilot tasks.
- **Process rule** (molde's own): the design gets an adversarial review before any task or checker code is written. Review 1 started today; its result goes to `REVIEW-1.md`.
- **Infrastructure started in parallel** with the review, because its findings can't change it: the container image (the toolchains at the same versions as the build benchmark).

**Environment.**

- Host: Apple M4 Pro, 24 GB, macOS 27.
- Docker Desktop 28.3.2, arm64 Linux VM, 12 CPUs, about 8 GB of memory.
- Upstream versions checked today. Linux arm64 releases exist for:
  - Hugo 0.168.0 (`hugo_extended_0.168.0_linux-arm64.tar.gz`). The macOS run used the "extended + withdeploy" build, which has no arm64 Linux release, so "extended" is used. The difference is the deploy command only, which the benchmark never calls.
  - Zola 0.23.6 (`zola-v0.23.6-aarch64-unknown-linux-gnu.tar.gz`).
  - Claude Code 2.1.300 on npm.

**Image build 1** (`docker/build.sh --allow-dirty`, from molde as of 15 Oct plus the uncommitted benchmark files): succeeded, 2 GB. The versions recorded inside the image (`/opt/versions/all.txt`):

- molde 0.1.0-rc.1 (source of 15 Oct, with uncommitted benchmark files)
- hugo v0.168.0+extended linux/arm64
- zola 0.23.6
- node v22.15.0
- Claude Code 2.1.300
- Eleventy 3.1.6, plus the Astro and Next.js trees from their lockfiles

The version line for Astro and Next.js picked up an "extraneous" package instead of the framework: a bug in the recording command, to be fixed.

Smoke test inside the image, as user `bench` (uid 1000): `molde check` on the molde port → `verdict: pass`, 5 pages.

**Authentication and the result event.** A first authentication attempt returned a 401 (the credential was not accepted) and the credential was replaced. A by-product of that attempt: the `result` event of `stream-json` carries `num_turns`, `usage` (input, output, cache-creation and cache-read tokens, thinking tokens), `total_cost_usd`, `duration_ms`, `duration_api_ms`, `terminal_reason` and `is_error`. That's everything the design needs per run, without a proxy. An authentication failure reports `is_error: true` with `terminal_reason: api_error`, so the runner can tell auth failures from task failures.

**Correction:** the container runtime is Docker Desktop (native arm64 Linux VM), not colima. `docker info` reports "Docker Desktop", 12 CPUs. Review 1's colima x86-emulation concern therefore doesn't apply, but its VM caveat for timings does.

**Review 1 received** (`REVIEW-1.md`, written by an independent agent with no part in the design): 6 blocking, 8 important, 11 minor findings. Its verdict: "a good skeleton, but as written the headline result would be dismissed". Response to each blocking finding:

| Finding | Response |
|---|---|
| **B1:** the molde arm bundles generator + checker + gate + MCP + docs, and the oracle's invariants are molde's own check list, so "molde wins" would only show that a checker helps | **Accepted.** Add ablation arms: molde-build-only, and each incumbent + one standalone checker emitting the same report format. Give every arm the same written requirements. Judge a fresh `molde build`, never `out/current` (a failed check leaves the last good build there, which would score a stale pass). *Scope decision.* |
| **B2:** the agent can see the oracle, the golden files and the other ports; the network is open to Bash; the token is in the environment and `printenv` would put it in a published transcript | **Accepted in full.** The agent's container holds only its workspace, the toolchain and the docs; the judge runs in a separate container afterwards, mounting the workspace read-only. Outbound traffic allowed only to Anthropic. Grep every transcript for `sk-ant-` before anything is published; revoke the token after the campaign. |
| **B3:** harness flags wrong against real Claude Code behaviour | **Accepted.** No `--bare` (refuses OAuth); a fresh config folder per run (the container is `--rm`, plus the auto-memory switch off); requirements passed with `--append-system-prompt-file`, since Claude Code reads CLAUDE.md, not AGENTS.md; `--tools` to restrict tools (`--allowedTools` only pre-approves); non-root user (already true); pin the full model ID and reject any run whose `modelUsage` shows another model; pin `--effort`; auto-updater off. |
| **B4:** the incumbent ports were bent to match a golden produced by molde (Next's template strings, hand-written sitemaps, Zola's generator), so editing them is artificially hard | **Accepted.** T0 (build from empty, idiomatic) becomes a co-equal headline. Add an idiomatic-port panel (Hugo, Astro, Next.js using their built-in sitemap and metadata features), written by agents with no molde context and published for experts to improve. Put all data inside each workspace. *Scope decision.* |
| **B5:** no real pre-registration; the pilot runs the headline tasks before predictions exist; H1 undefined; success-only conditioning; H2 underpowered | **Accepted.** Predictions, oracle and classifier frozen, hashed and published *before* the pilot, and pilot runs excluded from the analysis. Primary measure: new tokens *per attempt* (failed runs included), molde-CLI vs each incumbent, task-level bootstrap with Holm correction. T3/T4 raised to 15 repetitions. Task-selection rule fixed in advance (tasks proposed by at least 2 of 3 no-context agents). |
| **B6:** only defects molde detects are planted; molde's documented blind spots (relative links, single-quoted attributes, CSS `url()`) are untested; "silent failure" means different things per arm | **Accepted; the best credibility move.** Add T4b with molde-blind defects and pre-register that molde may do worse there. Report silent failures split into "a failing signal was shown and ignored" and "no signal existed". |

The important and minor findings are all accepted. The main ones:

- headline tokens are "new tokens" (input + cache creation + output), with cache reads reported apart;
- no small per-run spend cap censoring: caps far above typical, with cap hits as their own outcome;
- infrastructure failures (auth, rate limits) are re-run, never counted against an arm;
- `claimed_done` defined structurally, and validated on a blind hand-labelled sample;
- an independent second checker (lychee + an HTML validator) beside the oracle;
- a "mutate the data, rebuild, pages must follow" test against hard-coded answers;
- two-sided controls with an equivalence margin;
- run order randomised with a fixed seed;
- the build benchmark is not rerun inside the VM as a replacement for the native macOS numbers.

**Consequence for the order of work.** Pre-registration has to be public before the campaign. So: freeze the design, predictions, oracle and classifier; publish them; run the campaign; report. (In the event the pilot ran before any public freeze; see the 16 Oct entries. It is excluded from the analysis.)

**Design v2** written (`DESIGN.md`), with every review-1 finding traced (B/I/M labels). v1 kept as `DESIGN-v1.md`. Three scope decisions were taken:

- ablation arms (`molde-build`, and Hugo/Astro/Next.js with a standalone checker);
- idiomatic ports written by agents with no molde context;
- pre-registration published before the campaign, the campaign after it, then the report.

**Images split into three** (`docker/Dockerfile` targets `agent` and `judge`, plus `docker/egress/`), per review 1 B2:

- `molde-bench-agent`: toolchains and pre-installed npm dependency sets only. No oracle, golden, tasks or other ports. molde sits off PATH at `/opt/molde-real`, and each arm puts on PATH only what it may use. Checked: in the bare image, `molde` and `lychee` are not on PATH.
- `molde-bench-judge`: the agent image plus molde on PATH and lychee 0.24.2. The oracle and the answers are mounted read-only at run time, never baked in.
- `molde-bench-egress`: tinyproxy with an allow-list holding only `^api\.anthropic\.com$`.
- **npm dependency sets** installed in the image: the three parity ports' lockfiles, plus two **idiomatic** sets created today (`ports-idiomatic/`):
  - Astro 7.3.8 + `@astrojs/sitemap` 3.7.4;
  - Next.js 16.4.0 + React 19.3.0 + TypeScript 7.0.2 + `@types/react` 19.3.0 + `@types/node` 22.20.5.

  Lockfiles were generated with `npm install --package-lock-only` in `node:22.15.0-bookworm-slim`.
- Build fixes along the way:
  - lychee's archive keeps the binary in a subfolder (`--strip-components=1`);
  - npm dependencies were first symlinked read-only into workspaces, which broke two builds: Astro's Vite writes a cache inside `node_modules` (EACCES), and Next.js's Turbopack refuses a `node_modules` symlink that leaves the project. Each workspace now gets its own writable copy; the source sets stay owned by root.

**Egress test** (agent container on the internal network `molde-bench-int`, proxy as the only exit):

| Attempt | Result |
|---|---|
| Direct, no proxy | no route (`000`) |
| Via proxy, `api.anthropic.com` | reached (`401`, no credentials sent) |
| Via proxy, `registry.npmjs.org` | refused, logged "Proxying refused on filtered domain" |
| Via proxy, `github.com` | refused |
| `npm view react` | fails |

**Requirements text** (`requirements.md`): 9 rules, identical for every arm, the definition of done (review 1 B1). Rule 2 explicitly covers relative links, single-quoted attributes and CSS `url()`, which `molde check` does not see. So the molde arms are told rules their checker can't fully verify, the same as everyone else.

**Oracle** (`oracle/oracle.py`): Python standard library only, no molde code. `html.parser` decodes attributes in any quoting style and character references, so it grades what a browser shows.

**Oracle self-test 1** (`oracle/selftest.py`, against the golden site): **34/34 expectations hold.** The unchanged site passes T0 and T10, and fails every task that asks for a change. Each of 22 planted defects fails the rule that names it, including all six molde-blind classes:

- relative link;
- single-quoted href;
- unquoted href;
- CSS `url()`, absolute and relative;
- inline `style` `url()`.

**Oracle validation 2** (`judge/validate_ports.sh`): all six correct ports built by their own frameworks inside the judge container, then graded on T0, the data-mutation test and lychee.

- First run: two harness bugs.
  - The workspace copy skipped every folder named `public`, but that folder is source for Astro (its stylesheet and robots.txt), so the oracle correctly failed the Astro build.
  - A lychee flag was wrong.

  Both fixed.
- Second run: **6/6 pass**: molde, parity-hugo, parity-zola, parity-eleventy, parity-astro, parity-nextjs. Each passed the oracle and the mutation test, with lychee at 0 errors out of 18 links.
- The Next.js export (with its `404.html`, `_next/` folder and `data-next-head` markup) passes, so the oracle tolerates framework markup and isn't molde-shaped.

**site-check written blind** (`checker/site-check`, 311 lines, stdlib only) by a separate agent that was told never to open `oracle/`. Inputs: `requirements.md`, the report section of the molde reference, the golden site. Its own tests (`checker/test_site_check.py`): **24/24 pass**; the golden site passes.

Its stated ambiguities match choices the oracle made independently:

- a link to a directory without a trailing slash is accepted (static hosts redirect);
- escape debris in decoded text counts as double escaping;
- noindex pages are not excused from the sitemap.

**Task prompts written** (`tasks/T0.md` … `T10.md`, `p2-suffix.md`). Every URL rule the oracle grades is stated in the prompt: T2's slug rule, T3's new prefix, T5's exact descriptions, T6's `/page/N/` scheme.

**Oracle extended** for content the T4b plants touch:

- the home page shows the data's `cycle_id`;
- the "Estimated" badge appears exactly when an upcoming dividend is unconfirmed;
- the "No upcoming dividend declared." text appears when there is none.

The mutation test now also changes `cycle_id`. Self-test: **38/38**. Ports: still **6/6**.

**Plants for the molde port** (`runner/setup.py`), with two corrections before use:

- `| escape | escape` doesn't double-escape, because the second escape sees safe markup; `replace("&", "&amp;")` before autoescape does.
- The hand-written head keeps the duplicated title, so two plants don't cancel out.
- In molde a page can only be left out of the sitemap by marking its route `index: false`; that is the T4 sitemap plant for molde, documented as such.

`runner/verify_plants.py`: every planted rule must fail, and nothing else except knock-on task assertions. **molde T4: OK. molde T4b: OK.**

**What `molde check` itself reports on the planted workspaces.** This is what the `molde-check` agent would be shown:

- **T4:** `links.internal`, `titles.duplicate` and `og.mismatch` are caught, plus `robots.missing` (because of `index: false`).
  - **Not reported:** the missing canonical in a hand-written head, the double escaping, and the pages missing from the sitemap.
  - So the design's description of T4 as "six defects molde detects" is false for molde as it is today. The task stays as it is, and the per-defect detection table will show it.
- **T4b:** `molde check` reports **no errors (verdict pass)** on a site with a broken relative link, a broken `srcset`, a missing CSS `url()` target, a badge on the wrong pages and a stale heading. That's the false-pass case review 1 B6 asked to be tested.
- `site-check` on the same workspaces: T4: all six categories caught. T4b: the broken links caught, the content errors not (no checker sees content).

**Finding about molde (reported to the author).** `docs/reference.md`, "What the checks do not see", says single-quoted and unquoted attributes are not checked by `links.internal`. Tested today inside the judge image:

| Form | `molde check` |
|---|---|
| `<a href='/x/'>` | **seen** |
| `<link href='/x'>` | **seen** |
| `<a href=/x/>` | **seen** |
| `<img src='/x'>` | **seen** |
| `srcset` | not seen |
| relative `href` | not seen |
| inline `style` `url()` | not seen |

So the docs understate the link check (stale text). Separately, a hand-written head with no canonical and a double-escaped name pass `molde check`. Both are logged here and not changed: molde isn't modified on this branch.

The T4b plant "single-quoted broken href" was replaced by "broken `srcset`", which molde genuinely does not see.

**The runner, end to end** (`runner/run_one.py` + `prep.py` + `metrics.py`, `judge/judge.py`). Each run uses three containers:

- **prep:** judge image, no network, bench read-only. It builds a fresh workspace volume and a context folder.
- **agent:** agent image, internal network, egress proxy only, no bench material. The credential is passed to the agent container on a file descriptor (see 16 Oct).
- **judge:** no network, workspace read-only. It rebuilds, grades, archives the workspace with a git diff that includes new files, then metrics are extracted.

**Dry runs** (`--dry-run`: no agent; the untouched workspace is judged) for `molde-check` × T0, T3, T4 and T4b: all four pipelines complete, and all four untouched workspaces correctly fail.

**Task validation, positive half** (`runner/verify_solutions.py`, reference solutions in `solutions/molde.py`, judge-side only): a known-correct molde answer to each task must pass the whole judge.

- First run: 12/12 accepted.
- **But the mutation test was too weak for T2:** it added a stock in an *existing* sector, so a hand-maintained sectors list couldn't fail it. Changed: the mutation now adds a stock in a **new** sector ("Utilities"). Requirement 8 says pages must follow the data, and a new sector needs a new page.
- **Second run: molde T2 fails,** as it must. molde's `foreach` runs only over top-level arrays, so sector pages can't be derived from the stocks; the best molde can do is a hand-maintained `sectors` array, which the new mutation breaks.
  - **T2 is pre-registered as expected to fail for the molde arms** (`EXPECTED_FAIL` in `solutions/molde.py`).
  - The six parity ports still pass the stricter mutation (6/6).
  - **Finding about molde (reported to the author):** no way to generate pages grouped by a field (sector pages, tag pages). Hugo (taxonomies/content adapters), Astro and Next.js (`getStaticPaths`) can. This is a product gap of the same kind as Zola's missing data→pages step in the build benchmark.
- **T10:** the judge now compares against the site as handed to the agent (the workspace's first commit, cloned and built the same way). Negative control `T10-overedit` (a visible content change on a "nothing to do" task) is correctly rejected.
- **Final: 13/13 as expected.**

**Scope note.** The parity panel (zola, eleventy and the parity versions of hugo, astro and nextjs) needs per-port T4/T4b plants and T1–T6 reference solutions. The panel is the first thing cut if the campaign is too big (DESIGN.md §9), so that work is deferred until the main arms are complete. The main arms' idiomatic ports don't exist yet: they come from the port factory, which needs a working token.

**Review 2 received** (`REVIEW-2.md`: 5 blocking, 15 important, 13 minor). Fixed today:

| Finding | Fix |
|---|---|
| **B1** `manifest.json` (with the plant categories) was in the agent's context folder | It now goes to the run's results folder. `prep.py` refuses to finish if the context holds anything but `arm.md`, `prompt.txt`, `molde-reference.md`, `mcp.json` and `bin/`. |
| **B2** the judge graded against the agent's own `data.json` | The reference data is the data file in the workspace's first commit. Requirement 8 now says the data may be added to or moved but not changed. `build_site.py` checks that the agent's data still contains the reference (rows removed or values changed fail R8), finds the data file wherever the agent moved it, and builds the mutation from it. |
| **B3** `signal_seen` counted the example report in the docs, and "ever" instead of "last" | Only results of real checker calls count (matched by tool_use id); the last verdict decides. False-done now has four kinds: checker failed and was ignored, checker passed (blind), checker gave no report, no checker run. `runner/test_metrics.py`: 13 tests, including the docs-reading case. |
| **B4** `site-check` is stronger than `molde check`, so "molde vs framework + checker" was confounded | New arm **`molde+checker`** (molde build, no `molde check`, plus the same `site-check`), the like-for-like comparator for the thesis. Every checker arm's note states what its checker doesn't see (requirements 8 and 10). |
| **B5** T9 not judged; T8 too narrow; Hugo's `/page/1/` stub and `/sectors/` index would fail correct sites | T8 and T9 are judged in Chromium (Playwright 1.63.0, judge image only): T8 by computed color and weight, T9 by typing queries and reading visible links. Redirect stubs are not pages. An optional `/sectors/` index page is allowed in T2. Links with and without a trailing slash count the same everywhere. The T2 slug follows the prompt literally. |
| **I1, I2, I3** classification and tokens | Every error result except the turn limit is infrastructure (re-run); usage-limit text too. An out-of-memory kill counts as a failure of the arm. `claimed_done` is structural; the lexical version is a sensitivity field only. Tokens are also taken from the pinned model's `modelUsage` entry. |
| **I5** the judge ran agent build code with the oracle mounted | The judge is two containers: `build_site.py` (no benchmark material; only it and `ports.py` are mounted) and `grade.py` (oracle, lychee, browser; runs no workspace code). |
| **I6** molde in every agent image | Removed. `prep.py` copies it into the context folder for molde arms only; the `molde-build` shim wraps a hidden copy, and any direct use is flagged. |
| **I8** egress log not kept | The proxy log is saved per run, with refused hosts listed in `meta.json`. The agent container also gets `--cap-drop ALL`, `no-new-privileges` and `--cpus 4`. |
| **I9** content graded but not stated | New requirement 10 (content), identical for every arm. |
| **I12** mutation fragility | The mutation adds a stock at the front with a digit in its ticker (`A1`) and one at the end in a new sector. New rows carry only the original data's fields (an agent-added slug is not filled in for them, as upstream data wouldn't know it). |
| **I13** lychee didn't check absolute links | `--remap` of `https://demo.example/` to the build folder; agreement with the oracle's link verdict recorded per run. No HTML validator yet. |
| **I15** reproducibility record | `meta.json` now records the bench commit and dirty flag, SHA-256 of every judge, runner and task file and of `reference.md`, the Docker version and the image's tool versions. Command lines have host paths replaced by placeholders. |

**Still open from review 2:**

- I4 (the molde reference lists its own blind spots, which the `site-check` arms don't get the same way), partly addressed by the checker notes;
- I7 (token scrubbing from tool subprocesses: to verify on the smoke run);
- I10's port-factory data path;
- I11 (idiomatic-port solutions for T2, T3, T6, T7 and T9);
- I14's real-transcript fixtures;
- an HTML validator;
- several minor items.

**molde changed after this benchmark's findings.** Between the findings logged above and the end of 15 Oct, molde itself was changed, in this order:

1. `links.internal` reads relative links, `srcset`, and CSS `url()`/`@import`;
2. new check `html.double_escaped`;
3. "What the checks do not see" rewritten to match the code;
4. `group_by` routes (one page per distinct value of a field).

The benchmark measures molde as of its pinned version, so everything was re-verified against molde as of 15 Oct, with `group_by` routes:

- **Images** rebuilt from that molde. The agent image has no molde; the judge has Chromium.
- **Two of the earlier "findings" were plant artefacts, not molde bugs.**
  - The first T4 plant put the canonical defect on pages also marked `index: false`; noindex pages need no canonical, so the plants masked each other.
  - `index: false` leaving pages out of the sitemap is the documented behaviour.
- **T4 re-planted** so no plant masks another. The sitemap plant is now `index: false` on the home page only.
  - Verified: the oracle fails exactly the planted rules.
  - `molde check` now reports `links.internal`, `titles.duplicate`, `canonical.missing`, `og.mismatch` and `html.double_escaped`: every T4 category except the sitemap omission, which molde treats as intended.
- **T4b re-planted** from what molde as of 15 Oct still doesn't see:
  1. a second, single-quoted canonical;
  2. a single-quoted `og:title` contradicting the title;
  3. the badge on the wrong pages;
  4. a stale hand-written heading;
  5. a hand-written wrong amount.

  Verified: the oracle fails exactly those; **`molde check` reports no errors (pass)**; `site-check` catches the two head plants, not the content ones.
- **T2 is still an expected failure for molde, for a narrower reason.**
  - `group_by` puts the value into the path as is, and "Consumer Defensive" contains a space, which molde refuses in a path (`config.invalid`, tested). T2 asks for a slug and a path pattern can't transform a value.
  - Workaround tested: a slug field added by hand to every stock passes the base build, but fails the mutation test, because the new sector gets no page.
  - Recorded in `solutions/molde.py`.
- **Validation after all of the above:**

  | Check | Result |
  |---|---|
  | oracle self-test | 38/38 |
  | metrics tests | 13/13 |
  | molde plants | T4 and T4b both OK |
  | molde reference solutions | 13/13 as expected (T2 and the T10 over-edit control fail as expected) |
  | correct ports | 6/6 pass (oracle and mutation) |

## 16 Oct: harness review before the first real run

**molde re-pinned** on 16 Oct to the version that goes public. The benchmark was moved onto it and the images were rebuilt from it. **The pinned molde for the campaign is molde 0.1.0-rc.1, the public repository's first release; source tree hashes (git, survive any commit): `src` cbce30eb3127d577fed9f8cf1075963128a7e6ac, `Cargo.toml` a70ce314535d0d2583fe37e776c1ec084184f3e8, `Cargo.lock` dc34f5a27a5bdd134097469ecad75fec62be1d76, `build.rs` 1e388cda3c88dd8033a281572e14b70e525bbea5.**

- **T2 is no longer an expected failure.** molde added `{field|slug}` in paths and the `slug` filter (16 Oct). The molde reference solution uses `/sectors/{sector|slug}/index.html` and `{{ item.sector | slug }}`, and passes, mutation test included. `EXPECTED_FAIL` now holds only the T10 over-edit control; PREDICTIONS.md updated.
- Re-validated on the new images: oracle self-test 38/38; metrics tests 16/16; checker tests OK; molde plants T4 and T4b OK (molde check reports the T4 set and passes T4b, as before); molde reference solutions 13/13 as expected.

**Found by testing Claude Code 2.1.300 in the agent image, offline:**

- **Two built-in plugins are enabled by default** (`cc-plugin-agents-md`, which loads AGENTS.md files as project instructions, and `cc-plugin-plugin-authoring`). The metrics class any loaded plugin as infrastructure, so every run would have been discarded. Disabled for the bench user in the image; the init event now lists none.
- **`CLAUDE_CODE_SUBPROCESS_ENV_SCRUB` needs bubblewrap**, which can't run in the locked-down container. Instead the token now reaches Claude Code through `CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR`: `docker run -i` gets it on stdin, the container moves stdin to fd 3 and closes stdin. Verified with two Haiku probes: no `sk-ant-` string in any process environment, fd 3 is not inherited by the agent's commands, nothing in `docker inspect`. As a backstop, token-shaped strings are scrubbed from every saved file. This closes review 2 I7.
- The stream carries `rate_limit_event` with the subscription's five-hour and seven-day utilization and reset times. metrics.json keeps the last one (`rate_limit_last`), for the campaign runner to pace on.
- The egress proxy is enough: only `api.anthropic.com` was contacted; a test request to another host was refused.

**Fixed after an independent review of the harness (cost and wasted runs):**

| Finding | Fix |
|---|---|
| The rate-limit pattern (with a bare `429`) was searched in every final message: a normal summary mentioning "rate limit" became infrastructure and was re-run | Limit text counts only in an error result, or when the whole final message is a short limit notice (`LIMIT_NOTICE`); tested both ways |
| A timeout with no progress (stuck API or network) was censored, charged to the arm | Fewer than 3 assistant messages before the timeout is infrastructure; tested |
| DESIGN's budget cap had no flag, and its result would have been classed infrastructure (re-run forever) | `--max-budget-usd` added (off by default); `error_max_budget_usd` is censored like the turn cap; tested |
| Timed-out runs had no token count, biasing costs down | Tokens estimated from assistant messages (last usage per message id), flagged `tokens_estimated` |
| `timeout` sent TERM only; no host-side timeouts | `timeout -k 30`; the agent step has a host timeout of the run's timeout + 300 s; judge steps 1,800 s, archive 600 s; judge containers named and removed |
| Judge containers had no limits (Next.js builds and Chromium could take the whole 8 GB VM) | `--memory 4g --cpus 4 --pids-limit 1024` on both judge containers |
| A run that died on auth, API error or usage limit still paid minutes of judging | Judge skipped (`judge_skipped` in meta.json) |
| Diff size counted build output and caches for framework ports (`public/`, `dist/`, Zola's generated content), not for molde | The same exclusions for every port (its output folder, caches, generated files) are unstaged before the diff; the archive leaves them out |
| The agent image's version file named molde, readable by every arm and the port-factory agents | Moved to the judge image |
| The proxy's log grew without limit | `max-size=10m`, 3 files |
| `verify_plants.py` printed `?` for a passing report with no errors | Prints the verdict |

**Small examples before the real model:** dry runs (no agent) of molde-check T3, molde-mcp T4, molde-build T10, parity-hugo T3, parity-zola T1, parity-nextjs T3, parity-astro T8: prep, judge, archive and metrics all ran; untouched workspaces fail their tasks and T10 passes, as they should. Then one full run with Haiku at low effort (molde-check T1): passed in 13 s, 5 tool calls, the token in no file, nothing refused by the proxy.

**Still open:** the idiomatic ports don't exist yet (the factory arms make them), so the hugo, astro and nextjs arms can't run; the campaign runner (it must stop on a usage limit, keep a ledger keyed by arm, task, prompt and repetition, clean leftover containers and volumes, and pace on `rate_limit_last`); analyze.py; the blind-label tool; the Docker VM has 8 GB, so one run at a time.

**First real run (smoke), on the campaign settings** (`claude-opus-5-5`, effort high, molde-check, T3, harness committed and clean): **pass**, judged by the oracle and the mutation test. 7 assistant messages, 6 tool calls, 32 s; 13,503 new tokens (67,836 processed with cache reads); the subscription's five-hour usage window moved from 4% to 5%. Token in no file; the proxy refused nothing. The agent edited with `sed -i` inside the same command as `molde check`, which the phase count missed: shell edits now count (`bash_edit_calls`, and a shell call that edits without verifying is phase `edit`), tested. The transcript is kept as a test fixture (`runner/fixtures/smoke-molde-check-T3`, review 2 I14).

**Campaign runner** (`runner/campaign.py`): a seeded, interleaved plan written once; a ledger so a restart never repeats a finished run; pacing on the subscription's windows (sleep above 85% of the five-hour window, stop above 90% of the seven-day one); a wait for the reset after a usage-limit hit; a stop after 3 infrastructure failures in a row; removal of what killed runs leave behind.

**Cleanup bug in the harness.** The first version of that cleanup removed every container and volume named like a run's, running or not. Tested while the three factory runs were going, it killed all three within seconds (about 7,000 tokens spent); those runs were discarded and the factory runs repeated. It also showed that a kill from outside exits 137, which metrics read as out of memory, a failure charged to the arm. Fixed: the cleanup refuses to start while any benchmark container runs and removes only stopped containers and unused volumes (tested with a live dummy container: refused; after it stopped: cleaned). Out of memory now comes from Docker's `State.OOMKilled`; a 137 without it is infrastructure (tested).

## 16 Oct: idiomatic ports, plants, solutions; the freeze

**Factory runs** (Opus 5.5, high): Hugo, Astro and Next.js each passed the whole judge on the first run (oracle and mutation test): 21,396 / 16,841 / 21,567 new tokens, 82 / 47 / 103 s. Frozen into `ports-idiomatic/<fw>/site/` with their transcripts (`factory-transcript.jsonl`) and verdicts. Hugo builds pages from data with a content adapter; Astro writes its own `sitemap.xml` endpoint (the integration only emits a sitemap index); Next.js uses the App Router, the Metadata API, `sitemap.ts` and `robots.ts`. Dry runs: T10 passes untouched on all three, T3 fails untouched.

**Plants** (`runner/setup.py`) for each idiomatic port, the same categories as molde's, written in each framework's own code. verify_plants: the oracle fails exactly the planted categories on T4 and T4b for all three; site-check reports the T4 set and the two T4b head plants, as for molde. Next.js can't emit single-quoted attributes; its T4b head plants are the same defects double-quoted (React 19 hoists them into the head).

**Reference solutions** (`solutions/idiomatic_*.py`): T3 (Hugo: a permalink pattern; Astro and Next.js: the page file moved and the path helper changed), T4 and T4b (inverse plants), T10, and the over-edit control: 5/5 as expected on each port. T0–T2 and T5–T9 for the idiomatic ports are still to write before the campaign (review 2 I11); the pilot uses T3 and T4 only.

**Freeze 1 (local, 16 Oct).** DESIGN.md (§11 order updated: freeze before the pilot, published unchanged before the campaign; seeds `20261016` campaign, `20261017` pilot, `1016` bootstrap), PREDICTIONS.md (H2b re-verified on the pinned molde), requirements.md, the tasks, the oracle and the metrics are frozen. Freezes 1 and 2 were made locally; the public pre-registration is the published version of these files, published before the campaign. The pilot ran before any public freeze and is excluded from the analysis. SHA-256 of the frozen files at the freeze (content hashes, valid for any copy; for DESIGN.md, PREDICTIONS.md, ROUND-2.md and tasks/README.md they are the hashes of the local files before the one editorial publication pass described at the top of this log; the other files are unchanged):

```
5efbde79ac837297ef67b566dfae466bd65245a85bbfc8c96e187dcf435f6d1c  agent-cost/DESIGN.md
16292403f4d023c794c17141b5726bbeba928889ae026092b6a26f0f535e5c74  agent-cost/PREDICTIONS.md
4495c0962255252b90dda961bf77766c938b26629218da3c34aa79b46069be68  agent-cost/oracle/oracle.py
e65500367e3e8a6fd1a4e9e12be991dc368496aa894df1410ed32dae4733cc4c  agent-cost/oracle/selftest.py
274d31be340397ff2c498ee4ed9e5afe5c66dda93c42f9ffcf6f306f6fcd482b  agent-cost/requirements.md
c6d2ba1ae0dfe951a9b9b7dad39a753a173b6b55de2a2b577e485a8edd3c7ea1  agent-cost/runner/arms.py
175b58138e7681982d1c56d032e84a5c2fb642da7e4d74ec0e4167803092d6c6  agent-cost/runner/campaign.py
dc075a4db8223d43a4fa70c1767cdda97508900f8891d457bc148cf7dc2c9dd3  agent-cost/runner/metrics.py
9d87aab586d20ac835dd4b8bd8fa9a88c742d25fb1479d43a3e0a4db2290a04e  agent-cost/runner/setup.py
ea486a54bf8f83928ce3c7fbd45cfedd5fc537a56a3fb6a1b52715e1d369cf23  agent-cost/tasks/README.md
b75487f87605d202ef4aab8ac0ad1453b09937f9b30f80e1fe5cc2da2e3dadc1  agent-cost/tasks/T0.md
11e785d22be1c92d3f8d8e7ac8146d858d7590295c56fd447ae3b92a4b703895  agent-cost/tasks/T1.md
fb1df61162bc69ad8ecbbc42f1c14462d416f4f8fb0887b93df068cc727a45e9  agent-cost/tasks/T10.md
24ee5a30e905b3d2e15b81366f1fce82f9d36a2903d9e2f27e7e634bdec19ade  agent-cost/tasks/T2.md
da248299bb6e60d28b8cb65b9429a37b3e800ce5da27791c027e3335ba7a4ae3  agent-cost/tasks/T3.md
a6e923a06bd72c193ec8024de5adddb3709e75c3d432150644caa42fda5da07a  agent-cost/tasks/T4.md
a6e923a06bd72c193ec8024de5adddb3709e75c3d432150644caa42fda5da07a  agent-cost/tasks/T4b.md
d410afca38a7b5f3d2a0c985de491ee6493e5d8f92d4d8ddada1f686fff96cf1  agent-cost/tasks/T5.md
92aaa3d54dd8ddcf3fb23e0234438a5908af33d216e4e3ec03738a0ba36d8599  agent-cost/tasks/T6.md
400514319fdad6172cda159fec20f4434e873e5af4009b6ad0db1dc5ddd17d2c  agent-cost/tasks/T7.md
ad9da6c1f725cc95a45bfd251dbf5719be04d3d3100463478adc4f0ecfb7df99  agent-cost/tasks/T8.md
2152d7528aace38d0a1ff8e735b0b21c3523c57f534f4bd00a4ab70492400d25  agent-cost/tasks/T9.md
0db8f1a2bbdbcdc49c07d59bc6f01501b09b2d2ae0e73a2f5a2f3db1d01712b4  agent-cost/tasks/factory.md
2968e81759aaf50f88e73aedb8901a2ae41955d57123d8581dc00dfbeb5e2fca  agent-cost/tasks/p2-suffix.md
```

## 16 Oct: the pilot (36 runs, discarded from the analysis)

Plan seed `20261017`; images rebuilt from the committed harness; Opus 5.5, effort high; one run at a time through `runner/campaign.py`. All 36 completed: **36 pass, 0 fail, 0 censored, 0 infrastructure failures, 0 false-done.** 494,235 new tokens in total; 18 minutes of agent time, about 21 minutes in all. The subscription's five-hour usage window went from 9% to 15%; the seven-day window stayed at 11% (whole-percent reporting).

```
arm              task   n pass fail cens infra  tok med  tok p95 wall med turns med
hugo             T3     3    3    0    0     0     7922     8067       20         5
hugo             T4     3    3    0    0     0     9963    11004       28         6
hugo+checker     T3     3    3    0    0     0     7023     8052       27         6
hugo+checker     T4     3    3    0    0     0    12343    13091       35         6
molde-build      T3     3    3    0    0     0    11772    13412       24         5
molde-build      T4     3    3    0    0     0    23657    30487       30         9
molde-check      T3     3    3    0    0     0     9317    12312       25         7
molde-check      T4     3    3    0    0     0    35902    37623       35         7
nextjs           T3     3    3    0    0     0     8713    11162       34         6
nextjs           T4     3    3    0    0     0    14620    15116       51         7
nextjs+checker   T3     3    3    0    0     0     7664     8863       24         6
nextjs+checker   T4     3    3    0    0     0    13409    14911       38         7
```

**What it shows (calibration, not results):**

- **Usage is small.** About 0.17% of a five-hour window per run: the designed ~1,060-run campaign is about two five-hour windows and roughly 10–12 hours of wall time, well inside the seven-day window.
- **Ceiling.** Every arm passed every run, and no agent claimed done on a broken site. On these tasks and this model, H2 (false-done) has nothing to measure; the pass-rate secondary results are uninformative.
- **molde costs more tokens in the pilot**, mostly reading its documentation: every molde run reads the whole reference (one Read of about 65 KB). On T4, molde-check median 35,902 new tokens against 9,963 (hugo) to 14,620 (nextjs); on T3, 9,317 against 7,023–8,713. The non-molde agents read no docs: the model already knows those frameworks. H1 as frozen would very likely fail; the per-phase token split (with and without docs, DESIGN §7) needs token attribution per tool result, which analyze.py must do.
- One molde-build run tried `molde check`; the shim refused it, as designed. metrics flagged it as a problem; now only a forbidden call that was not refused is (`forbidden_tool_refused` counts the refused ones). Infrastructure change, allowed after the pilot.

**Not changed:** tasks, oracle, predictions. The ceiling and the docs cost are design questions; under the freeze, a change to tasks after the pilot is a dated amendment that the report must disclose.

**Amendment 1 (16 Oct, after the pilot; additive, to frozen `runner/metrics.py`).** DESIGN §2 and PREDICTIONS promise every headline with and without docs tokens; metrics had no per-run docs figure. Added `docs_tokens_est` (each tool result's share, by characters, of the next assistant message's new input, attributed to the docs when the result is a docs read) and `new_tokens_without_docs`. A docs read is recognised by its command or file name and, new, by the reference's opening sentence in the result: in the pilot, agents that `cat` the whole reference get Claude Code's "output saved to a file" notice and then Read that file, which the name rule missed. No existing field or classification changed. Tested on the real smoke transcript.

**`analyze.py`** implements PREDICTIONS.md: H1 (per-task median ratios, geometric mean over the core tasks, 10,000-resample bootstrap with seed 1016, Holm over the comparisons; with and without docs tokens; with and without T2; molde+checker like for like), H2 (Fisher one-sided, Clopper-Pearson), H2b, H3 (T8 band, T9, T10 over-editing), H4. Standard library only. Fisher's test and the Clopper-Pearson interval checked against known values.

Run on the pilot (calibration only; the pilot is excluded from the analysis): molde-check's pooled T3+T4 ratio is 1.62–2.06 against the four arms with docs tokens, and 0.58–0.74 without them, every interval below 1. The cost is learning molde from its reference, not making the change.

## 16 Oct: every task validated on every port

**Reference solutions** (the positive half of task validation, review 2 I11), written per port in its own idiom and each run through the whole judge (oracle and data-mutation test), then re-verified independently:

| Port | Tasks | Result |
|---|---|---|
| molde | T0–T10, T4b, over-edit control | 13/13 as expected |
| idiomatic Hugo, Astro, Next.js | T0–T10, T4b, over-edit control | 13/13 each |
| parity Hugo, Zola, Eleventy, Astro, Next.js | T1–T6, T4b, T10, over-edit control | 9/9 each |

Notes from writing them: Astro's `paginate()` and Hugo's taxonomies (with the port's `disablePathToLower`) don't produce the URL schemes T6 and T2 ask for, so those solutions remap params or use a content adapter; Next.js T8 uses a global stylesheet (a CSS module would rename the `badge` class the task names); Zola's T6 generates `content/page/`, now excluded from diffs like `content/stocks`.

**Amendment 2 (additive, to frozen `runner/setup.py`).** DESIGN §9's parity panel runs T1–T6, which includes T4 and T4b, but only molde and the idiomatic ports had plants. Added T4 and T4b plants for the five parity ports, the same categories; verify_plants OK on all five (the oracle fails exactly the planted categories). No existing plant changed.

## 16 Oct: the task-selection rule (DESIGN §5), run

Three independent no-context agents (Opus 5.5, high effort, no tools, the locked-down container), prompt in `tasks/proposals-prompt.md`, answers in `tasks/proposals/` (`agent-1.md` to `agent-3.md`, each agent's final five-line answer). Proposals made by at least two:

| Proposal | Agents | Already a task? |
|---|---|---|
| Add a field, show it on item pages and the index | 3 | yes, T1 |
| Client-side search box on the index | 3 | yes, T9 |
| Paginate the index with previous/next and `/page/N/` | 3 | yes, T6 |
| **Sort the index** (by name or another field; agents 1 and 3 client-side controls, agent 2 a build-time order) | 3 | **no** |
| **Filter the index by category or tag** (buttons that show only matching items) | 3 | **no** |

Proposed by one only (not added): SEO metadata with a sitemap; per-category listing pages (T2 covers it).

Two tasks follow, **wording proposed here and agreed below**: the rule says "added verbatim", but every proposal names fields this site doesn't have (`date_added`, `price`, `tags`), so the text must be fitted to `data.json`. Proposed: **T11** "Sort the stock list on the home page by name, A to Z, instead of the order in data.json." (build time, judged by the oracle); **T12** "Add buttons above the stock list on the home page, one per sector, that show only that sector's stocks when clicked." (browser-judged, like T9). Each needs its oracle check and a reference solution in all nine ports before the campaign.

**Clarifications to the frozen design, found while planning the campaign (not changes; recorded so the plan follows one reading):**

1. §9 says "T0–T2, T5–T10: 10 tasks × 5"; that is 9 tasks. The core is 8 arms × (9 × 5 + 3 × 15) = 720 runs, not 760 (plus 80 if T11 and T12 are added).
2. §5 and §9: "P2 … on T1–T6 for the four main arms" names no arms. Reading: the four arms without a checker plus molde-check: `molde-check`, `hugo`, `astro`, `nextjs`, on T1, T2, T3, T4, T5, T6 (six tasks; T4b not included).
3. §5's T4b row describes the first plant set; the plants in use are the 15 Oct re-plant (LOG.md, "T4b re-planted"), because molde as of 15 Oct began to check relative links, `srcset` and CSS `url()`.

## 16 Oct: decisions; T11 and T12; the campaign plan

**Agreed** (16 Oct): round 1 run as registered plus a separately registered round 2 (`ROUND-2.md`, draft); the wording of T11 and T12; the three clarifications above (core 720 + 80 runs; P2 on molde-check, hugo, astro, nextjs, tasks T1–T6; T4b as re-planted); the freeze published before the campaign; the blind labelling by two people.

**Amendment 3: T11 and T12 added**, as the task-selection rule requires (DESIGN §5).

- `tasks/T11.md`: "Sort the stock list on the home page by name, A to Z, instead of the order in data.json." The oracle checks that the home page links to the stock pages in case-insensitive name order. The mutation's renamed and added stocks ("Zeta <Z> & Co", "Alpha One Inc" against "AT&T Inc") make a case-sensitive sort fail, as one first attempt at the Zola solution showed.
- `tasks/T12.md`: "Add buttons above the stock list on the home page, one per sector, that show only that sector's stocks when clicked." Judged in Chromium (`judge/browser.js`): a button whose text is each sector's name, before the first stock link; clicking it leaves exactly that sector's stock links visible; no reload. Because the buttons come from the data, T12 is also judged in the browser on the mutated build (`BROWSER_MUTATED` in `judge/grade.py`; the mutation adds the sector "Utilities").
- Oracle self-test: 42/42 (new: T11 and T12 not done fail; a list in another order fails T11; the list sorted by name passes T11). The browser check was tested by hand both ways: the unchanged site fails, a minimal correct filter passes.
- Reference solutions for both tasks in all nine ports: molde, idiomatic 15/15 each; parity 11/11 each (the full sets, re-run independently).
- Dry runs of T11 and T12 through `run_one.py`: untouched workspaces fail them with the right reasons.

**The campaign plan** (`runner/campaign-plan.sh`, seed `20261016`): 1,100 runs. Core 800 (8 arms × (T0–T2, T5–T12 × 5 + T3, T4, T4b × 15)), P2 120, molde-mcp 30, parity panel 150. `campaign.py plan` now takes several groups (`--spec`) and refuses a run listed twice. Not written yet: the plan is drawn when the freeze is public.

**Blind labelling** (`runner/label.py`): stratified sample of final messages by arm, generator and framework names replaced by "[tool]", key kept apart; one person at a time in a terminal, resumable; Cohen's kappa between the two people and each one's agreement with the structural rule. Tested on the pilot (sampling, labelling, resume, scoring; kappa checked by hand).

**Freeze 2 (local, 16 Oct):** the registered design with amendments 1–3 and the clarifications, as the campaign will run it. It supersedes freeze 1 for the campaign; both are published, and the report lists every difference between them (this log). Freezes 1 and 2 were made locally; the public pre-registration is the published version of these files, published before the campaign. The pilot ran before any public freeze and is excluded from the analysis. SHA-256 of the files at the freeze (same caveat as freeze 1 for the four edited documents):

```
5efbde79ac837297ef67b566dfae466bd65245a85bbfc8c96e187dcf435f6d1c  agent-cost/DESIGN.md
16292403f4d023c794c17141b5726bbeba928889ae026092b6a26f0f535e5c74  agent-cost/PREDICTIONS.md
52e083e4a4b63632722a11f90c0149629430041527fa59cb172028f7af5490eb  agent-cost/ROUND-2.md
028dcd0307aa0734d352ddebccdced46d9a9e9694ac0738bb98a2c580cebf794  agent-cost/analyze.py
dec04696e5a70f1148b407da7f9ef6f5e7961d86cb1c794abe3823b872571207  agent-cost/judge/browser.js
fee97262e1781a1b7569a4e829a91cfb2236e7626d262337c79a744788fb9d78  agent-cost/judge/build_site.py
48d9c8e0b038ea31bd13fcb519270a4b930cc02ede6d69859ebfdb9eb18d5011  agent-cost/judge/grade.py
3bb16de1146ccb05f687e261a7d70c0ce645f6a2787dcef62eef1d87e5354dcb  agent-cost/oracle/oracle.py
d93107fa3a8b9ac3d608ba2c2488d4c9b26e60793cfd5a304a9cb36a47a09433  agent-cost/oracle/selftest.py
274d31be340397ff2c498ee4ed9e5afe5c66dda93c42f9ffcf6f306f6fcd482b  agent-cost/requirements.md
c6d2ba1ae0dfe951a9b9b7dad39a753a173b6b55de2a2b577e485a8edd3c7ea1  agent-cost/runner/arms.py
74a9e0e5ec36270f2a3427d81eb78b1d1cd8032232cc193890ba293395d1912c  agent-cost/runner/campaign-plan.sh
6d015c187217eada20c5dcba1575fe6e6421276659e40bb5d5b8b564fe87c468  agent-cost/runner/campaign.py
744fa250e2346349266d154358c0ea3733868eb66e77d84270923c1b0c543adf  agent-cost/runner/label.py
ab09aa773c56e69b1920ce41a35ed56f8893bb45a629b3cde9ee4c3855462039  agent-cost/runner/metrics.py
878f1e7a91c00cac4a5a3b979d63d016cac44a96089d6f9b66105c9a07fd29dc  agent-cost/runner/setup.py
a5c273577a82a1107013936b4330459351175b42a1940f5182d15a1e9afae476  agent-cost/tasks/README.md
b75487f87605d202ef4aab8ac0ad1453b09937f9b30f80e1fe5cc2da2e3dadc1  agent-cost/tasks/T0.md
11e785d22be1c92d3f8d8e7ac8146d858d7590295c56fd447ae3b92a4b703895  agent-cost/tasks/T1.md
fb1df61162bc69ad8ecbbc42f1c14462d416f4f8fb0887b93df068cc727a45e9  agent-cost/tasks/T10.md
d1d0b1382702ef8b2b85328f98d9f1bfdc3e48c520aa7478b6648328903cb151  agent-cost/tasks/T11.md
ef8491ffc4e86588fb32467872b9ba37fcf2ea4d45f4107c1040ec9cfd7188c1  agent-cost/tasks/T12.md
24ee5a30e905b3d2e15b81366f1fce82f9d36a2903d9e2f27e7e634bdec19ade  agent-cost/tasks/T2.md
da248299bb6e60d28b8cb65b9429a37b3e800ce5da27791c027e3335ba7a4ae3  agent-cost/tasks/T3.md
a6e923a06bd72c193ec8024de5adddb3709e75c3d432150644caa42fda5da07a  agent-cost/tasks/T4.md
a6e923a06bd72c193ec8024de5adddb3709e75c3d432150644caa42fda5da07a  agent-cost/tasks/T4b.md
d410afca38a7b5f3d2a0c985de491ee6493e5d8f92d4d8ddada1f686fff96cf1  agent-cost/tasks/T5.md
92aaa3d54dd8ddcf3fb23e0234438a5908af33d216e4e3ec03738a0ba36d8599  agent-cost/tasks/T6.md
400514319fdad6172cda159fec20f4434e873e5af4009b6ad0db1dc5ddd17d2c  agent-cost/tasks/T7.md
ad9da6c1f725cc95a45bfd251dbf5719be04d3d3100463478adc4f0ecfb7df99  agent-cost/tasks/T8.md
2152d7528aace38d0a1ff8e735b0b21c3523c57f534f4bd00a4ab70492400d25  agent-cost/tasks/T9.md
0db8f1a2bbdbcdc49c07d59bc6f01501b09b2d2ae0e73a2f5a2f3db1d01712b4  agent-cost/tasks/factory.md
2968e81759aaf50f88e73aedb8901a2ae41955d57123d8581dc00dfbeb5e2fca  agent-cost/tasks/p2-suffix.md
e3bbb2df2e429df71a2d3b4a6ebe04f5af098df529030de8e6db5a5197ac1ab4  agent-cost/tasks/proposals-prompt.md
```

**Freeze 3, the publication freeze (16 Oct).** One editorial pass for publication (private context removed; no method, task, oracle, analysis or seed changed; details at the top of this log). These are the files as published; the hashes of freezes 1 and 2 above are of the local pre-publication text, and differ only for the files the editorial pass touched (DESIGN.md, PREDICTIONS.md, ROUND-2.md, tasks/README.md). The campaign runs on exactly these files. SHA-256:

```
7f620ed7d95cef2e7369ee7d255c3f0c32f885fec096fbdd10870d7883b6e679  agent-cost/DESIGN.md
4724085a2048f0a6d33a1231409940afa320d7dd415da445ccc3e9b9e4a0132e  agent-cost/PREDICTIONS.md
e59cf2dfef83eb83c9634a8f23c6e61c05f8a2108b12aadab17da892d4ddea49  agent-cost/ROUND-2.md
028dcd0307aa0734d352ddebccdced46d9a9e9694ac0738bb98a2c580cebf794  agent-cost/analyze.py
dec04696e5a70f1148b407da7f9ef6f5e7961d86cb1c794abe3823b872571207  agent-cost/judge/browser.js
fee97262e1781a1b7569a4e829a91cfb2236e7626d262337c79a744788fb9d78  agent-cost/judge/build_site.py
48d9c8e0b038ea31bd13fcb519270a4b930cc02ede6d69859ebfdb9eb18d5011  agent-cost/judge/grade.py
3bb16de1146ccb05f687e261a7d70c0ce645f6a2787dcef62eef1d87e5354dcb  agent-cost/oracle/oracle.py
d93107fa3a8b9ac3d608ba2c2488d4c9b26e60793cfd5a304a9cb36a47a09433  agent-cost/oracle/selftest.py
274d31be340397ff2c498ee4ed9e5afe5c66dda93c42f9ffcf6f306f6fcd482b  agent-cost/requirements.md
c6d2ba1ae0dfe951a9b9b7dad39a753a173b6b55de2a2b577e485a8edd3c7ea1  agent-cost/runner/arms.py
74a9e0e5ec36270f2a3427d81eb78b1d1cd8032232cc193890ba293395d1912c  agent-cost/runner/campaign-plan.sh
6d015c187217eada20c5dcba1575fe6e6421276659e40bb5d5b8b564fe87c468  agent-cost/runner/campaign.py
744fa250e2346349266d154358c0ea3733868eb66e77d84270923c1b0c543adf  agent-cost/runner/label.py
ab09aa773c56e69b1920ce41a35ed56f8893bb45a629b3cde9ee4c3855462039  agent-cost/runner/metrics.py
878f1e7a91c00cac4a5a3b979d63d016cac44a96089d6f9b66105c9a07fd29dc  agent-cost/runner/setup.py
45a483ce5dae99a595d3209175b0d0e02164a61507b2486cc524880acaf4748b  agent-cost/tasks/README.md
b75487f87605d202ef4aab8ac0ad1453b09937f9b30f80e1fe5cc2da2e3dadc1  agent-cost/tasks/T0.md
11e785d22be1c92d3f8d8e7ac8146d858d7590295c56fd447ae3b92a4b703895  agent-cost/tasks/T1.md
fb1df61162bc69ad8ecbbc42f1c14462d416f4f8fb0887b93df068cc727a45e9  agent-cost/tasks/T10.md
d1d0b1382702ef8b2b85328f98d9f1bfdc3e48c520aa7478b6648328903cb151  agent-cost/tasks/T11.md
ef8491ffc4e86588fb32467872b9ba37fcf2ea4d45f4107c1040ec9cfd7188c1  agent-cost/tasks/T12.md
24ee5a30e905b3d2e15b81366f1fce82f9d36a2903d9e2f27e7e634bdec19ade  agent-cost/tasks/T2.md
da248299bb6e60d28b8cb65b9429a37b3e800ce5da27791c027e3335ba7a4ae3  agent-cost/tasks/T3.md
a6e923a06bd72c193ec8024de5adddb3709e75c3d432150644caa42fda5da07a  agent-cost/tasks/T4.md
a6e923a06bd72c193ec8024de5adddb3709e75c3d432150644caa42fda5da07a  agent-cost/tasks/T4b.md
d410afca38a7b5f3d2a0c985de491ee6493e5d8f92d4d8ddada1f686fff96cf1  agent-cost/tasks/T5.md
92aaa3d54dd8ddcf3fb23e0234438a5908af33d216e4e3ec03738a0ba36d8599  agent-cost/tasks/T6.md
400514319fdad6172cda159fec20f4434e873e5af4009b6ad0db1dc5ddd17d2c  agent-cost/tasks/T7.md
ad9da6c1f725cc95a45bfd251dbf5719be04d3d3100463478adc4f0ecfb7df99  agent-cost/tasks/T8.md
2152d7528aace38d0a1ff8e735b0b21c3523c57f534f4bd00a4ab70492400d25  agent-cost/tasks/T9.md
0db8f1a2bbdbcdc49c07d59bc6f01501b09b2d2ae0e73a2f5a2f3db1d01712b4  agent-cost/tasks/factory.md
2968e81759aaf50f88e73aedb8901a2ae41955d57123d8581dc00dfbeb5e2fca  agent-cost/tasks/p2-suffix.md
e3bbb2df2e429df71a2d3b4a6ebe04f5af098df529030de8e6db5a5197ac1ab4  agent-cost/tasks/proposals-prompt.md
b742b296e94e9c9c3328f593246de0ce17266a793d0f22ec2e670cce2214e408  agent-cost/tasks/proposals/agent-1.md
acec08dc3a66ddee936a473d73e0c853529335d73001100da2cddd8a7d5ac836  agent-cost/tasks/proposals/agent-2.md
45efddd59b39c0727b8d608d133bd08c5e837fb3283167a91f91262f349351b5  agent-cost/tasks/proposals/agent-3.md
```

## 19 Oct: campaign done; two findings; amendment 4; a measurement fix

**Campaign** (set `campaign`, plan from `runner/campaign-plan.sh`, seed 20261016): 1,100 of 1,100 runs, 16–19 Oct, paused three times at the owner's request between runs (each time the one run that had just started was killed within seconds, moved to `results/discarded/`, and rerun on restart). Outcomes: 1,071 pass, 29 fail, 0 infrastructure failures. Every frozen file matched its freeze-3 hash during the campaign.

**Finding 1: T6 contradicts requirement 10.** T6 asks for 2 stocks per page; requirement 10 (added after review 2, I9) says the home page links to every stock's page. Agents that keep requirement 10 add an "all stocks" line and fail the oracle's T6 check ("/: lists /stocks/X/, which belongs on another page"); agents that drop it pass. All 22 T6 failures fail on that check alone. Neither the task nor the oracle is changed: T6 is reported as registered, beside a labelled post-hoc regrade that ignores that one check, and with the count of passing runs that dropped requirement 10. The defect was ours: requirement 10 was not checked against T6.

**Finding 2: T8's wording admits a second reading.** "Make the badge red and bold" was read by some agents as a red background with white text; the judge asks for red text. Reported as registered, with the count of failures that are this reading.

**Amendment 4: the `molde+checker` arm.** DESIGN §2 (after review 2, B4) defines `molde+checker` as the like-for-like comparator for the thesis, and PREDICTIONS.md tests H1(b) against it, but the size table in §9 was not updated and lists eight main arms without it; the campaign plan followed §9, so the thesis test had no runs. Found on 19 Oct while reading a preview of the analysis on the finished main arms (the preview's cost ratios for the other arms had been seen). Added as its own set, `campaign-mcheck` (seed 20261019), on the main arms' grid: T0–T2, T5–T12 × 5 and T3, T4, T4b × 15 = 100 runs, same model, image and judge. Two runs first as a check (both used `molde build` and `site-check`, no `molde check`). The report labels these runs as added after the campaign, and run later in time than the arms they are compared with.

**Measurement fix: docs tokens behind a saved output** (`runner/metrics.py`; freeze-3 hash `ab09aa77…` → `70328e53…`). When an agent `cat`s the whole reference, Claude Code saves the output to a tool-results file and returns a preview; the fingerprint caught the preview, but later `sed`/`grep` reads of sections of that saved file were counted as explore. Any call that names a saved file whose preview is the reference now counts as docs. New unit test (`test_sections_of_a_saved_docs_output_are_docs`; 20/20). All 1,102 runs' metrics were recomputed from their transcripts: only `docs_tokens_est`, `new_tokens_without_docs` and `tool_calls_by_phase` changed, in 23 runs (molde-build 14, molde-check 8, molde+checker 1; mean +7–10k docs tokens). No outcome changed. The error ran against molde: too few docs tokens subtracted made molde's "without docs" cost look higher.

**The Sonnet robustness row** (DESIGN §200, PREDICTIONS "secondary results"; not in the registered plan, so not run in the campaign). Planned on 19 Oct before any Sonnet run, as set `robust-sonnet` (seed 20261020): the main arms, `molde+checker` included, × T3 and T4 × 5 repetitions = 90 runs, `--model claude-sonnet-5-5`, everything else as in the campaign. The design gives no repetition count; 5 is the campaign's default for a task outside the safety core. Two runs first as a check (both passed, `models_used` = `claude-sonnet-5-5` only).

**More benchmark defects, found in the analysis** (reported in REPORT.md, not fixed in round 1): `next-env.d.ts`, generated by `next build`, is not excluded from the diff, so the Next.js arms' T10 over-editing is overstated; the Next.js export's `404.html` is read by agents as a page missing from the sitemap (two P2 failures added it; most Next.js T10 edits pruned it). Every T6 failure (23 of 23, amendment 4 included) is the requirement-10 conflict; every T8 failure (4 of 4) is the red-background reading.

**Secondary analysis** (`analysis/secondary.py`, written after seeing the data, labelled as such in REPORT.md): price-weighted cost with the model's published price ratios (input 1, cache write 1.25, cache read 0.05, output 5); output tokens, turns and wall time; the docs-once-per-session projection; molde-mcp; P2; variability; whole-reference reads; checking by hand; failures by cause.

**Files changed after freeze 3, on purpose.** `ROUND-2.md` is a draft, never a registration; it gained a section on what round 1's campaign adds (freeze-3 hash `e59cf2df…` → `99ab4a53…`). `runner/metrics.py` has the docs-counter fix above (`ab09aa77…` → `70328e53…`), with `runner/test_metrics.py`. Every other file in the freeze-3 list is unchanged.

**Blind labelling, by two models instead of two people** (DESIGN §7 registered two people; changed at the owner's request on 19 Oct). The labellers were two fresh agents, one `claude-opus-5-5` and one `claude-sonnet-5-5`, in separate contexts with no knowledge of the runs. Each read only the masked sample (`labels/sample.json`, tool names replaced by `[tool]`) and answered label.py's question for all 40 messages. Their answers are `labels/model-opus.json` and `labels/model-sonnet.json`.
- **Result:** both labelled all 40 "finished", in agreement with the structural `claimed_done` rule on 40 of 40. The lexical variant marked 3 as not finished, and both labellers disagreed with it on those 3.
- **The limit:** every one of the 1,290 runs (Opus and Sonnet) has `claimed_done` true. No agent ever said it was blocked or unfinished, so the sample cannot test the rule on a negative, and kappa is undefined.
- **Two more caveats:** the labellers are the same model family as the agents they judge, and file names in the messages can hint at the arm.
- **Fix:** `runner/label.py` printed kappa as 1.000 when every label is the same; it now prints "undefined".

**Independent audit of REPORT.md** (19 Oct; a fresh agent recomputed every number from metrics.json, verdict.json, diff-stat.txt and the transcripts with its own code, not the analysis scripts). Corrections applied:
- **H4 drops T2 in the frozen script.** PREDICTIONS.md defines H4 over the pooled core ratio (T2 included); `analyze.py` computes it on the core without T2, at 90%. With T2 (95%): molde-check vs molde-build 0.91 [0.84, 1.05], **does not hold**; nextjs+checker vs nextjs 0.90 [0.82, 0.96], holds; hugo 0.95, astro 1.13. The report gives both and lets the registered text decide.
- **The price weights were wrong.** Claude Code writes the cache with a one-hour lifetime, billed at 2× (each run's `total_cost_usd` matches input 1 / cache write 2 / cache read 0.05 / output 5 to within 3e-16), not the 1.25× first used. `analysis/secondary.py` now uses `total_cost_usd`.
  - molde-check comes out at 0.95–1.46× (was 0.88–1.37×); "cheaper than Next.js" no longer holds (0.95 [0.86, 1.05]).
  - On Sonnet: 3.2–4.2×.
- **"Docs before the first molde run, 120 of 120"** is 95 runs in an earlier call and 25 in the same command as the first molde run.
- **Other corrections:**
  - "no agent ever claimed done on a broken site" holds only for H2's tasks under P1;
  - the Next.js T10 edits mostly delete the export's extra not-found copies, not `404.html`;
  - molde costs more than all three incumbents on T3 and T5;
  - the whole-reference payback is mixed, not negative;
  - Sonnet reads the whole reference more often in only two cells of four;
  - T4 whole reads are 49 of 50;
  - the second link judge only ever saw zero broken links;
  - there were 18 forbidden commands, not 17;
  - minor wording.
- **Verified as correct:** every count, H1, H2, H3, the diff sizes, the failure causes, the projection and the other secondary numbers.

## 19 Oct: round 2A registered

`ROUND-2A.md` registers a small follow-up to round 1: does pointing the agent at molde's new short guide (`molde guide`, docs/agents.md) reduce molde's cost?
- **Arms:** `molde-check` and `molde-check-guide` (the same binary; only the note's docs line differs), plus `hugo`, `astro` and `nextjs` as concurrent controls.
- **Size:** 9 tasks × 5 repetitions = 225 runs, seed 20261020.
- **Predictions:** A1–A4.
- **Analysis:** `analysis/round2a.py`, dry-run on round 1's data before any round 2A run. With `molde+checker` standing in for the guide arm, it reproduces round 1's ratios.

**The molde under test** is the stack after rc.1: the guide; links written absolute to the site's own address checked; `canonical.duplicate`, `og.duplicate`, `og.title_mismatch`; head tags read in any quoting and from the markup only; the robots.txt checks. Both molde arms run it.
- **This changes T4b for molde:** plant 1 (a second single-quoted canonical) is now reported as `canonical.duplicate`, and plant 2 (a contradicting single-quoted og:title) as `og.duplicate`.
- **`runner/setup.py`'s comment** still describes the T4b plants as invisible to molde's check. That was true for round 1's pinned molde and is no longer true.
- **H2b is a round 1 hypothesis** about round 1's molde. Round 2A doesn't re-test it, and its T4b numbers are not comparable with round 1's on what the check reports.

**Measurement:** `runner/metrics.py` counts the output of `molde guide` as docs reading (`phase_of`), with a unit test (21/21). No round 1 run called `molde guide`, which didn't exist then.

Frozen (SHA-256, at the commit that adds this entry):

```
714aea78287290ee9fd48a6c5427f5a8ae8ce769213fcd6b489bfc2866b8dbef  agent-cost/ROUND-2A.md
4ceede8a28e217e0a237907a5ab029aa19e3d187425f39262fb5b10bec7c3872  agent-cost/analysis/round2a.py
3e958ea753f908947ca62481255ca2ce9574f5535927acb529bbe1b804bbba6c  agent-cost/runner/round2a-plan.sh
43e5320857b020564e0ecf98ed4f62c114409fdb43987638548f16a0548a5615  agent-cost/runner/arms.py
4b7946ac40e0d4f3d8d5b6b5b643c5614594cad6a06c0786b34c5f3c8e177199  agent-cost/runner/metrics.py
028dcd0307aa0734d352ddebccdced46d9a9e9694ac0738bb98a2c580cebf794  agent-cost/analyze.py
7e87569eb3656f5b4489f73c859ccea0457c7969f67bb16d3573760af9576960  agent-cost/analysis/secondary.py
3bb16de1146ccb05f687e261a7d70c0ce645f6a2787dcef62eef1d87e5354dcb  agent-cost/oracle/oracle.py
48d9c8e0b038ea31bd13fcb519270a4b930cc02ede6d69859ebfdb9eb18d5011  agent-cost/judge/grade.py
dec04696e5a70f1148b407da7f9ef6f5e7961d86cb1c794abe3823b872571207  agent-cost/judge/browser.js
274d31be340397ff2c498ee4ed9e5afe5c66dda93c42f9ffcf6f306f6fcd482b  agent-cost/requirements.md
b75487f87605d202ef4aab8ac0ad1453b09937f9b30f80e1fe5cc2da2e3dadc1  agent-cost/tasks/T0.md
11e785d22be1c92d3f8d8e7ac8146d858d7590295c56fd447ae3b92a4b703895  agent-cost/tasks/T1.md
fb1df61162bc69ad8ecbbc42f1c14462d416f4f8fb0887b93df068cc727a45e9  agent-cost/tasks/T10.md
d1d0b1382702ef8b2b85328f98d9f1bfdc3e48c520aa7478b6648328903cb151  agent-cost/tasks/T11.md
ef8491ffc4e86588fb32467872b9ba37fcf2ea4d45f4107c1040ec9cfd7188c1  agent-cost/tasks/T12.md
24ee5a30e905b3d2e15b81366f1fce82f9d36a2903d9e2f27e7e634bdec19ade  agent-cost/tasks/T2.md
da248299bb6e60d28b8cb65b9429a37b3e800ce5da27791c027e3335ba7a4ae3  agent-cost/tasks/T3.md
a6e923a06bd72c193ec8024de5adddb3709e75c3d432150644caa42fda5da07a  agent-cost/tasks/T4.md
a6e923a06bd72c193ec8024de5adddb3709e75c3d432150644caa42fda5da07a  agent-cost/tasks/T4b.md
d410afca38a7b5f3d2a0c985de491ee6493e5d8f92d4d8ddada1f686fff96cf1  agent-cost/tasks/T5.md
92aaa3d54dd8ddcf3fb23e0234438a5908af33d216e4e3ec03738a0ba36d8599  agent-cost/tasks/T6.md
400514319fdad6172cda159fec20f4434e873e5af4009b6ad0db1dc5ddd17d2c  agent-cost/tasks/T7.md
ad9da6c1f725cc95a45bfd251dbf5719be04d3d3100463478adc4f0ecfb7df99  agent-cost/tasks/T8.md
2152d7528aace38d0a1ff8e735b0b21c3523c57f534f4bd00a4ab70492400d25  agent-cost/tasks/T9.md
```
