# Roadmap

What comes next, and what molde deliberately does not do. Priorities move with what
people report; [open a discussion](https://github.com/ManuelFontRodriguez/molde/discussions)
if something here matters to you, or something missing does.

## 0.1.x: soon after the first release

- Fixes for whatever the first users find, released as patch versions.
- `group_by` on a localized field, and pagination within a group.
- A template time limit, so a template that loops forever stops with an error.
- A page in the reference on template patterns that agents get wrong most often.

## 0.2

- **A typed report.** The JSON report's errors as a documented schema, with a version
  field, so tools can validate it.
- **Two kinds of failure, two exit codes.** "The site is wrong" (a failed check) and
  "molde could not run" (a disk or permission problem) exit differently.
- **`molde init`**: a new site from a template, without cloning this repository.
- **Smaller builds for embedding**: `serve` and `mcp` as optional features.

## Later, depending on demand

- Native Windows builds (Windows users can use WSL today).
- Publishing to crates.io and `cargo binstall` metadata. Release binaries and
  `cargo install --git` cover installation today.
- A stable Rust library API. Today only the command line, the site format and the
  report are promised (see [compatibility](docs/reference.md#compatibility)).

## Not planned

These are choices, not gaps. Each one is something molde leaves to the step before it
or the tool after it, so molde itself stays small and its output stays predictable.

- **A Markdown or content pipeline.** molde renders a JSON snapshot. Convert Markdown,
  fetch APIs or read a CMS in the program that writes `data.json`.
- **Plugins or custom filters in other languages.** Every filter is part of molde and
  covered by its tests and determinism guarantee.
- **Incremental builds by default.** molde rebuilds everything; 1,901 pages take
  170 ms. If a site ever needs partial builds, molde will infer them, never ask you
  to maintain them.
- **Hosting, accounts or telemetry.** molde runs on your machine and sends nothing
  anywhere.
- **Checks of what a page says.** molde checks structure: links, titles, metadata,
  sitemaps, translations, sizes. Whether a sentence is true is for a person, or the
  agent, to judge.
