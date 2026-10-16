# CLDR data

`src/i18n/cldr_data.rs` is generated from the Unicode CLDR JSON packages, and
`tests/cldr/icu.json` holds the reference strings molde must reproduce. Both are
pinned to one CLDR version (`CLDR_VERSION`, now 48.0.0); the test suite asserts that
the reference ICU version and the data version move together.

To move to a new CLDR version X:

1. In `generate.py`, set `VERSION` and the three package integrities
   (`npm view cldr-core@X dist.integrity`, and the same for the other two packages).
2. Run `python3 tools/cldr/generate.py --cache <dir>`.
3. Regenerate `tests/cldr/icu.json` with an ICU whose CLDR is X (`goldens.php` prints
   the command; one case per line, so the change reviews as a diff).
4. Update the version assertion in `tests/cldr.rs`.
5. Run `cargo test` and read the `icu.json` and `cldr_data.rs` diffs: every change in a
   locale a site uses is a visible change and belongs in the changelog.
