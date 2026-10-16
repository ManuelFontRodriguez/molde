//! CLDR formatting against ICU4C: every string in tests/cldr/icu.json was printed by
//! PHP's intl (tools/cldr/goldens.php — the formatter WordPress themes call), and
//! molde must print the same bytes from its generated CLDR tables. 22 widely used
//! locales plus resolution and numbering-system edges (pt-PT, zh-TW, sr-ME, ar-EG, fa, …).
use molde::testing::i18n::{self, NumOptions};
use serde_json::Value;

fn goldens() -> Value {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cldr/icu.json"),
    )
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn loc(tag: &str) -> &'static i18n::CldrLocale {
    i18n::resolve(tag).unwrap_or_else(|| panic!("no CLDR data for {tag}"))
}

/// Invisible characters spelled out, so a failure shows which space or mark differs.
fn show(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ' ' => " ".to_string(),
            c if c.is_whitespace() || ('\u{200B}'..='\u{200F}').contains(&c) || c == '\u{061C}' => {
                format!("<U+{:04X}>", c as u32)
            }
            c => c.to_string(),
        })
        .collect()
}

fn check(failures: &mut Vec<String>, what: String, got: String, want: &str) {
    if got != want {
        failures.push(format!("{what}: got {} want {}", show(&got), show(want)));
    }
}

fn rows<'a>(g: &'a Value, section: &str) -> &'a Vec<Value> {
    g[section].as_array().unwrap()
}

#[test]
fn reference_is_icu_with_the_pinned_cldr() {
    // ICU 78 ships CLDR 48; bump both together (tools/cldr/README.md)
    let g = goldens();
    assert_eq!(g["icu"], "78.1");
    assert_eq!(i18n::CLDR_VERSION, "48.0.0");
}

#[test]
fn numbers_match_icu() {
    let g = goldens();
    let mut failures = Vec::new();
    for r in rows(&g, "numbers") {
        let (tag, v, want) = (
            r[0].as_str().unwrap(),
            r[1].as_str().unwrap(),
            r[4].as_str().unwrap(),
        );
        let decimals = r[2].as_u64().map(|n| n as usize);
        let grouping = r[3].as_bool().unwrap();
        // verbatim rows (decimals null) run with the default grouping (off)
        let opts = NumOptions {
            decimals,
            grouping: decimals.map(|_| grouping),
        };
        let got = i18n::num_format(loc(tag), v, opts);
        check(
            &mut failures,
            format!("{tag} {v} decimals={decimals:?} grouping={grouping}"),
            got,
            want,
        );
    }
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Where ICU4C (through PHP) is not the reference: fa and th default to the persian and
/// buddhist calendars, and IntlDateFormatter takes the style pattern from the locale's
/// default calendar before applying GREGORIAN — so ICU prints a persian/buddhist pattern
/// over Gregorian fields. molde uses CLDR's gregorian patterns (cldr-formatting.md §8).
const KNOWN_DIVERGENCES: [(&str, &str); 3] = [("fa", "full"), ("fa", "month"), ("th", "long")];

#[test]
fn date_styles_match_icu() {
    let g = goldens();
    let mut failures = Vec::new();
    let mut diverged = 0;
    for r in rows(&g, "dates") {
        let [tag, date, style, want] = [0, 1, 2, 3].map(|i| r[i].as_str().unwrap());
        let got = i18n::date_format(loc(tag), date, style);
        if KNOWN_DIVERGENCES.contains(&(tag, style)) {
            // still differs — if ICU and molde ever agree, drop the entry
            if got != want {
                diverged += 1;
            }
            continue;
        }
        check(&mut failures, format!("{tag} {date} {style}"), got, want);
    }
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(
        diverged,
        4 * KNOWN_DIVERGENCES.len(),
        "a known divergence now matches ICU"
    );
}

#[test]
fn date_patterns_match_icu() {
    let g = goldens();
    let mut failures = Vec::new();
    for r in rows(&g, "patterns") {
        let [tag, date, pattern, want] = [0, 1, 2, 3].map(|i| r[i].as_str().unwrap());
        let got = i18n::date_pattern(loc(tag), date, pattern).unwrap();
        check(
            &mut failures,
            format!("{tag} {date} {pattern:?}"),
            got,
            want,
        );
    }
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
