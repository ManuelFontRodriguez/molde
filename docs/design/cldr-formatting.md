# CLDR formatting: `date_format` and `num_format` for any locale

This document records the reasoning behind molde's locale-aware date and number formatting:
the goals, the API, what changes for the languages that were hand-written before, why the
data is a generated table rather than ICU4X, how determinism is guaranteed, the edge cases,
and the risks considered in review. For usage, see [docs/reference.md](../reference.md)
("Templates" and "Locales"). The locale setup this builds on is described in
[i18n.md](i18n.md).

The risks considered are listed at the end (R1–R14). Each one names the section that answers
it.

## 1. Goals

A site in 22 languages (`en es pt nl de fr it ru ja tr ko pl el sv no da fi he zh hi uk ar`)
needs every one of them to load as a site locale, and `date_format` and `num_format` to
format in all of them. Originally they could format only in 7 hand-written tables (de en es
fr it nl pt). The requirements:

- **Any well-formed locale** CLDR covers formats. There are no hand tables and no per-language
  code.
- **Deterministic.** The data is pinned in the repository. molde uses no host ICU and no
  system locale, so every machine and CI runner produces the same bytes.
- **ICU-compatible output.** Sites built with PHP (WordPress themes, for example) typically
  format with PHP intl (ICU4C). The reference for every string is what ICU4C prints (§4, §5).
- **`num_format(decimals=N)`**: exactly N fraction digits, grouped.
- **New date styles**: `full` and `month` (the `yMMMM` skeleton).
- **The honesty rule stays.** A value that isn't a real ISO date or a plain decimal string
  passes through unchanged.
- **Measured cost.** The binary size and build time of each data option are measured, not
  guessed (§4).

## 2. API

```
date_format(style="long")                   # style: full | long | medium | short | month
date_format(pattern="EEEE, dd MMMM yyyy")   # an LDML pattern instead of a style
date_format(..., locale="de")               # format in another locale for one call
num_format()                                # verbatim: the digits as given
num_format(decimals=2)                      # ICU DECIMAL: exactly 2 fraction digits, grouped
num_format(grouping=true|false)             # override the mode's grouping default
num_format(..., locale="de")
```

**`date_format`**
- It takes a string. `style` defaults to `long`.
- `style` and `pattern` are mutually exclusive. Passing both is a render error.
- An unknown style is a render error that lists the five styles.
- `pattern` supports the Gregorian date fields a template needs: `G`–`GGGG`, `y…`,
  `M`–`MMMMM`, `L`–`LLLLL`, `d`, `dd`, `E`–`EEEEE`, `ccc`–`ccccc`, plus quoted literals
  (`'de'`, `''`).
- Any other letter is a render error that names the field. Time fields fall under this
  rule: the input is a date and has no time.
- The pattern is checked on every call, including calls whose value would pass through, so
  a bad pattern fails on the first render and not only on pages that have a date.
- `month` is CLDR's `availableFormats/yMMMM`, the pattern ICU's
  `DateTimePatternGenerator::getBestPattern("yMMMM")` returns. Examples: en `March 2026`,
  es `marzo de 2026`, de `März 2026`, ja `2026年3月`.

**`num_format`**
- It takes a decimal string or a template number. A number is formatted from its display
  string, so `1234.5 | num_format(decimals=2)` gives en `1,234.50`. Any other type is a
  render error.
- `decimals` must be 0–100. A value outside that range is a render error.
- With `locales:`, both filters use the page's locale. With a single `locale:`, they use
  that locale. `locale=` overrides either one for one call.
- An unknown kwarg is a render error (minijinja `assert_all_used`).

## 3. Default output: what changes for es / en / nl / it / de / fr / pt

Before the CLDR data, molde formatted these seven languages with hand-written tables
(they are gone now). Each part of the output was checked against those tables, and this
section records what changed.

- **`num_format` without `decimals` is byte-identical.** The verbatim mode is the original
  behaviour: the digits exactly as given, no rounding, no grouping, no zero stripping. Only
  the decimal separator, the minus sign and the digits are localized.
  - `"0.0850"` gives es `0,0850`, as before.
  - Making grouping the default everywhere, as the original request sketched, would change
    `1234.5 → 1.234,5` on every existing page and would mix two meanings in one filter.
  - So grouping is on by default only where it is new: in `decimals=N` mode, which exists
    for ICU parity. `grouping=` overrides the default in both modes.
- **`long`, `short` and the de `medium` are byte-identical.** Every month name and
  every pattern of the 7 old tables was checked against CLDR 48. Months and patterns are equal
  everywhere except one case.
- **nl `medium` changes.** CLDR 48 drops the period from Dutch abbreviated months:
  `5 mrt. 2026` becomes `5 mrt 2026`, and likewise `jan feb apr jun jul aug sep okt nov dec`
  (`mei` never had one). ICU 78 prints the new form (tests/cldr/icu.json).
  - Sites whose PHP runs an older ICU still print `10 apr. 2025`, so this is a visible
    change on Dutch pages that use `style="medium"`.
  - The data is not patched. A per-language override would bring back the hand tables this
    change removes, and it would drift from ICU on the next ICU upgrade anyway.
  - A site that wants the dots should fix it in the site (use `long` there), not in
    molde's data.
- **Invalid calendar dates now pass through.** The old parser accepted `2026-02-30` and
  printed "30 de febrero". Now a day must exist (leap years included), or the value is
  returned unchanged: the honesty rule applied more strictly. Year `0000` passes through
  too.
- **Years below 1000 are no longer zero-padded.** They print as CLDR's `y` does: en
  `March 5, 999`, where the old code printed `0999`.

## 4. Data source: a generated table from cldr-json, not ICU4X

Both options were measured on the same workload: decimal formatting plus Gregorian dates in
full, long, medium and month style.

| | ICU4X `icu` 2.3, `compiled_data` (CLDR 48.2.1) | generated table (cldr-json 48.0.0) |
|---|---|---|
| release binary delta | +457,104 B (hello-world probe: 376,624 → 833,728 B) | **+454,272 B** (molde: 4,645,136 → 5,099,408 B, +9.8 %) |
| clean `cargo build --release` | +9.1 s (probe: 2.2 s → 11.3 s real) | **+0.2 s** (molde: 30.04 → 30.24 s real, within noise) |
| new dependencies | the `icu` crate tree | none (stdlib-Python generator, run by hand) |
| locales | all of CLDR | 529 (coverage basic or better) + 33 aliases |
| matches ICU4C (PHP) | no: da `full` lacks "den", and es groups `1234,50` (min-grouping 2) | yes, byte-for-byte on 1,509 of 1,521 golden rows (§8 lists the rest) |

Machine: Apple Silicon macOS, rustc 1.98, `lto = true`, `codegen-units = 1`. The ICU4X probe
was a throwaway scratch crate and is not part of the repository.

The binary sizes are close. The generated table wins on the other three rows:

- **ICU parity.** ICU4X formats dates from semantic skeletons, not CLDR's style patterns. It
  also applies `minimumGroupingDigits`, which ICU4C's `DECIMAL` formatter ignores. Both
  show up in the output of the 22-language set.
- **Build time.** There is no build-time crate graph to compile.
- **One pinned input.** The data comes from three npm tarballs checked by sha512.

The generated source is 350 KB of Rust statics: `src/i18n/cldr_data.rs`, 978 deduplicated
name arrays. `#[rustfmt::skip]` keeps it out of `cargo fmt`, and clippy passes on it.

The generator is `tools/cldr/generate.py`. It uses only the Python stdlib, and CI never runs
it. It reads `cldr-core`, `cldr-dates-full` and `cldr-numbers-full`, and keeps each locale's:

- Gregorian months and days (format and stand-alone; wide, abbreviated, narrow)
- CE era
- four date styles and `yMMMM`
- default numbering system's digits
- decimal, group and minus symbols
- grouping sizes from the decimal pattern

cldr-json's "full" packages are already resolved (inheritance applied), so a locale's entry
is complete on its own.

It fails loudly on anything molde can't honour:

- a pattern field outside the supported set
- a numbering system that isn't a plain digit set
- a `-nu` symbol override
- a decimal pattern that isn't `#,##0.###`-shaped

`und` is excluded. The Unicode License v3 ships with the data as `src/i18n/LICENSE-UNICODE`.

## 5. Determinism and the pinned version

- **Pinned data.** The data is `CLDR_VERSION = "48.0.0"`, and the three tarballs are pinned
  by npm sha512 integrity in the generator.
- **Nothing at run time.** The binary reads no environment variable, locale file or ICU
  library. Output depends only on (tag, value, kwargs, molde version).
- **Integer and string arithmetic only.** Rounding is done on the decimal digits
  themselves, never through `f64` (§9), so there is nothing platform-dependent left.
- **The reference.** `tests/cldr/icu.json` holds the goldens. They were printed by PHP
  8.2.33 intl, **ICU 78.1, ICU data 78.1 (CLDR 48)**, in a `wordpress:cli-php8.2` image with
  `icu-data-full` installed. The Alpine image otherwise ships English-only data.
  - `tools/cldr/goldens.php` regenerates the file and prints one case per line, so a bump
    reviews as a diff.
  - `tests/cldr.rs` asserts that the ICU version and `CLDR_VERSION` move together.
- **Bump procedure.**
  1. Set `VERSION` and the three integrities in `generate.py` (`npm view cldr-core@X dist.integrity`).
  2. Run `python3 tools/cldr/generate.py --cache <dir>`.
  3. Regenerate `icu.json` with an ICU whose CLDR equals X.
  4. Update the version assertion.
  5. Run `cargo test`. Read the icu.json and cldr_data.rs diffs: any change on a deployed site's
     locale is a visible change and belongs in the PR body.

## 6. Numbering systems and symbols

The rule is to match ICU's default for the locale: CLDR `defaultNumberingSystem`, with no
`-nu` keyword support. Dates use the same digits as numbers, as ICU does.

- **ar.** CLDR 48 makes `latn` the default for `ar`, with minus `U+200E -`: `‎-1,234.50`.
  `ar-EG` (and `ar-SA` and others) use `arab`: `؜-١٬٢٣٤٫٥٠`, and dates like `٥ مارس ٢٠٢٦`.
- **fa** uses `arabext` (`۱٬۲۳۴٫۵۰`). **bn** uses `beng`.
- **hi** uses `latn` with Indian grouping (`#,##,##0.###` → `12,34,567.00`), which is kept
  as primary 3, secondary 2.
- **Group separators** are what CLDR says, invisible characters included:
  - fr uses U+202F (narrow no-break space), not U+0020 or U+00A0.
  - ru, pl, uk, sv, no, fi and pt-PT use U+00A0.
  - de-CH uses `'` (ASCII apostrophe).
  - The tests print invisibles as `<U+XXXX>`, so a separator regression is readable.
- **Minus signs.** sv, no and fi use U+2212 (`−`). he uses `‎-` (with U+200E).
- **`minimumGroupingDigits` is not applied.** ICU4C's `DECIMAL` formatter ignores it:
  es `1.234,50` and pl `1 234,50` in ICU 78, which molde matches.

## 7. Locale resolution

A tag is looked up after canonical casing and separators: `pt_br` and `PT-BR` → `pt-BR`,
`zh-hant-tw` → `zh-Hant-TW`. The lookup tries, in order:

1. the tag itself;
2. an alias. There are 33:
   - script changes derived from `likelySubtags`: `zh-TW → zh-Hant`,
     `zh-HK → zh-Hant-HK`, `sr-ME → sr-Latn-ME`, …
   - deprecated two-letter codes: `iw → he`, `in → id`, `tl → fil`, `mo → ro`, …
3. the tag with its last subtag dropped (`es-ES → es`), and then steps 1–3 again.

Notes:
- **`no` and `nb`.** CLDR 48 ships `no` (Norwegian, the Bokmål data) and `nb` as separate
  locales with the same formats. `no`, `nb` and `nn` all resolve and all match ICU.
  WordPress's `nn_NO` is Nynorsk and resolves to `nn`, not `no`.
- **`zh`** is `zh-Hans` data (Simplified). `zh-TW` and `zh-HK` get Traditional through the
  alias table, as ICU does.
- **Region data is used when CLDR has it.** For example `en-GB` prints `5 March 2026`,
  `en-IN` groups `12,34,567.50`, `de-CH` groups with `'`, and `fr-CA` prints
  `2026-03-05` short.
- A tag nothing resolves to fails at render time (§11): `tlh`, `xx`, `und`, or `aa`
  (coverage below basic).

## 8. Dates and calendars

- **Input.** Only strict `YYYY-MM-DD` counts as a date: years 0001–9999, and the day must
  exist. Everything else passes through unchanged:
  - datetimes (`2026-03-05T10:00:00Z`)
  - `2026-3-5`
  - full-width digits
  - `2026-02-30`
  - `0000-01-01`
  - empty strings

  `year` is unchanged: it still extracts from ISO datetimes.
- **Proleptic Gregorian.** The weekday is computed with Sakamoto's method.
  - ICU4C's `GregorianCalendar` switches to Julian before 1582-10-15, so ICU prints
    `0001-01-01` as "January 3, 1". molde prints the date the data says.
  - ISO 8601 dates are proleptic Gregorian by definition, so the data's date is the one to
    print; a hybrid calendar would make one input string mean two days.
- **Only the Gregorian calendar.** fa defaults to `persian` and th to `buddhist`. molde
  formats every locale in Gregorian with CLDR's Gregorian patterns.
  - PHP's `IntlDateFormatter` (the golden source) takes the style pattern from the locale's
    default calendar and then applies the `GREGORIAN` constant. So for fa `full` and
    `month`, and th `long`, ICU prints a Persian or Buddhist pattern over Gregorian fields:
    - fa: `۲۰۲۶ مارس ۵, پنجشنبه` where molde prints `پنجشنبه ۵ مارس ۲۰۲۶`
    - th: `5 มีนาคม 2026` where molde prints `5 มีนาคม ค.ศ. 2026`
  - molde's output is CLDR's own Gregorian patterns for the locale. `tests/cldr.rs` lists these 12 rows as known divergences and fails if one of them ever
    starts matching.
  - Neither locale is one of the 22.
- **Stand-alone vs format.** `MMMM` is the format form, as in PHP's usual month-name
  formatting: ru `марта`. `LLLL` is the stand-alone form: ru `март`. Both match ICU for all
  39 golden locales.

## 9. Numbers: rounding and edge cases

- **Half-even.** Rounding is ICU's default mode, applied to the decimal string, so there is
  no binary-float surprise:
  - `0.125 → 0.12`, `0.135 → 0.14`, `2.675 → 2.68` (ICU agrees, because it rounds the
    shortest round-trip digits)
  - `999.995 → 1,000.00` (the carry is propagated and then grouped)
- **Negative zero keeps its sign**, as ICU does: `-0.001` with `decimals=2` → `-0.00`.
  Verbatim `-0.0850` → es `-0,0850`.
- **Leading zeros.** `decimals=N` strips leading integer zeros (`007.5` → `7.5` at `decimals=1`), as ICU
  does. Verbatim keeps them (`007,50`), as the original did.
- **Huge numbers stay exact.** ICU goes through `double`, so above about 15 significant
  digits it prints float noise. molde works on the string and prints every digit given.
  That is a deliberate divergence in molde's favour, and no golden row exceeds `f64`
  precision.
- **Not plausible → unchanged**, in both modes: `""`, `1.2.3`, `.5`, `5.`, `1,5`, `+1`, `1e5`,
  `-` and `NaN`. A template number goes through its display string, so a float that
  displays in exponent form passes through unchanged too.

## 10. Right-to-left: not a concern

- The filters return plain text. Directionality in that text is CLDR's own: the marks are
  inside the data (`U+200E`/`U+200F`/`U+061C` in ar and he minus signs and date patterns,
  e.g. ar short `5‏/3‏/2026`), and molde copies them byte-for-byte.
- Layout direction is the template's job: `dir="rtl"`, as [reference.md](../reference.md)
  says. molde never touches `<html>`.
- Nothing is reordered, and no bidi algorithm runs. ICU doesn't run one either.

## 11. Lazy errors

Loading a site doesn't consult the data: any well-formed tag is a site locale. A filter
looks up its locale per call:

- the page's locale, the single `locale:` value, or `locale=`;
- a tag with no data is a render error that aborts `check`, e.g. `date_format: locale 'tlh'
  has no formatting data — molde bundles CLDR 48.0.0 for 529 locales (coverage basic or
  better) (page /tlh/) (in home.html.j2:2)`;
- pages that don't format in that locale are unaffected.

A bad style, a bad pattern or bad kwargs are render errors too, with the same page and
template context.

## 12. Parity with typical PHP intl code

Themes that molde replaces usually wrap `NumberFormatter` and `IntlDateFormatter`. The
mapping of those usages to molde:

| PHP intl usage | molde |
|---|---|
| `NumberFormatter` DECIMAL, default / with N fraction digits | `num_format(decimals=2)` / `num_format(decimals=N)` |
| `IntlDateFormatter` LONG / MEDIUM / SHORT | `date_format` / `(style="medium")` / `(style="short")` |
| a full-date pattern (`'EEEE, dd MMMM yyyy'`) | `date_format(pattern="EEEE, dd MMMM yyyy")` |
| a month-name pattern (`'MMMM'`) | `date_format(pattern="MMMM")` |
| month and year | `date_format(style="month")` |

WordPress locale codes map onto molde tags as follows:

- `nn_NO` → `nn`, `pt_PT` → `pt-PT`, `zh_CN` → `zh`, `he_IL` → `he`
- underscores and region casing are canonicalized, so `locale="pt_PT"` works too

## 13. Out of scope and follow-ups

- **Times and datetimes**: the input contract is dates. That needs a time zone decision
  first.
- **Non-Gregorian calendars and `-u-nu-` / `-u-ca-` keywords.**
- **Percent, currency, compact and scientific notation; plural rules.** When plurals come,
  they use CLDR categories ([i18n.md](i18n.md)).
- **Site locale tags with scripts** (`zh-Hant`) remain a load error under `locales:`
  ([i18n.md](i18n.md)). `locale=` accepts them already.
- **pt-PT vs pt.** On WordPress, a Portuguese site often runs `pt_PT`, while molde's `pt` is
  Brazilian, as in CLDR. A site that wants European Portuguese formatting uses the tag
  `pt-PT`. Mapping a site's tag is a site decision, not molde's.

## 14. Risks considered

These attacks were written against the draft before the code was final. Each resolution
names where it lives.

| # | Attack | Severity | Resolution |
|---|---|---|---|
| R1 | **Determinism across platforms.** A formatter that asks the OS (ICU, `strftime`, `Intl`) gives different bytes on macOS, glibc and musl. | blocker | No runtime data (§5): static tables and integer and string arithmetic, so nothing depends on the host. The same goldens gate every CI run. |
| R2 | **CLDR drift.** A dependency bump silently changes a deployed site's dates (nl `mrt.` already has, §3). | major | Data pinned by version and sha512. Bumps are a manual generator run with the diff in review. The goldens and the version assertion move with the ICU they came from (§5). |
| R3 | **NBSP vs space in fr.** A hand table or a "normalizing" helper writes U+0020 or U+00A0 where CLDR has U+202F. Tests that print strings can't see it. | major | Separators are copied from CLDR. The generator writes invisibles as `\u{…}` escapes, and `tests/cldr.rs` spells them `<U+XXXX>` in failures (§6). fr is U+202F, and ICU agrees in the goldens. |
| R4 | **`no` vs `nb` vs `nn`.** WordPress's Norwegian locale code is `nn_NO`. `no` might have no data, or the wrong data. | major | `no`, `nb` and `nn` all resolve (§7), and all three are in the goldens. The tag decision is documented (`nn_NO` → `nn`, §12). |
| R5 | **The `zh` script.** `zh-TW` falling back to `zh` prints Simplified to Taiwan. | major | The alias table comes from `likelySubtags` (`zh-TW → zh-Hant`, `zh-HK → zh-Hant-HK`), checked in the goldens against ICU (§7). |
| R6 | **Date input edge cases.** `2026-02-30`, `0000-01-01`, datetimes, `2026-3-5`, full-width digits and pre-1582 dates. | major | Strict parser: an existing day, years 0001–9999, everything else unchanged (§8). The Julian switch diverges from ICU on purpose, and this is documented. |
| R7 | **Negative numbers.** ar's `U+200E` minus, sv's `U+2212`, and `-0.001` rounding to "0.00" with its sign lost. | major | Per-locale minus from CLDR. The sign is kept on negative zero, as ICU does (§9). Negative rows are in the goldens for all 39 locales. |
| R8 | **Huge numbers.** `f64` loses digits past 2^53 and prints `1e21`. | minor | String arithmetic (§9). Verbatim and rounded modes both keep every digit. |
| R9 | **Rounding mode.** Half-up (what a hand-written rounding would do) disagrees with ICU on `0.125`. Float rounding disagrees on `2.675`. | major | Half-even on the decimal string, and the goldens include `0.125`, `0.135`, `2.675`, `999.995` and `-0.001` (§9). |
| R10 | **Binary bloat.** "All of CLDR" in a static binary. | minor | Measured at +454 KB (+9.8 %) for 529 locales, the same as ICU4X for less (§4). Coverage below basic is excluded. The generator dedupes name arrays. |
| R11 | **Compile time.** A 350 KB generated file, or ICU4X's crate graph, slows every clean build. | minor | Measured at +0.2 s for the table vs +9 s for ICU4X (§4). The file is plain statics: no macros, no const-eval. |
| R12 | **Grouping by default breaks existing pages.** Turning grouping on by default changes `1234,5` on sites already built. | blocker | Verbatim mode keeps the original default. Grouping defaults on only with `decimals=` (§3). |
| R13 | **Non-Gregorian defaults.** fa and th: whose output is right, ICU-through-PHP's or CLDR's Gregorian? | minor | CLDR Gregorian. It is documented and the goldens pin the divergence (§8). |
| R14 | **License.** Shipping CLDR data in a binary without its license. | minor | Unicode License v3 is in `src/i18n/LICENSE-UNICODE`, next to the data (§4). |
