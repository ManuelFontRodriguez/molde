//! Elision where a value meets a translation: "de" + "Aperam" is "d'Aperam" in French,
//! "il" + "8" is "l'8" in Italian. Only the word right before an inserted value can
//! change, and only by the language's own rule; the rest of the sentence is the
//! translator's.
//!
//! A word starting with "h" is the one case the spelling can't settle (French "l'hôtel"
//! but "le héros"; Italian "l'hotel" but "lo hamburger"), so it is never elided unless
//! the site lists it, and `t()` reports it as `elision.undecided`.

/// Languages with rules, by primary language subtag (`fr-CA` uses `fr`).
pub const LANGUAGES: &[&str] = &["fr", "it"];

/// The language subtag of a locale tag: "fr-CA" -> "fr".
pub fn language(tag: &str) -> &str {
    tag.split(['-', '_']).next().unwrap_or(tag)
}

pub fn supported(tag: &str) -> bool {
    LANGUAGES.contains(&language(tag))
}

/// Words that elide before a vowel sound, lowercase, with their elided form.
fn words(lang: &str) -> &'static [(&'static str, &'static str)] {
    match lang {
        "fr" => &[
            ("de", "d'"),
            ("le", "l'"),
            ("la", "l'"),
            ("que", "qu'"),
            ("jusque", "jusqu'"),
            ("lorsque", "lorsqu'"),
            ("puisque", "puisqu'"),
            ("ne", "n'"),
            ("je", "j'"),
            ("me", "m'"),
            ("te", "t'"),
            ("se", "s'"),
        ],
        "it" => &[
            ("il", "l'"),
            ("lo", "l'"),
            ("la", "l'"),
            ("del", "dell'"),
            ("dello", "dell'"),
            ("della", "dell'"),
            ("al", "all'"),
            ("allo", "all'"),
            ("alla", "all'"),
            ("dal", "dall'"),
            ("dallo", "dall'"),
            ("dalla", "dall'"),
            ("nel", "nell'"),
            ("nello", "nell'"),
            ("nella", "nell'"),
            ("sul", "sull'"),
            ("sullo", "sull'"),
            ("sulla", "sull'"),
            ("una", "un'"),
            ("quello", "quell'"),
            ("quella", "quell'"),
        ],
        _ => &[],
    }
}

/// Values the site has decided, in site.yaml `elision: {<locale>: …}`: a value that
/// starts with one of `vowel_sound` elides, one that starts with a `consonant_sound`
/// entry doesn't, whatever the letter says (e.g. Hermès, a silent h).
#[derive(Debug, Default, Clone)]
pub struct Exceptions {
    pub vowel_sound: Vec<String>,
    pub consonant_sound: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Sound {
    Vowel,
    Consonant,
    /// starts with "h": the site must say (the word is returned for the report)
    Undecided(String),
}

/// How the value is read aloud at its start. `value` is the plain text (no markup).
pub fn sound(lang: &str, value: &str, exceptions: &Exceptions) -> Sound {
    let v = value.trim_start_matches(['"', '\'', '«', '“', '‘', '(', '[']);
    // the site's entries win over the rules; the longest matching entry decides
    // ("Ha" vowel, "Hamburger" consonant: "Hamburger" is a consonant)
    let longest = |list: &[String]| {
        list.iter()
            .filter(|w| !w.is_empty() && v.starts_with(w.as_str()))
            .map(String::len)
            .max()
    };
    match (
        longest(&exceptions.vowel_sound),
        longest(&exceptions.consonant_sound),
    ) {
        (Some(a), Some(b)) => {
            return if a > b {
                Sound::Vowel
            } else {
                Sound::Consonant
            }
        }
        (Some(_), None) => return Sound::Vowel,
        (None, Some(_)) => return Sound::Consonant,
        (None, None) => {}
    }
    let mut chars = v.chars();
    let Some(first) = chars.next() else {
        return Sound::Consonant;
    };
    let next = chars.next();
    let lower: String = first.to_lowercase().collect();
    let is_vowel = |c: &str| "aeiouàâäáéèêëíìîïóòôöúùûüœæ".contains(c);
    let first_word = || {
        v.split(|c: char| !c.is_alphanumeric())
            .next()
            .unwrap_or(v)
            .to_string()
    };
    match lang {
        "fr" => {
            // French writes "de 8 %", "de 11 h": no elision before figures
            if first.is_ascii_digit() {
                return Sound::Consonant;
            }
            let word = first_word().to_lowercase();
            if ["onze", "onzième", "oui"].contains(&word.as_str()) {
                return Sound::Consonant;
            }
            match lower.as_str() {
                // an acronym or ticker read letter by letter: "ache", a vowel ("d'HSBC")
                "h" if is_acronym(&first_word()) => Sound::Vowel,
                "h" => Sound::Undecided(first_word()),
                // y before a vowel is a consonant sound: "de Yamaha", "le yaourt"
                "y" => match next {
                    Some(n) if is_vowel(&n.to_lowercase().collect::<String>()) => Sound::Consonant,
                    _ => Sound::Vowel,
                },
                c if is_vowel(c) => Sound::Vowel,
                _ => Sound::Consonant,
            }
        }
        "it" => {
            if first.is_ascii_digit() {
                // read with a vowel: otto, ottanta, ottocento… and undici (not 110, 1100)
                let eleven =
                    v.starts_with("11") && !v[2..].starts_with(|c: char| c.is_ascii_digit());
                return if first == '8' || eleven {
                    Sound::Vowel
                } else {
                    Sound::Consonant
                };
            }
            match lower.as_str() {
                "h" => Sound::Undecided(first_word()),
                c if is_vowel(c) => Sound::Vowel,
                _ => Sound::Consonant,
            }
        }
        _ => Sound::Consonant,
    }
}

/// An acronym or ticker, read letter by letter: 2 to 5 plain capitals or digits, e.g.
/// "HSBC", "HNR1". A longer or accented capitalised word ("HERMÈS", "HEIDELBERG") is a
/// name in capitals, which the letter can't decide.
fn is_acronym(word: &str) -> bool {
    (2..=5).contains(&word.chars().count())
        && word
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// What to do where `before` (the text so far) meets a value.
#[derive(Debug, PartialEq, Eq)]
pub enum Join {
    /// nothing to change
    Keep,
    /// keep `before[..cut]`, then write `word` (the elided word, no space) and the value
    Elide { cut: usize, word: String },
    /// an elidable word meets an "h" value the site hasn't decided: kept as written
    Undecided(String),
}

/// `before` must end with an elidable word and exactly one space ("… de "), the word
/// standing alone: at the start of `before`, or after a space or punctuation other than
/// a hyphen or an apostrophe ("dis-le", "aujourd'hui" never elide). A value that starts
/// with whitespace is never elided (it would read "d' Aperam"). Case follows the word:
/// "De" -> "D'", "QUE" -> "QU'".
pub fn join(lang: &str, before: &str, value: &str, exceptions: &Exceptions) -> Join {
    let Some(head) = before.strip_suffix(' ') else {
        return Join::Keep;
    };
    if value.starts_with(char::is_whitespace) {
        return Join::Keep;
    }
    let boundary = head.char_indices().rev().find(|(_, c)| !c.is_alphabetic());
    if let Some((_, c)) = boundary {
        if matches!(c, '-' | '‐' | '‑' | '\'' | '’') {
            return Join::Keep;
        }
    }
    let start = boundary.map(|(i, c)| i + c.len_utf8()).unwrap_or(0);
    let word = &head[start..];
    if word.is_empty() {
        return Join::Keep;
    }
    let lower = word.to_lowercase();
    let Some((_, elided)) = words(lang).iter().find(|(w, _)| *w == lower) else {
        return Join::Keep;
    };
    match sound(lang, value, exceptions) {
        Sound::Consonant => Join::Keep,
        Sound::Undecided(w) => Join::Undecided(w),
        Sound::Vowel => {
            let all_caps = word.chars().count() > 1 && word.chars().all(char::is_uppercase);
            let out: String = if all_caps {
                elided.to_uppercase()
            } else if word.starts_with(char::is_uppercase) {
                let mut cs = elided.chars();
                cs.next()
                    .map(|c| c.to_uppercase().chain(cs).collect())
                    .unwrap_or_default()
            } else {
                elided.to_string()
            };
            Join::Elide {
                cut: start,
                word: out,
            }
        }
    }
}

/// The value's text for the sound test. A plain value is its own text. A markup value
/// (a safe argument of `t_html`) has its leading tags skipped (quoted attributes may
/// hold `>`) and a leading numeric or basic named entity decoded, so `<strong>Aperam`
/// and `&#201;ric` read as "Aperam" and "Éric" (other named entities stay as written,
/// so such a value is not elided).
pub fn leading_text(value: &str, markup: bool) -> String {
    if !markup {
        return value.to_string();
    }
    let mut v = value;
    while v.starts_with('<') {
        let mut quote: Option<char> = None;
        let mut end = None;
        for (i, c) in v.char_indices().skip(1) {
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) => {}
                None if c == '"' || c == '\'' => quote = Some(c),
                None if c == '>' => {
                    end = Some(i);
                    break;
                }
                None => {}
            }
        }
        match end {
            Some(i) => v = &v[i + 1..],
            None => return String::new(),
        }
    }
    match v.strip_prefix('&').and_then(|rest| rest.find(';')) {
        Some(i) if i < 32 => {
            let decoded = crate::check::html::unescape_entities(&v[..i + 2]);
            format!("{decoded}{}", &v[i + 2..])
        }
        _ => v.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(lang: &str, before: &str, value: &str) -> String {
        let ex = Exceptions::default();
        match join(lang, before, value, &ex) {
            Join::Elide { cut, word } => format!("{}{}{}", &before[..cut], word, value),
            _ => format!("{before}{value}"),
        }
    }

    #[test]
    fn french_elides_before_a_vowel_sound() {
        assert_eq!(e("fr", "Le symbole de ", "Aperam"), "Le symbole d'Aperam");
        assert_eq!(e("fr", "actions de ", "AEX"), "actions d'AEX");
        assert_eq!(e("fr", "actions de ", "Ibex 35"), "actions d'Ibex 35");
        assert_eq!(e("fr", "plus grand que ", "Engie"), "plus grand qu'Engie");
        assert_eq!(e("fr", "De ", "Enel"), "D'Enel");
        assert_eq!(e("fr", "de ", "Éiffage"), "d'Éiffage");
        assert_eq!(e("fr", "de ", "Yves Rocher"), "d'Yves Rocher");
    }

    #[test]
    fn french_keeps_consonants_figures_and_its_exceptions() {
        assert_eq!(e("fr", "de ", "Bouygues"), "de Bouygues");
        assert_eq!(e("fr", "de ", "8 %"), "de 8 %");
        assert_eq!(e("fr", "de ", "onze ans"), "de onze ans");
        assert_eq!(e("fr", "de ", "Yamaha"), "de Yamaha");
        assert_eq!(e("fr", "de ", "S&P 500"), "de S&P 500");
        // only a whole word elides
        assert_eq!(e("fr", "monde ", "Aperam"), "monde Aperam");
        assert_eq!(e("fr", "garde ", "Aperam"), "garde Aperam");
        // two spaces, no space, or an already elided word: untouched
        assert_eq!(e("fr", "de  ", "Aperam"), "de  Aperam");
        assert_eq!(e("fr", "de", "Aperam"), "deAperam");
        assert_eq!(e("fr", "aujourd'hui ", "Aperam"), "aujourd'hui Aperam");
    }

    #[test]
    fn french_reads_an_h_acronym_as_a_vowel() {
        assert_eq!(e("fr", "de ", "HSBC"), "d'HSBC");
        assert_eq!(e("fr", "de ", "HNR1"), "d'HNR1");
        // a name in capitals is still a word: only an acronym is spelled out
        let ex = Exceptions::default();
        assert_eq!(
            join("fr", "de ", "Henkel", &ex),
            Join::Undecided("Henkel".into())
        );
        assert_eq!(join("fr", "de ", "H", &ex), Join::Undecided("H".into()));
    }

    #[test]
    fn h_is_left_to_the_site() {
        let ex = Exceptions::default();
        assert_eq!(
            join("fr", "de ", "Hermès", &ex),
            Join::Undecided("Hermès".into())
        );
        let ex = Exceptions {
            vowel_sound: vec!["Hermès".into()],
            consonant_sound: vec!["Heineken".into()],
        };
        assert!(matches!(
            join("fr", "de ", "Hermès International", &ex),
            Join::Elide { .. }
        ));
        assert_eq!(join("fr", "de ", "Heineken", &ex), Join::Keep);
    }

    #[test]
    fn italian_numbers_read_with_a_vowel() {
        assert_eq!(e("it", "il ", "8 maggio"), "l'8 maggio");
        assert_eq!(e("it", "il ", "11 giugno"), "l'11 giugno");
        assert_eq!(e("it", "ha pagato il ", "87"), "ha pagato l'87");
        assert_eq!(e("it", "del ", "8,48"), "dell'8,48");
        assert_eq!(e("it", "Il ", "11"), "L'11");
        assert_eq!(e("it", "il ", "11.000"), "l'11.000");
        assert_eq!(e("it", "il ", "110"), "il 110");
        assert_eq!(e("it", "il ", "1100"), "il 1100");
        assert_eq!(e("it", "il ", "18 maggio"), "il 18 maggio");
        assert_eq!(e("it", "il ", "1 settembre"), "il 1 settembre");
    }

    #[test]
    fn italian_words() {
        assert_eq!(e("it", "il ticker del ", "ISIN"), "il ticker dell'ISIN");
        assert_eq!(e("it", "la ", "Enel"), "l'Enel");
        assert_eq!(e("it", "di ", "Enel"), "di Enel");
        assert_eq!(e("it", "il ", "Banco BPM"), "il Banco BPM");
    }

    #[test]
    fn markup_is_skipped_only_in_markup_values() {
        assert_eq!(
            leading_text("<strong>Aperam</strong>", true),
            "Aperam</strong>"
        );
        assert_eq!(leading_text(r#"<img alt="x>y">Aperam"#, true), "Aperam");
        assert_eq!(leading_text("&#201;ric", true), "Éric");
        // a named entity beyond the basic ones stays as written: no elision, never a guess
        assert_eq!(leading_text("&Eacute;ric", true), "&Eacute;ric");
        assert_eq!(leading_text("<b>Hamburger", false), "<b>Hamburger");
        assert_eq!(leading_text("<b", true), "");
    }

    #[test]
    fn review_cases() {
        let ex = Exceptions::default();
        // a value with leading whitespace is never elided
        assert_eq!(join("fr", "de ", " Aperam", &ex), Join::Keep);
        assert_eq!(join("fr", "de ", "\u{a0}Aperam", &ex), Join::Keep);
        // a clitic after a hyphen or an apostrophe stays
        assert_eq!(e("fr", "dis-le ", "Aperam"), "dis-le Aperam");
        assert_eq!(e("fr", "garde-de ", "Aperam"), "garde-de Aperam");
        // a name in capitals is not an acronym
        assert_eq!(
            join("fr", "de ", "HERMÈS", &ex),
            Join::Undecided("HERMÈS".into())
        );
        assert_eq!(
            join("fr", "de ", "HEIDELBERG", &ex),
            Join::Undecided("HEIDELBERG".into())
        );
        // all-caps words elide in caps
        assert_eq!(e("fr", "QUE ", "Engie"), "QU'Engie");
        assert_eq!(e("fr", "Que ", "Engie"), "Qu'Engie");
        // the longest exception decides
        let ex = Exceptions {
            vowel_sound: vec!["Ha".into()],
            consonant_sound: vec!["Hamburger".into()],
        };
        assert_eq!(join("fr", "de ", "Hamburger", &ex), Join::Keep);
        assert!(matches!(join("fr", "de ", "Hals", &ex), Join::Elide { .. }));
    }

    #[test]
    fn languages() {
        assert!(supported("fr") && supported("fr-CA") && supported("it"));
        assert!(!supported("es") && !supported("en"));
    }
}
