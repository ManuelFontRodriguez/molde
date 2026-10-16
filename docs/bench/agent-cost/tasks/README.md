# Tasks

Each `T*.md` is the exact user prompt an agent receives (after the arm's note and the requirements, which arrive as an appended system prompt). Prompt variant P2 appends `p2-suffix.md`. Setup that changes the workspace before the agent starts (new data fields, planted defects) is in `../runner/setup.py`, with every edit listed per port.

Tasks are frozen before the pilot (DESIGN.md §11). The tasks proposed by independent no-context agents (DESIGN.md §5) arrived on 16 Oct (the three final answers are in `proposals/agent-1.md`, `agent-2.md` and `agent-3.md`; prompt in `proposals-prompt.md`). Two proposals were made by all three agents and were not yet tasks; they are `T11.md` (sort the list) and `T12.md` (filter by sector). Their wording is fitted to this site's data, because every proposal named fields `data.json` doesn't have; the wording was agreed on 16 Oct (LOG.md).
