//! The `locale` capability: locale-aware formatting FILTERS, not code in the site.
//!
//! Contract data stays canonical (ISO dates, dot-decimal strings — see the exchange
//! doctrine); presentation is a template concern, so molde supplies it as filters the
//! way it supplies `std_head`: config on, capability exists. The data is Unicode CLDR,
//! generated into `i18n/cldr_data.rs` by `tools/cldr/generate.py` from pinned cldr-json
//! releases — no ICU, no host data, the same bytes on every machine
//! (docs/reference.md, Locale data). A site may declare a locale the data doesn't
//! cover: paths, hreflang and `t()` don't need it, and the filters fail at render time,
//! only when they actually format in that locale (`locale_data`).
//!
//! The output follows ICU4C (what PHP's intl prints): the CLDR date patterns per style,
//! `NumberFormatter::DECIMAL` grouping (minimum grouping digits not applied), half-even
//! rounding, the locale's default numbering system.
//!
//! Honesty rule carried from the contract: a value that does not parse as what the
//! filter formats (a real ISO `YYYY-MM-DD` date, a plain decimal string) is returned
//! UNCHANGED — filters never invent or mangle data. A locale without data, an unknown
//! style and a malformed pattern, by contrast, are authoring errors and fail loudly.

#[rustfmt::skip]
mod cldr_data;

pub use cldr_data::CLDR_VERSION;

/// One CLDR locale, Gregorian calendar only. Generated; see `tools/cldr/generate.py`.
#[derive(Debug)]
pub struct CldrLocale {
    pub tag: &'static str,
    /// format wide, abbreviated, narrow; stand-alone wide, abbreviated, narrow.
    pub months: [&'static [&'static str; 12]; 6],
    /// Same order as `months`, Sunday first.
    pub days: [&'static [&'static str; 7]; 6],
    /// The CE era: abbreviated, wide.
    pub era: [&'static str; 2],
    /// Date patterns: full, long, medium, short, and the `yMMMM` skeleton (`month`).
    pub dates: [&'static str; 5],
    /// The default numbering system's digits, 0-9.
    pub digits: &'static [char; 10],
    pub decimal: &'static str,
    pub group: &'static str,
    pub minus: &'static str,
    /// Primary and secondary grouping sizes (`#,##,##0` -> [3, 2]).
    pub grouping: [u8; 2],
}

/// Styles in the order error messages list them; index = position in `CldrLocale::dates`.
pub const STYLES: &[&str] = &["full", "long", "medium", "short", "month"];

/// "es-ES" | "es_ES" | "es" -> "es"
pub fn language(tag: &str) -> &str {
    tag.split(['-', '_']).next().unwrap_or(tag)
}

/// `pt_br` / `PT-BR` -> `pt-BR`, `zh-hant-tw` -> `zh-Hant-TW`: CLDR's casing.
fn canonical(tag: &str) -> String {
    let mut out = String::with_capacity(tag.len());
    for (i, part) in tag.split(['-', '_']).enumerate() {
        if i > 0 {
            out.push('-');
        }
        if i == 0 {
            out.push_str(&part.to_ascii_lowercase());
        } else if part.len() == 4 && part.bytes().all(|b| b.is_ascii_alphabetic()) {
            let (first, rest) = part.split_at(1);
            out.push_str(&first.to_ascii_uppercase());
            out.push_str(&rest.to_ascii_lowercase());
        } else if part.len() == 2 {
            out.push_str(&part.to_ascii_uppercase());
        } else {
            out.push_str(&part.to_ascii_lowercase());
        }
    }
    out
}

fn exact(tag: &str) -> Option<&'static CldrLocale> {
    cldr_data::LOCALES
        .binary_search_by(|l| l.tag.cmp(tag))
        .ok()
        .map(|i| &cldr_data::LOCALES[i])
}

fn alias(tag: &str) -> Option<&'static str> {
    cldr_data::ALIASES
        .binary_search_by(|(k, _)| (*k).cmp(tag))
        .ok()
        .map(|i| cldr_data::ALIASES[i].1)
}

/// The CLDR locale a tag formats with: the tag itself, its script alias (`zh-TW` ->
/// `zh-Hant`), or the tag with subtags dropped from the right (`es-ES` -> `es`). The
/// generated data is fully resolved, so a hit needs no further inheritance.
pub fn resolve(tag: &str) -> Option<&'static CldrLocale> {
    let mut tag = canonical(tag);
    loop {
        if let Some(l) = exact(&tag) {
            return Some(l);
        }
        if let Some(target) = alias(&tag) {
            tag = target.to_string();
            continue;
        }
        tag.truncate(tag.rfind('-')?);
    }
}

/// The CLDR data `filter` formats `tag` with, or the authoring error that names the
/// locale and the data. Called per render, so a locale without data only fails where a
/// filter really formats in it.
pub fn locale_data(filter: &str, tag: &str) -> Result<&'static CldrLocale, String> {
    resolve(tag).ok_or_else(|| {
        format!(
            "{filter}: locale '{tag}' has no formatting data — molde bundles CLDR {CLDR_VERSION} \
             for {} locales (coverage basic or better)",
            cldr_data::LOCALES.len()
        )
    })
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// A calendar date from strict ISO `YYYY-MM-DD` (years 0001-9999, the day must exist).
struct Date {
    year: u32,
    month: u32,
    day: u32,
}

impl Date {
    fn parse(s: &str) -> Option<Date> {
        let b = s.as_bytes();
        if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return None;
        }
        let digits = |r: std::ops::Range<usize>| -> Option<u32> {
            let part = &s[r];
            if part.bytes().all(|c| c.is_ascii_digit()) {
                part.parse().ok()
            } else {
                None
            }
        };
        let (year, month, day) = (digits(0..4)?, digits(5..7)?, digits(8..10)?);
        if year == 0 || !(1..=12).contains(&month) || day == 0 {
            return None;
        }
        if day > days_in_month(year, month) {
            return None;
        }
        Some(Date { year, month, day })
    }

    /// 0 = Sunday. Proleptic Gregorian (Sakamoto).
    fn weekday(&self) -> usize {
        const T: [u32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
        let y = if self.month < 3 {
            self.year - 1
        } else {
            self.year
        };
        ((y + y / 4 - y / 100 + y / 400 + T[self.month as usize - 1] + self.day) % 7) as usize
    }
}

enum Token<'a> {
    Field(char, usize),
    Literal(std::borrow::Cow<'a, str>),
}

/// LDML pattern -> tokens. `'text'` is literal, `''` is a quote, ASCII letters are
/// fields, everything else is literal. Errors name the problem for `pattern=`.
fn tokenize(pattern: &str) -> Result<Vec<Token<'_>>, String> {
    let mut out = Vec::new();
    let mut chars = pattern.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '\'' {
            if matches!(chars.peek(), Some((_, '\''))) {
                chars.next();
                out.push(Token::Literal("'".into()));
                continue;
            }
            let mut text = String::new();
            loop {
                match chars.next() {
                    None => return Err(format!("unterminated quote in pattern '{pattern}'")),
                    Some((_, '\'')) => {
                        if matches!(chars.peek(), Some((_, '\''))) {
                            chars.next();
                            text.push('\'');
                        } else {
                            break;
                        }
                    }
                    Some((_, ch)) => text.push(ch),
                }
            }
            out.push(Token::Literal(text.into()));
        } else if c.is_ascii_alphabetic() {
            let mut n = 1;
            while matches!(chars.peek(), Some((_, ch)) if *ch == c) {
                chars.next();
                n += 1;
            }
            let ok = match c {
                'G' => n <= 4,
                'y' | 'd' => c == 'y' || n <= 2,
                'M' | 'L' | 'E' => n <= 5,
                'c' => (3..=5).contains(&n),
                _ => false,
            };
            if !ok {
                return Err(format!(
                    "pattern '{pattern}': field '{}' is not supported — supported: G-GGGG, y+, \
                     M-MMMMM, L-LLLLL, d, dd, E-EEEEE, ccc-ccccc; quote literal text with ''",
                    c.to_string().repeat(n)
                ));
            }
            out.push(Token::Field(c, n));
        } else {
            let end = i + c.len_utf8();
            out.push(Token::Literal(pattern[i..end].into()));
        }
    }
    Ok(out)
}

/// A pattern the `pattern=` kwarg may pass: fails loudly before any value is formatted.
pub fn check_pattern(pattern: &str) -> Result<(), String> {
    tokenize(pattern).map(|_| ())
}

fn digits(loc: &CldrLocale, n: u32, width: usize) -> String {
    format!("{n:0width$}")
        .bytes()
        .map(|b| loc.digits[(b - b'0') as usize])
        .collect()
}

fn format_date(loc: &CldrLocale, date: &Date, pattern: &str) -> Result<String, String> {
    let mut out = String::new();
    for token in tokenize(pattern)? {
        match token {
            Token::Literal(s) => out.push_str(&s),
            Token::Field(c, n) => {
                let m = date.month as usize - 1;
                let w = date.weekday();
                match (c, n) {
                    ('G', 1..=3) => out.push_str(loc.era[0]),
                    ('G', _) => out.push_str(loc.era[1]),
                    ('y', 2) => out.push_str(&digits(loc, date.year % 100, 2)),
                    ('y', _) => out.push_str(&digits(loc, date.year, n)),
                    ('M' | 'L', 1 | 2) => out.push_str(&digits(loc, date.month, n)),
                    ('M', _) => out.push_str(loc.months[[1, 0, 2][n - 3]][m]),
                    ('L', _) => out.push_str(loc.months[[4, 3, 5][n - 3]][m]),
                    ('d', _) => out.push_str(&digits(loc, date.day, n)),
                    ('E', 1..=3) => out.push_str(loc.days[1][w]),
                    ('E', 4) => out.push_str(loc.days[0][w]),
                    ('E', _) => out.push_str(loc.days[2][w]),
                    ('c', _) => out.push_str(loc.days[[4, 3, 5][n - 3]][w]),
                    _ => unreachable!("tokenize rejects unsupported fields"),
                }
            }
        }
    }
    Ok(out)
}

/// A CLDR date style for an ISO date. Non-dates pass through unchanged. `style` has
/// already been validated by the filter wrapper.
pub fn date_format(loc: &CldrLocale, value: &str, style: &str) -> String {
    let Some(date) = Date::parse(value) else {
        return value.to_string();
    };
    let i = STYLES
        .iter()
        .position(|s| *s == style)
        .expect("style validated by the filter wrapper");
    format_date(loc, &date, loc.dates[i]).expect("generated patterns are checked by the generator")
}

/// An explicit LDML pattern (`EEEE, dd MMMM yyyy`) for an ISO date, with the locale's
/// names and digits. Non-dates pass through; a bad pattern is an error either way.
pub fn date_pattern(loc: &CldrLocale, value: &str, pattern: &str) -> Result<String, String> {
    check_pattern(pattern)?;
    match Date::parse(value) {
        Some(date) => format_date(loc, &date, pattern),
        None => Ok(value.to_string()),
    }
}

/// `num_format` options. `decimals: None` is the verbatim mode (the digits as given);
/// `grouping: None` means "the mode's default": off verbatim, on with `decimals`.
#[derive(Default, Clone, Copy)]
pub struct NumOptions {
    pub decimals: Option<usize>,
    pub grouping: Option<bool>,
}

/// Largest `decimals=` accepted; past it the call is an authoring error.
pub const MAX_DECIMALS: usize = 100;

/// `-?digits(.digits)?` -> (negative, integer digits, fraction digits).
fn parse_decimal(value: &str) -> Option<(bool, &str, &str)> {
    let (negative, bare) = match value.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, value),
    };
    let (int, frac) = match bare.split_once('.') {
        Some((int, frac)) if !frac.is_empty() => (int, frac),
        Some(_) => return None,
        None => (bare, ""),
    };
    let all_digits = |s: &str| s.bytes().all(|c| c.is_ascii_digit());
    (!int.is_empty() && all_digits(int) && all_digits(frac)).then_some((negative, int, frac))
}

/// Round `int.frac` to `n` fraction digits, half-even (ICU's default), on the decimal
/// digits themselves — no binary floating point. Leading integer zeros are dropped.
fn round_half_even(int: &str, frac: &str, n: usize) -> (String, String) {
    let mut digits: Vec<u8> = int.bytes().chain(frac.bytes().take(n)).collect();
    digits.extend(std::iter::repeat_n(b'0', n.saturating_sub(frac.len())));
    if frac.len() > n {
        let rest = &frac.as_bytes()[n..];
        let tail = rest[1..].iter().any(|&b| b != b'0');
        let last_odd = digits.last().is_some_and(|b| (b - b'0') % 2 == 1);
        if rest[0] > b'5' || (rest[0] == b'5' && (tail || last_odd)) {
            let mut i = digits.len();
            loop {
                if i == 0 {
                    digits.insert(0, b'1');
                    break;
                }
                i -= 1;
                if digits[i] == b'9' {
                    digits[i] = b'0';
                } else {
                    digits[i] += 1;
                    break;
                }
            }
        }
    }
    let split = digits.len() - n;
    let int: String = digits[..split].iter().map(|&b| b as char).collect();
    let frac: String = digits[split..].iter().map(|&b| b as char).collect();
    let trimmed = int.trim_start_matches('0');
    let int = if trimmed.is_empty() { "0" } else { trimmed };
    (int.to_string(), frac)
}

/// `1234567` -> `1,234,567` with the locale's sizes and separator.
fn group(loc: &CldrLocale, int: &str) -> String {
    let [primary, secondary] = loc.grouping.map(usize::from);
    if int.len() <= primary {
        return int.to_string();
    }
    let (head, last) = int.split_at(int.len() - primary);
    let mut parts = vec![last];
    let mut rest = head;
    while rest.len() > secondary {
        let (h, t) = rest.split_at(rest.len() - secondary);
        parts.push(t);
        rest = h;
    }
    parts.push(rest);
    parts.reverse();
    parts.join(loc.group)
}

/// Decimal-string presentation. Only values shaped `-?digits(.digits)?` are touched;
/// anything else passes through. Verbatim (no `decimals`): the digits as given, the
/// locale's separator, minus sign and digits. With `decimals`: ICU `DECIMAL` with
/// min = max fraction digits, half-even rounding, grouping on.
pub fn num_format(loc: &CldrLocale, value: &str, opts: NumOptions) -> String {
    let Some((negative, int, frac)) = parse_decimal(value) else {
        return value.to_string();
    };
    let (int, frac) = match opts.decimals {
        Some(n) => round_half_even(int, frac, n),
        None => (int.to_string(), frac.to_string()),
    };
    let grouping = opts.grouping.unwrap_or(opts.decimals.is_some());
    let int = if grouping { group(loc, &int) } else { int };
    let localize = |s: &str| -> String {
        s.chars()
            .map(|c| match c.to_digit(10) {
                Some(d) => loc.digits[d as usize],
                None => c,
            })
            .collect()
    };
    let mut out = String::new();
    if negative {
        out.push_str(loc.minus);
    }
    // the group separator is inserted before localizing: none of CLDR's separators
    // contains an ASCII digit, so only the number's own digits change
    out.push_str(&localize(&int));
    if !frac.is_empty() {
        out.push_str(loc.decimal);
        out.push_str(&localize(&frac));
    }
    out
}

/// `"2026-04-29" | year` -> "2026". Always registered (no locale needed).
/// Anything that doesn't start `YYYY-` passes through unchanged.
pub fn year(value: &str) -> String {
    let b = value.as_bytes();
    if b.len() >= 5 && b[4] == b'-' && value[..4].bytes().all(|c| c.is_ascii_digit()) {
        value[..4].to_string()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(tag: &str) -> &'static CldrLocale {
        resolve(tag).unwrap_or_else(|| panic!("no data for {tag}"))
    }

    fn num(tag: &str, v: &str, decimals: Option<usize>) -> String {
        num_format(
            loc(tag),
            v,
            NumOptions {
                decimals,
                grouping: None,
            },
        )
    }

    #[test]
    fn es_dates() {
        let es = loc("es");
        assert_eq!(
            date_format(es, "2026-11-03", "long"),
            "3 de noviembre de 2026"
        );
        assert_eq!(date_format(es, "2026-11-03", "medium"), "3 nov 2026");
        assert_eq!(date_format(es, "2026-11-03", "short"), "3/11/26");
        assert_eq!(date_format(es, "2026-09-18", "medium"), "18 sept 2026");
    }

    #[test]
    fn en_dates() {
        let en = loc("en");
        assert_eq!(date_format(en, "2026-11-03", "long"), "November 3, 2026");
        assert_eq!(date_format(en, "2026-11-03", "medium"), "Nov 3, 2026");
        assert_eq!(date_format(en, "2026-11-03", "short"), "11/3/26");
    }

    #[test]
    fn non_dates_pass_through_unchanged() {
        for junk in [
            "",
            "N/D",
            "2026-13-01",
            "2026-00-10",
            "29-10-2026",
            "2026-1-3",
            "20261103",
            // the day must exist, the year must be 0001-9999
            "2026-02-29",
            "2026-04-31",
            "0000-01-01",
            "２０２６-03-05",
            "2026-03-05T10:00:00Z",
        ] {
            assert_eq!(
                date_format(loc("es"), junk, "long"),
                junk,
                "input: {junk:?}"
            );
            assert_eq!(
                date_pattern(loc("es"), junk, "EEEE").unwrap(),
                junk,
                "input: {junk:?}"
            );
        }
        assert_eq!(
            date_format(loc("en"), "2024-02-29", "long"),
            "February 29, 2024"
        );
    }

    #[test]
    fn numbers_verbatim() {
        assert_eq!(num("es", "0.0850", None), "0,0850");
        assert_eq!(num("es", "-3.90", None), "-3,90");
        assert_eq!(num("es", "48", None), "48");
        assert_eq!(num("en", "0.0850", None), "0.0850");
        // verbatim keeps the digits as given, ungrouped
        assert_eq!(num("de", "1234567.5", None), "1234567,5");
        assert_eq!(num("de", "007.50", None), "007,50");
        // not plausible decimals -> untouched, in both modes
        for junk in [
            "", "1.2.3", "v1.0", ".5", "5.", "1,5", "+1", "1e5", "-", "NaN",
        ] {
            assert_eq!(num("es", junk, None), junk, "input: {junk:?}");
            assert_eq!(num("es", junk, Some(2)), junk, "input: {junk:?}");
        }
    }

    #[test]
    fn numbers_with_decimals_round_half_even_and_group() {
        let cases = [
            ("0.125", 2, "0.12"),
            ("0.135", 2, "0.14"),
            ("2.675", 2, "2.68"),
            ("0.1251", 2, "0.13"),
            ("2.5", 0, "2"),
            ("3.5", 0, "4"),
            ("999.995", 2, "1,000.00"),
            ("9.5", 0, "10"),
            ("-0.001", 2, "-0.00"),
            ("0", 2, "0.00"),
            ("007.5", 1, "7.5"),
            ("1234567.891", 2, "1,234,567.89"),
            (
                "123456789012345678901234567890.5",
                0,
                "123,456,789,012,345,678,901,234,567,890",
            ),
        ];
        for (v, n, want) in cases {
            assert_eq!(num("en", v, Some(n)), want, "{v} decimals={n}");
        }
        // Indian grouping, secondary size 2
        assert_eq!(num("hi", "1234567.5", Some(1)), "12,34,567.5");
        // grouping=false in decimals mode, grouping=true in verbatim mode
        let de = loc("de");
        let off = NumOptions {
            decimals: Some(2),
            grouping: Some(false),
        };
        assert_eq!(num_format(de, "1234.5", off), "1234,50");
        let on = NumOptions {
            decimals: None,
            grouping: Some(true),
        };
        assert_eq!(num_format(de, "1234.5", on), "1.234,5");
    }

    #[test]
    fn native_digits_follow_the_default_numbering_system() {
        // CLDR 48: ar defaults to latn, ar-EG and fa to Arabic-Indic digits
        assert_eq!(num("ar", "-1234.5", Some(2)), "\u{200E}-1,234.50");
        assert_eq!(num("ar-EG", "-1234.5", Some(2)), "\u{061C}-١٬٢٣٤٫٥٠");
        assert_eq!(num("fa", "1234.5", Some(2)), "۱٬۲۳۴٫۵۰");
        assert_eq!(
            date_format(loc("ar-EG"), "2026-03-05", "long"),
            "٥ مارس ٢٠٢٦"
        );
    }

    #[test]
    fn patterns() {
        let en = loc("en");
        let d = "2026-03-05";
        assert_eq!(
            date_pattern(en, d, "EEEE, dd MMMM yyyy").unwrap(),
            "Thursday, 05 March 2026"
        );
        assert_eq!(
            date_pattern(en, d, "EEE d MMM yy G").unwrap(),
            "Thu 5 Mar 26 AD"
        );
        assert_eq!(
            date_pattern(en, d, "'week of' d. 'o''clock' ''").unwrap(),
            "week of 5. o'clock '"
        );
        assert_eq!(date_pattern(loc("ru"), d, "MMMM").unwrap(), "марта");
        assert_eq!(date_pattern(loc("ru"), d, "LLLL").unwrap(), "март");
        assert_eq!(
            date_pattern(en, "0999-01-01", "y yyyy").unwrap(),
            "999 0999"
        );
        for bad in ["HH:mm", "yyyy-QQ", "D", "'open", "c", "EEEEEE"] {
            let err = date_pattern(en, d, bad).unwrap_err();
            assert!(err.contains("pattern"), "{bad}: {err}");
        }
    }

    #[test]
    fn weekdays() {
        let en = loc("en");
        for (d, want) in [
            ("2026-03-05", "Thursday"),
            ("2000-01-01", "Saturday"),
            ("1900-03-01", "Thursday"),
            ("2024-02-29", "Thursday"),
            ("0001-01-01", "Monday"),
            ("9999-12-31", "Friday"),
        ] {
            assert_eq!(date_pattern(en, d, "EEEE").unwrap(), want, "{d}");
        }
    }

    #[test]
    fn year_extraction() {
        assert_eq!(year("2026-04-29"), "2026");
        assert_eq!(year("2026-04-29T07:00:00Z"), "2026");
        for junk in ["", "abcd-01", "2026", "202-04-29"] {
            assert_eq!(year(junk), junk, "input: {junk:?}");
        }
    }

    #[test]
    fn resolution() {
        assert_eq!(language("es-ES"), "es");
        assert_eq!(language("en_US"), "en");
        for (tag, want) in [
            ("es-ES", "es"),
            ("es_ES", "es"),
            ("pt-BR", "pt"),
            ("pt-PT", "pt-PT"),
            ("pt_pt", "pt-PT"),
            ("en-GB", "en-GB"),
            ("de-AT", "de-AT"),
            ("zh", "zh"),
            ("zh-CN", "zh"),
            ("zh-TW", "zh-Hant"),
            ("zh-HK", "zh-Hant-HK"),
            ("zh-hant-tw", "zh-Hant"),
            ("sr-ME", "sr-Latn-ME"),
            ("no", "no"),
            ("nb", "nb"),
            ("nn-NO", "nn"),
            ("iw", "he"),
            ("en-ZZ", "en"),
        ] {
            assert_eq!(loc(tag).tag, want, "{tag}");
        }
        for none in ["tlh", "xx", "", "und", "aa"] {
            assert!(resolve(none).is_none(), "{none}");
        }
        let err = locale_data("date_format", "tlh").unwrap_err();
        assert!(
            err.contains("date_format")
                && err.contains("'tlh'")
                && err.contains(&format!("CLDR {CLDR_VERSION}")),
            "{err}"
        );
    }

    /// The generated table is what the code assumes: sorted, fully resolved, sane.
    #[test]
    fn generated_data_invariants() {
        let tags: Vec<_> = cldr_data::LOCALES.iter().map(|l| l.tag).collect();
        assert!(tags.windows(2).all(|w| w[0] < w[1]), "LOCALES sorted");
        assert!(cldr_data::ALIASES.windows(2).all(|w| w[0].0 < w[1].0));
        for (from, to) in cldr_data::ALIASES.iter() {
            assert!(exact(to).is_some(), "alias {from} -> {to}");
        }
        for l in &cldr_data::LOCALES {
            for p in l.dates {
                check_pattern(p).unwrap_or_else(|e| panic!("{}: {e}", l.tag));
            }
            assert!(l.grouping[0] > 0 && l.grouping[1] > 0, "{}", l.tag);
            assert!(
                [l.decimal, l.group, l.minus]
                    .iter()
                    .all(|s| !s.bytes().any(|b| b.is_ascii_digit())),
                "{}",
                l.tag
            );
        }
    }
}
