# Rules for agents

Every agent working on molde, and every person, follows these rules. Claude Code reads
them through `CLAUDE.md`. Human contributors: see also [CONTRIBUTING.md](CONTRIBUTING.md).

## Before you push

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

Verify locally first: CI confirms, it is not where you find out. Commit messages, pull
request titles and descriptions follow [docs/git-conventions.md](docs/git-conventions.md).

## How CI runs

- **Every pull request and every push to `main`** runs the three commands above on Linux
  and macOS (macOS is the case-insensitive file system), builds with the oldest supported
  Rust (`rust-version` in `Cargo.toml`) and runs the `cargo deny` licence and advisory
  audit. A newer push to a pull request cancels the older run.
- **A failure is something to fix, not to re-run.** Re-run a job only after a GitHub-side
  problem (a runner that never started), and say so.
- **When you change a workflow,** run `actionlint` locally first.

## What a change must keep

- **Deterministic output.** The same site and the same molde give the same bytes. The
  golden tests in `tests/golden/` hold that line; never regenerate them to make a test
  pass without explaining the output change.
- **The check report is a contract.** Agents and scripts parse it. Its fields and check
  ids change only as the [compatibility section](docs/reference.md#compatibility) allows,
  and every change is listed in [CHANGELOG.md](CHANGELOG.md).
- **A capability ships with its checks.** If a feature can be wrong in the output, the
  check that catches it comes in the same pull request, documented in
  `docs/reference.md`.
- **Performance work never weakens a check.** A speed-up must produce identical reports
  and output, shown on real sites and by tests. Errors keep their order, and a duplicate
  names the same earlier page.
- **The security model holds.** The site folder is trusted; `data.json` and everything an
  agent passes to `molde mcp` are not. No MCP tool may read or write outside `--site` and
  `--out` (see the reference's security model).

## Working rules

- English in code, comments, file names and docs.
- One pull request per item; the maintainer merges.
- Ask first before anything that cannot be undone or is public: deleting branches,
  rewriting history, force-pushing, tagging or publishing a release.
- Never print a secret's value.
