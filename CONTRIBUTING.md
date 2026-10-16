# Contributing

molde is maintained by one person for now. Issues are very welcome: bug reports,
a check that failed a correct site, a template pattern the reference does not explain,
or a capability you needed. The issue templates ask for what makes a report useful,
above all the smallest site that shows the problem.

For pull requests, open an issue first so we can agree on the change before you write
it. A pull request is licensed like the rest of molde (MIT or Apache-2.0, see the
README) unless you say otherwise.

## Working on molde

You need Rust 1.89 or newer ([rustup.rs](https://rustup.rs)). Before you push:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

`cargo test --test routes` runs one test file, and `cargo test some_name` the tests whose
name contains `some_name`. When a change is meant to alter the output, regenerate the
golden files with `UPDATE_GOLDEN=1 cargo test` and explain the difference in the pull
request.

CI runs the same three on Linux and macOS for every pull request, builds with the oldest
supported Rust (`rust-version` in `Cargo.toml`) and runs a licence and advisory check
(`cargo deny`).

Everyone taking part in molde's issues, discussions and pull requests follows the
[code of conduct](CODE_OF_CONDUCT.md). Questions go to
[Discussions](https://github.com/ManuelFontRodriguez/molde/discussions); see
[SUPPORT.md](SUPPORT.md).

Two rules shape most changes:

- **A capability ships with its checks.** A feature that can be wrong in the output
  comes with the conformance check that catches it, documented in `docs/reference.md`.
- **Output is deterministic.** The same site and the same molde produce the same bytes;
  the golden tests in `tests/golden/` hold that line.

Commit messages, pull request titles and descriptions follow
[docs/git-conventions.md](docs/git-conventions.md).
