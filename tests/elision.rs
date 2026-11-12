//! Elision where `t()` / `t_html()` insert a value (docs/reference.md, Elision), through
//! the real pipeline: site.yaml `elision:`, the rendered text, the `elision.undecided`
//! warning and the load errors.
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

const ROUTES: &str =
    "routes:\n  - path: /index.html\n    template: home.html.j2\n    title: \"{{ t('title') }}\"\n";

const HOME: &str = "<!doctype html><html lang=\"{{ locale }}\"><head><meta charset=\"utf-8\">\
{{ std_head }}</head><body>\
<p id=\"a\">{{ t('symbol', name='Aperam') }}</p>\
<p id=\"b\">{{ t('symbol', name='Bouygues') }}</p>\
<p id=\"h\">{{ t('symbol', name='Hermès') }}</p>\
<p id=\"u\">{{ t('symbol', name='Hannover Rück') }}</p>\
<p id=\"p\">{{ t('paid', pct=87) }}</p>\
<p id=\"f\">{{ t('pool.3', args={'name': 'ISIN', 'unused': 1}) }}</p>\
<p id=\"m\">{{ t_html('symbol', name='Enel') }}</p>\
<p id=\"v\">{{ t('pair', a='de', n='Aperam') }}</p>\
</body></html>";

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("molde-elision-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn site(name: &str, site_yaml: &str, home: &str) -> PathBuf {
    let dir = tmp(name);
    let files = [
        ("site.yaml", site_yaml),
        ("routes.yaml", ROUTES),
        ("data.json", "{}"),
        ("templates/home.html.j2", home),
        (
            "translations/en.yaml",
            "pair: \"{a} {n}\"\ntitle: Home\nsymbol: \"The symbol of {name}\"\npaid: \"It paid {pct}% of its earnings\"\npool:\n  \"3\": \"The symbol of the {name}\"\n",
        ),
        (
            "translations/fr.yaml",
            "pair: \"{a} {n}\"\ntitle: Accueil\nsymbol: \"Le symbole de {name}\"\npaid: \"Elle a versé {pct} % de ses bénéfices\"\npool:\n  \"3\": \"Le symbole de {name}\"\n",
        ),
        (
            "translations/it.yaml",
            "pair: \"{a} {n}\"\ntitle: Home\nsymbol: \"Il simbolo di {name}\"\npaid: \"Ha pagato il {pct}% dei suoi utili\"\npool:\n  \"3\": \"Il simbolo del {name}\"\n",
        ),
    ];
    for (path, body) in files {
        let f = dir.join(path);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, body).unwrap();
    }
    dir
}

/// The page's `<p id=…>` texts, by id.
fn paragraphs(dir: &Path, report: &Value, rel: &str) -> std::collections::BTreeMap<String, String> {
    let build = PathBuf::from(report["build"]["dir"].as_str().unwrap());
    let html = fs::read_to_string(build.join(rel))
        .unwrap_or_else(|_| panic!("{rel} missing in {}", dir.display()));
    html.split("<p id=\"")
        .skip(1)
        .map(|s| {
            let (id, rest) = s.split_once("\">").unwrap();
            (
                id.to_string(),
                rest.split("</p>").next().unwrap().to_string(),
            )
        })
        .collect()
}

const ON: &str = "base_url: https://ex.com\nlocales: [en, fr, it]\nelision:\n  fr:\n    vowel_sound: [Hermès]\n  it: {}\n";

#[test]
fn values_elide_the_word_before_them_in_french_and_italian() {
    let dir = site("on", ON, HOME);
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "pass", "{report:#}");

    let fr = paragraphs(&dir, &report, "fr/index.html");
    assert_eq!(fr["a"], "Le symbole d&#x27;Aperam");
    assert_eq!(fr["b"], "Le symbole de Bouygues");
    assert_eq!(fr["h"], "Le symbole d&#x27;Hermès", "listed in vowel_sound");
    assert_eq!(
        fr["u"], "Le symbole de Hannover Rück",
        "an undecided h is kept"
    );
    assert_eq!(fr["p"], "Elle a versé 87 % de ses bénéfices");
    assert_eq!(
        fr["m"], "Le symbole d'Enel",
        "t_html: the elided word is markup-safe text"
    );

    let it = paragraphs(&dir, &report, "it/index.html");
    assert_eq!(it["p"], "Ha pagato l&#x27;87% dei suoi utili");
    assert_eq!(
        it["f"], "Il simbolo dell&#x27;ISIN",
        "a text-pool variant (`args=`) elides too"
    );
    assert_eq!(it["a"], "Il simbolo di Aperam", "\"di\" is not elided");
    // only the translation's own text changes: a value that ends in "de" stays
    assert_eq!(fr["v"], "de Aperam");

    // English has no rules: untouched
    let en = paragraphs(&dir, &report, "index.html");
    assert_eq!(en["a"], "The symbol of Aperam");

    // the undecided h is a warning (the build still publishes), once per locale and word
    let undecided: Vec<&Value> = report["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["check"] == "elision.undecided")
        .collect();
    assert_eq!(undecided.len(), 1, "{report:#}");
    assert_eq!(undecided[0]["locale"], "fr");
    assert_eq!(undecided[0]["word"], "Hannover");
    assert_eq!(
        undecided[0]["templates"],
        serde_json::json!(["home.html.j2"])
    );
    assert!(undecided[0]["hint"]
        .as_str()
        .unwrap()
        .contains("consonant_sound"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn without_elision_the_output_is_unchanged() {
    let dir = site(
        "off",
        "base_url: https://ex.com\nlocales: [en, fr, it]\n",
        HOME,
    );
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "pass", "{report:#}");
    let fr = paragraphs(&dir, &report, "fr/index.html");
    assert_eq!(fr["a"], "Le symbole de Aperam");
    let it = paragraphs(&dir, &report, "it/index.html");
    assert_eq!(it["p"], "Ha pagato il 87% dei suoi utili");
    assert_eq!(it["f"], "Il simbolo del ISIN");
    assert!(report["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .all(|w| w["check"] != "elision.undecided"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn elision_config_is_validated_at_load() {
    for (yaml, want) in [
        (
            "locales: [en, fr]\nelision:\n  de: {}\n",
            "isn't in site.yaml locales",
        ),
        (
            "locales: [en, es]\nelision:\n  es: {}\n",
            "no elision rules",
        ),
        ("elision:\n  fr: {}\n", "needs `locales:`"),
        (
            "locales: [en, fr]\nelision:\n  fr:\n    silent_h: [x]\n",
            "silent_h",
        ),
        (
            "locales: [en, fr]\nelision:\n  fr:\n    vowel_sound: [Hera]\n    consonant_sound: [Hera]\n",
            "in both",
        ),
    ] {
        let dir = site("config", yaml, HOME);
        let err = match molde::testing::load_site(&dir) {
            Ok(_) => panic!("expected a load error for {yaml:?}"),
            Err(e) => format!("{e:#}"),
        };
        assert!(err.contains(want), "{yaml:?}: {err}");
        let _ = fs::remove_dir_all(&dir);
    }
}
