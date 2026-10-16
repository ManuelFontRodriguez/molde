//! The `locales` capability (docs/reference.md, Locales) through the real pipeline: routes ×
//! locales, per-item localized slugs, hreflang from the manifest, `t()`, and the checks
//! that ship with it.
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

const SITE: &str = "base_url: https://ex.com\nlocales: [es, en, de]\n";

const ROUTES: &str = r#"routes:
  - path: /index.html
    template: home.html.j2
    title: "{{ t('home.title') }}"
  - path:
      es: /libros/{slug}/index.html
      en: /books/{slug}/index.html
      de: /buecher/{slug}/index.html
    template: book.html.j2
    foreach: books
    localized: [slug, name]
    title: "{{ item.name | loc }} ({{ item.isbn }})"
"#;

const DATA: &str = r#"{"books": [
  {"isbn": "B1", "slug": {"es": "don-quixote", "en": "don-quixote", "de": "don-quixote"},
   "name": {"es": "Don Quixote", "en": "Don Quixote", "de": "Don Quixote"}},
  {"isbn": "B2", "slug": {"es": "emma", "en": "emma", "de": null},
   "name": {"es": "Emma", "en": "Emma", "de": null}}
]}"#;

const HOME: &str = "<!doctype html><html lang=\"{{ locale }}\"><head><meta charset=\"utf-8\">\
{{ std_head }}</head><body><p>{{ t('greet', name='<b>') }}</p><p>{{ t_html('greet', name='<b>') }}</p>\
{% for a in alternates %}<a href=\"{{ a.path }}\">{{ a.locale }}</a>{% endfor %}</body></html>";

const BOOK: &str = "<!doctype html><html lang=\"{{ locale }}\"><head><meta charset=\"utf-8\">\
{{ std_head }}</head><body><h1>{{ item.name | loc }}</h1>\
<p>{{ \"2026-03-05\" | date_format }}</p><p>{{ \"0.5\" | num_format }}</p></body></html>";

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("molde-locales-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A site dir from (relative path, contents) pairs, over the defaults above.
fn site(name: &str, overrides: &[(&str, &str)]) -> PathBuf {
    let dir = tmp(name);
    let mut files: Vec<(&str, &str)> = vec![
        ("site.yaml", SITE),
        ("routes.yaml", ROUTES),
        ("data.json", DATA),
        ("templates/home.html.j2", HOME),
        ("templates/book.html.j2", BOOK),
        (
            "translations/es.yaml",
            "home:\n  title: Inicio\ngreet: \"Hola, {name}\"\n",
        ),
        (
            "translations/en.yaml",
            "home:\n  title: Home\ngreet: \"Hello, <i>{name}</i>\"\n",
        ),
        (
            "translations/de.yaml",
            "home.title: Startseite\ngreet: \"Hallo, {name}\"\n",
        ),
    ];
    for (path, body) in overrides {
        files.retain(|(p, _)| p != path);
        files.push((path, body));
    }
    for (path, body) in files {
        if body == "-" {
            continue; // "-" deletes a default file
        }
        let f = dir.join(path);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, body).unwrap();
    }
    dir
}

fn check(dir: &Path) -> Value {
    molde::run_check(dir, &dir.join("out")).unwrap()
}

fn read(dir: &Path, report: &Value, rel: &str) -> String {
    let build = PathBuf::from(report["build"]["dir"].as_str().unwrap());
    fs::read_to_string(build.join(rel))
        .unwrap_or_else(|_| panic!("{rel} missing in {}", dir.display()))
}

fn checks(report: &Value) -> Vec<String> {
    report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["check"].as_str().unwrap().to_string())
        .collect()
}

fn load_err(dir: &Path) -> String {
    match molde::testing::load_site(dir) {
        Ok(_) => panic!("expected a load error"),
        Err(e) => format!("{e:#}"),
    }
}

#[test]
fn routes_expand_per_locale_with_default_at_root() {
    let dir = site("expand", &[]);
    let s = molde::testing::load_site(&dir).unwrap();
    let pages = molde::testing::expand_routes(&s).unwrap();
    let paths: Vec<&str> = pages.iter().map(|p| p.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "/index.html",
            "/en/index.html",
            "/de/index.html",
            "/libros/don-quixote/index.html",
            "/libros/emma/index.html",
            "/en/books/don-quixote/index.html",
            "/en/books/emma/index.html",
            "/de/buecher/don-quixote/index.html",
        ]
    );
    // Emma has no de value: no de page, and its group is es + en
    let emma_page = pages
        .iter()
        .find(|p| p.path == "/en/books/emma/index.html")
        .unwrap();
    let group: Vec<&str> = emma_page
        .alternates
        .iter()
        .map(|a| a.locale.as_str())
        .collect();
    assert_eq!(group, ["es", "en"]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn full_site_passes_with_hreflang_lang_and_translations() {
    let dir = site("pass", &[]);
    let report = check(&dir);
    assert_eq!(report["verdict"], "pass", "{report:#}");
    assert_eq!(report["warnings"], serde_json::json!([]));

    let home = read(&dir, &report, "index.html");
    assert!(home.contains("<html lang=\"es\">"), "{home}");
    assert!(home.contains("<title>Inicio</title>"), "{home}");
    for tag in [
        "<link rel=\"alternate\" hreflang=\"es\" href=\"https://ex.com/\">",
        "<link rel=\"alternate\" hreflang=\"en\" href=\"https://ex.com/en/\">",
        "<link rel=\"alternate\" hreflang=\"de\" href=\"https://ex.com/de/\">",
        "<link rel=\"alternate\" hreflang=\"x-default\" href=\"https://ex.com/\">",
    ] {
        assert!(home.contains(tag), "missing {tag} in {home}");
    }
    // canonical stays self-referencing
    let en = read(&dir, &report, "en/index.html");
    assert!(
        en.contains("<link rel=\"canonical\" href=\"https://ex.com/en/\">"),
        "{en}"
    );
    // t() is escaped; t_html keeps the translation's markup but escapes its arguments
    assert!(
        en.contains("<p>Hello, &lt;i&gt;&lt;b&gt;&lt;/i&gt;</p><p>Hello, <i>&lt;b&gt;</i></p>"),
        "{en}"
    );
    // alternates in the template context, in site locale order
    assert!(en.contains("<a href=\"/\">es</a><a href=\"/en/\">en</a><a href=\"/de/\">de</a>"));

    // date/number filters follow the page's locale
    let de = read(&dir, &report, "de/buecher/don-quixote/index.html");
    assert!(de.contains("<p>5. März 2026</p><p>0,5</p>"), "{de}");
    let en_b1 = read(&dir, &report, "en/books/don-quixote/index.html");
    assert!(en_b1.contains("<p>March 5, 2026</p><p>0.5</p>"), "{en_b1}");

    // a group of one member emits no hreflang; x-default needs the default member
    let emma_page = read(&dir, &report, "en/books/emma/index.html");
    assert!(!emma_page.contains("hreflang=\"de\""), "{emma_page}");
    assert!(emma_page.contains("hreflang=\"x-default\" href=\"https://ex.com/libros/emma/\""));

    // the sitemap lists every locale's pages
    let sitemap = read(&dir, &report, "sitemap.xml");
    assert_eq!(sitemap.matches("<loc>").count(), 8, "{sitemap}");

    // the report counts pages per route × locale
    assert_eq!(
        report["pages_by_locale"][1]["pages"],
        serde_json::json!({"de": 1, "en": 2, "es": 2})
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn default_locale_paths_do_not_move_when_locales_are_added() {
    let paths = |name: &str, locales: &str| -> Vec<String> {
        let routes = "routes:\n  - path: /index.html\n    template: home.html.j2\n  \
                      - path: /books/{isbn}/index.html\n    template: book.html.j2\n    \
                      foreach: books\n";
        let dir = site(
            name,
            &[
                ("site.yaml", &format!("locales: {locales}\n")),
                ("routes.yaml", routes),
                (
                    "data.json",
                    r#"{"books": [{"isbn": "B1"}, {"isbn": "B2"}]}"#,
                ),
                ("translations/en.yaml", "-"),
                ("translations/de.yaml", "-"),
            ],
        );
        let s = molde::testing::load_site(&dir).unwrap();
        let out = molde::testing::expand_routes(&s)
            .unwrap()
            .into_iter()
            .filter(|p| p.locale.as_deref() == Some("es"))
            .map(|p| p.path)
            .collect();
        let _ = fs::remove_dir_all(&dir);
        out
    };
    let one = paths("one", "[es]");
    assert_eq!(one.len(), 3);
    assert_eq!(one, paths("three", "[es, en, de]"));
}

#[test]
fn printing_a_localized_field_is_a_render_error() {
    let book = BOOK.replace("{{ item.name | loc }}", "{{ item.name }}");
    let dir = site("print", &[("templates/book.html.j2", &book)]);
    let msg = source_error(&dir);
    assert!(msg.contains("item.name") && msg.contains("| loc"), "{msg}");
    // every other way to text fails too, in templates and in route patterns
    for (i, print) in [
        "{{ item.name | upper }}",
        "{{ item.name ~ '' }}",
        "{{ item.name | tojson }}",
        "{{ item }}",
    ]
    .iter()
    .enumerate()
    {
        let book = BOOK.replace("{{ item.name | loc }}", print);
        let dir = site(&format!("print{i}"), &[("templates/book.html.j2", &book)]);
        let msg = source_error(&dir);
        assert!(
            msg.contains("item.name") && msg.contains("| loc"),
            "{print}: {msg}"
        );
        let _ = fs::remove_dir_all(&dir);
    }
    let routes = ROUTES.replace("{{ item.name | loc }} (", "{{ item.name | upper }} (");
    let dir = site("print-title", &[("routes.yaml", &routes)]);
    let msg = source_error(&dir);
    assert!(
        msg.contains("title pattern") && msg.contains("item.name"),
        "{msg}"
    );
    // indexing by locale is fine
    let book = BOOK.replace("{{ item.name | loc }}", "{{ item.name[locale] }}");
    let dir = site("index", &[("templates/book.html.j2", &book)]);
    assert_eq!(check(&dir)["verdict"], "pass");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn translation_coverage_is_reported() {
    let dir = site(
        "tr",
        &[
            (
                "translations/en.yaml",
                "home:\n  title: Home\ngreet: \"Hello, {who}\"\nold: x\n",
            ),
            ("translations/de.yaml", "home.title: Startseite\n"),
        ],
    );
    let report = check(&dir);
    let mut got = checks(&report);
    got.sort();
    assert_eq!(
        got,
        [
            "translations.args",
            "translations.extra",
            "translations.missing"
        ],
        "{report:#}"
    );
    // a missing translation renders empty, never falls back to the default
    let de = read(&dir, &report, "de/index.html");
    assert!(
        !de.contains("Hola") && de.contains("<p></p><p></p>"),
        "{de}"
    );
    // a translation's unknown placeholder stays literal (and is reported above)
    let en = read(&dir, &report, "en/index.html");
    assert!(en.contains("<p>Hello, {who}</p>"), "{en}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unknown_translation_key_is_reported_not_aborted() {
    let home = HOME.replace("t('greet', name='<b>') }}</p><p>", "t('nope') }}</p><p>");
    let dir = site("unknown", &[("templates/home.html.j2", &home)]);
    let report = check(&dir);
    let unknown: Vec<&Value> = report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["check"] == "translations.unknown_key")
        .collect();
    assert_eq!(unknown.len(), 1, "one per key: {report:#}");
    assert_eq!(unknown[0]["key"], "nope");
    assert_eq!(unknown[0]["template"], "home.html.j2");
    assert_eq!(unknown[0]["locales"], serde_json::json!(["de", "en", "es"]));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn missing_t_argument_is_a_render_error() {
    let home = HOME.replace("t('greet', name='<b>')", "t('greet')");
    let dir = site("args", &[("templates/home.html.j2", &home)]);
    let msg = source_error(&dir);
    assert!(msg.contains("name="), "{msg}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn wrong_html_lang_and_stray_hreflang_fail() {
    let home = HOME
        .replace("lang=\"{{ locale }}\"", "lang=\"es\"")
        .replace(
            "{{ std_head }}",
            // single-quoted, and a second x-default: both must be seen
            "{{ std_head }}<link rel='alternate' hreflang='fr' href='https://ex.com/fr/'>\
             <link rel=\"alternate\" hreflang=\"x-default\" href=\"https://ex.com/\">",
        );
    // a `locales: none` page has no group, so any hreflang on it is stray
    let routes = format!(
        "{ROUTES}  - path: /404.html\n    template: home.html.j2\n    locales: none\n    index: false\n"
    );
    let dir = site(
        "lang",
        &[("templates/home.html.j2", &home), ("routes.yaml", &routes)],
    );
    let report = check(&dir);
    let errors = report["errors"].as_array().unwrap();
    let lang: Vec<&str> = errors
        .iter()
        .filter(|e| e["check"] == "html.lang")
        .map(|e| e["page"].as_str().unwrap())
        .collect();
    assert_eq!(lang, ["en/index.html", "de/index.html"], "{report:#}");
    let stray = errors
        .iter()
        .filter(|e| e["check"] == "hreflang.mismatch")
        .count();
    assert_eq!(stray, 4, "{report:#}");
    let first = errors
        .iter()
        .find(|e| e["check"] == "hreflang.mismatch")
        .unwrap();
    assert_eq!(first["extra"], serde_json::json!(["fr"]));
    assert_eq!(first["duplicate"], serde_json::json!(["x-default"]));
    assert!(errors
        .iter()
        .any(|e| e["check"] == "hreflang.mismatch" && e["page"] == "404.html"));
    assert!(first["hint"].is_string() && first["route"] == "/index.html");
    assert_eq!(report["published"], false);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn titles_may_repeat_across_locales_but_not_within_one() {
    // every book page is titled "Don Quixote (B1)" in all locales: fine
    let dir = site("titles", &[]);
    assert_eq!(check(&dir)["verdict"], "pass");
    let routes = ROUTES.replace("{{ item.name | loc }} ({{ item.isbn }})", "Book");
    let dir = site("titles-dup", &[("routes.yaml", &routes)]);
    let report = check(&dir);
    let dups: Vec<&Value> = report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["check"] == "titles.duplicate")
        .collect();
    // es and en each hold two "Book" pages; de holds one
    assert_eq!(dups.len(), 2, "{report:#}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn root_namespace_paths_and_chars_are_checked() {
    let routes = format!(
        "{ROUTES}  - path: /en/about.html\n    template: home.html.j2\n    locales: [es]\n    title: About\n"
    );
    let data = DATA.replace("\"es\": \"emma\"", "\"es\": \"emmá\"");
    let dir = site("paths", &[("routes.yaml", &routes), ("data.json", &data)]);
    let report = check(&dir);
    assert!(
        report["build"].is_null(),
        "routes-layer errors abort: {report:#}"
    );
    let mut got = checks(&report);
    got.sort();
    assert_eq!(
        got,
        ["paths.invalid_chars", "paths.locale_prefix"],
        "{report:#}"
    );
    assert!(molde::build(&dir, &dir.join("out")).is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn prefix_default_locale_needs_a_root_page() {
    let site_yaml = format!("{SITE}prefix_default_locale: true\n");
    let dir = site("prefix", &[("site.yaml", &site_yaml)]);
    let report = check(&dir);
    assert_eq!(checks(&report), ["paths.root_missing"], "{report:#}");

    let routes = format!(
        "{ROUTES}  - path: /index.html\n    template: chooser.html.j2\n    locales: none\n    title: Choose\n"
    );
    let dir = site(
        "prefix-ok",
        &[
            ("site.yaml", &site_yaml),
            ("routes.yaml", &routes),
            (
                "templates/chooser.html.j2",
                "<html lang=\"mul\"><head>{{ std_head }}</head><body>{{ locale }}</body></html>",
            ),
        ],
    );
    let s = molde::testing::load_site(&dir).unwrap();
    let pages = molde::testing::expand_routes(&s).unwrap();
    assert!(pages.iter().any(|p| p.path == "/es/index.html"));
    assert!(pages
        .iter()
        .any(|p| p.path == "/es/libros/don-quixote/index.html"));
    let report = check(&dir);
    assert_eq!(report["verdict"], "pass", "{report:#}");
    // a `none` page renders with the default locale and joins no group
    let chooser = read(&dir, &report, "index.html");
    assert!(
        chooser.contains("<body>es</body>") && !chooser.contains("hreflang"),
        "{chooser}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn paginate_lists_only_the_items_of_each_locale() {
    let routes = r#"routes:
  - path: /index.html
    template: list.html.j2
    title: "{{ t('home.title') }} {{ pagination.n }}"
    localized: [name]
    paginate:
      over: books
      per_page: 1
      path:
        es: /pagina/{n}/index.html
        en: /page/{n}/index.html
        de: /seite/{n}/index.html
"#;
    let list = "<html lang=\"{{ locale }}\"><head>{{ std_head }}</head><body>\
{% for s in pagination.items %}{{ s.name | loc }}{% endfor %}|{{ pagination.total }}</body></html>";
    let dir = site(
        "paginate",
        &[("routes.yaml", routes), ("templates/list.html.j2", list)],
    );
    let s = molde::testing::load_site(&dir).unwrap();
    let paths: Vec<String> = molde::testing::expand_routes(&s)
        .unwrap()
        .into_iter()
        .map(|p| p.path)
        .collect();
    assert_eq!(
        paths,
        [
            "/index.html",
            "/pagina/2/index.html",
            "/en/index.html",
            "/en/page/2/index.html",
            "/de/index.html",
        ]
    );
    let report = check(&dir);
    assert_eq!(report["verdict"], "pass", "{report:#}");
    assert!(read(&dir, &report, "de/index.html").contains("<body>Don Quixote|1</body>"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sparse_locale_is_a_warning_not_an_error() {
    let data = r#"{"books": [
  {"isbn": "A", "slug": {"es": "a", "en": "a"}, "name": {"es": "A", "en": "A"}},
  {"isbn": "B", "slug": {"es": "b", "en": "b"}, "name": {"es": "B", "en": "B"}},
  {"isbn": "C", "slug": {"es": "c", "en": "c", "de": "c"}, "name": {"es": "C", "en": "C", "de": "C"}}
]}"#;
    let dir = site("sparse", &[("data.json", data)]);
    let report = check(&dir);
    assert_eq!(report["verdict"], "pass", "{report:#}");
    let w = report["warnings"].as_array().unwrap();
    assert_eq!(w.len(), 1, "{report:#}");
    assert_eq!(w[0]["check"], "i18n.sparse_locale");
    assert_eq!(w[0]["locale"], "de");
    assert_eq!(w[0]["pages"], 1);
    assert_eq!(w[0]["default_pages"], 3);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_site_without_locales_has_no_warnings_key() {
    let dir = tmp("plain");
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: p.html.j2\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    fs::write(dir.join("templates/p.html.j2"), "<title>x</title>").unwrap();
    let report = check(&dir);
    assert_eq!(report["verdict"], "pass");
    assert!(report.get("warnings").is_none());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn config_errors_fail_at_load() {
    type Case<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a str);
    let cases: &[Case] = &[
        ("both", &[("site.yaml", "locale: es\nlocales: [es]\n")], "can't both be set"),
        ("badtag", &[("site.yaml", "locales: [es, EN]\n")], "\"EN\""),
        ("badregion", &[("site.yaml", "locales: [es, ja-jp]\n")], "\"ja-jp\""),
        ("twice", &[("site.yaml", "locales: [es, es]\n")], "twice"),
        ("prefixonly", &[("site.yaml", "prefix_default_locale: true\n")], "needs `locales:`"),
        (
            "noloc",
            &[("site.yaml", "base_url: https://ex.com\n")],
            "need `locales:` in site.yaml",
        ),
        (
            "unknownroute",
            &[(
                "routes.yaml",
                "routes:\n  - path: /index.html\n    template: home.html.j2\n    locales: [fr]\n",
            )],
            "locales.unknown",
        ),
        (
            "localizednoforeach",
            &[(
                "routes.yaml",
                "routes:\n  - path: /index.html\n    template: home.html.j2\n    localized: [name]\n",
            )],
            "needs `foreach` or `paginate`",
        ),
        (
            "nonemap",
            &[(
                "routes.yaml",
                "routes:\n  - path: {es: /a.html}\n    template: home.html.j2\n    locales: none\n",
            )],
            "one fixed path",
        ),
        (
            "badword",
            &[(
                "routes.yaml",
                "routes:\n  - path: /a.html\n    template: home.html.j2\n    locales: all\n",
            )],
            "`none`",
        ),
        ("notrdefault", &[("translations/es.yaml", "-")], "translations/es.yaml is missing"),
        ("trtag", &[("translations/fr.yaml", "a: b\n")], "translations/fr.yaml"),
        ("trleaf", &[("translations/es.yaml", "a: 3\n")], "must be a string"),
        (
            "trcollide",
            &[("translations/es.yaml", "a.b: x\na:\n  b: y\n")],
            "defined twice",
        ),
        ("trempty", &[("translations/es.yaml", "a: \"\"\n")], "is empty"),
    ];
    for (name, files, want) in cases {
        let dir = site(name, files);
        let msg = load_err(&dir);
        assert!(msg.contains(want), "{name}: expected {want:?} in: {msg}");
        let _ = fs::remove_dir_all(&dir);
    }
}

/// A locale without CLDR formatting data: the site loads, builds and passes
/// until `date_format` / `num_format` actually formats in that locale.
#[test]
fn a_locale_without_data_fails_only_where_a_filter_formats_in_it() {
    const SITE_TLH: &str = "base_url: https://ex.com\nlocales: [en, tlh]\n";
    const ROUTES_TLH: &str = "routes:\n  - path: /index.html\n    template: home.html.j2\n    \
                             title: \"{{ t('home.title') }}\"\n";
    let home = |body: &str| {
        format!(
            "<!doctype html><html lang=\"{{{{ locale }}}}\"><head><meta charset=\"utf-8\">\
             {{{{ std_head }}}}</head><body>{body}</body></html>"
        )
    };
    let files = |name: &str, body: &str| {
        site(
            name,
            &[
                ("site.yaml", SITE_TLH),
                ("routes.yaml", ROUTES_TLH),
                ("templates/home.html.j2", &home(body)),
                ("templates/book.html.j2", "-"),
                ("translations/es.yaml", "-"),
                ("translations/de.yaml", "-"),
                ("translations/en.yaml", "home:\n  title: Home\n"),
                ("translations/tlh.yaml", "home:\n  title: ホーム\n"),
            ],
        )
    };

    // (a) no filter: builds and checks green
    let dir = files("tlh-plain", "<p>{{ t('home.title') }}</p>");
    let report = check(&dir);
    assert_eq!(report["verdict"], "pass", "{report:#}");
    let tlh = read(&dir, &report, "tlh/index.html");
    assert!(
        tlh.contains("lang=\"tlh\"") && tlh.contains("ホーム"),
        "{tlh}"
    );
    let _ = fs::remove_dir_all(&dir);

    // (b) a filter on a tlh page is a render error naming the locale, page and template
    for filter in ["\"2026-03-05\" | date_format", "\"0.5\" | num_format"] {
        let dir = files("tlh-filter", &format!("<p>{{{{ {filter} }}}}</p>"));
        let msg = source_error(&dir);
        let name = filter.rsplit(' ').next().unwrap();
        assert!(
            msg.contains(&format!("{name}: locale 'tlh' has no formatting data"))
                && msg.contains("molde bundles CLDR 48.0.0")
                && msg.contains("(page /tlh/)")
                && msg.contains("home.html.j2"),
            "{msg}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    // (c) an explicit locale= with a table works on the tlh page; one without fails
    let dir = files(
        "tlh-override",
        "<p>{{ \"2026-03-05\" | date_format(locale=\"en\") }}</p>\
         <p>{{ \"0.5\" | num_format(locale=\"de\") }}</p>",
    );
    let report = check(&dir);
    assert_eq!(report["verdict"], "pass", "{report:#}");
    let tlh = read(&dir, &report, "tlh/index.html");
    assert!(tlh.contains("<p>March 5, 2026</p><p>0,5</p>"), "{tlh}");
    let _ = fs::remove_dir_all(&dir);
    let dir = files(
        "tlh-override-bad",
        "<p>{{ \"2026-03-05\" | date_format(locale=\"tlh\") }}</p>",
    );
    let msg = source_error(&dir);
    assert!(
        msg.contains("date_format: locale 'tlh' has no formatting data")
            && msg.contains("(page /)"),
        "{msg}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn bad_localized_values_fail_at_expansion() {
    let data = DATA.replace(
        "\"name\": {\"es\": \"Emma\", \"en\": \"Emma\", \"de\": null}",
        "\"name\": \"Emma\"",
    );
    let dir = site("shape", &[("data.json", &data)]);
    let s = molde::testing::load_site(&dir).unwrap();
    let msg = format!("{:#}", molde::testing::expand_routes(&s).err().unwrap());
    assert!(
        msg.contains("'name'") && msg.contains("object keyed by locale"),
        "{msg}"
    );
    let data = DATA.replace("\"de\": \"don-quixote\"", "\"fr\": \"don-quixote\"");
    let dir = site("key", &[("data.json", &data)]);
    let s = molde::testing::load_site(&dir).unwrap();
    let msg = format!("{:#}", molde::testing::expand_routes(&s).err().unwrap());
    assert!(msg.contains("\"fr\""), "{msg}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn noindex_members_leave_the_hreflang_group() {
    let routes = ROUTES.replace(
        "    localized: [slug, name]\n",
        "    localized: [slug, name]\n    index: \"{{ locale != 'de' }}\"\n",
    );
    let dir = site("noindex", &[("routes.yaml", &routes)]);
    let report = check(&dir);
    assert_eq!(report["verdict"], "pass", "{report:#}");
    let es = read(&dir, &report, "libros/don-quixote/index.html");
    assert!(
        es.contains("hreflang=\"en\"") && !es.contains("hreflang=\"de\""),
        "{es}"
    );
    let de = read(&dir, &report, "de/buecher/don-quixote/index.html");
    assert!(!de.contains("hreflang"), "{de}");
    // a language switcher still reaches every version, noindex ones included
    let s = molde::testing::load_site(&dir).unwrap();
    let pages = molde::testing::expand_routes(&s).unwrap();
    let de = pages
        .iter()
        .find(|p| p.path == "/de/buecher/don-quixote/index.html")
        .unwrap();
    let versions: Vec<&str> = de.alternates.iter().map(|a| a.locale.as_str()).collect();
    assert_eq!(versions, ["es", "en", "de"]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_item_exists_in_a_locale_only_with_every_declared_field() {
    // Emma gets a de slug but keeps a null de name: no de page, and no de list entry,
    // instead of a page whose `| loc` title aborts the build
    let data = DATA.replace(
        r#"{"es": "emma", "en": "emma", "de": null}"#,
        r#"{"es": "emma", "en": "emma", "de": "emma"}"#,
    );
    let routes = format!(
        "{ROUTES}  - path: /list.html\n    template: list.html.j2\n    title: \"{{{{ t('home.title') }}}} list\"\n    localized: [slug, name]\n    paginate:\n      over: books\n      per_page: 10\n      path: /list/{{n}}/index.html\n"
    );
    let list = "<html lang=\"{{ locale }}\"><head>{{ std_head }}</head><body>\
{% for s in pagination.items %}{{ s.isbn }},{% endfor %}</body></html>";
    let dir = site(
        "exists",
        &[
            ("data.json", &data),
            ("routes.yaml", &routes),
            ("templates/list.html.j2", list),
        ],
    );
    let report = check(&dir);
    assert_eq!(report["verdict"], "pass", "{report:#}");
    let build = PathBuf::from(report["build"]["dir"].as_str().unwrap());
    assert!(!build.join("de/buecher/emma/index.html").exists());
    assert!(read(&dir, &report, "de/list.html").contains("<body>B1,</body>"));
    // a declared field missing from an item is a load error, not a silent drop
    let data = DATA.replace(
        r#""name": {"es": "Emma", "en": "Emma", "de": null}"#,
        r#""x": 1"#,
    );
    let dir2 = site("exists-missing", &[("data.json", &data)]);
    let msg = source_error(&dir2);
    assert!(msg.contains("no localized field 'name'"), "{msg}");
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&dir2);
}

#[test]
fn t_arguments_are_filled_in_one_pass() {
    let who = "who: \"{a} and {b}\"\n";
    let home = HOME.replace("<body>", "<body><i>{{ t('who', a='{b}', b='X') }}</i>");
    let dir = site(
        "one-pass",
        &[
            ("templates/home.html.j2", &home),
            (
                "translations/es.yaml",
                &format!("home:\n  title: Inicio\ngreet: \"Hola, {{name}}\"\n{who}"),
            ),
            (
                "translations/en.yaml",
                &format!("home:\n  title: Home\ngreet: \"Hello, {{name}}\"\n{who}"),
            ),
            (
                "translations/de.yaml",
                &format!("home.title: Startseite\ngreet: \"Hallo, {{name}}\"\n{who}"),
            ),
        ],
    );
    let report = check(&dir);
    assert!(read(&dir, &report, "index.html").contains("<i>{b} and X</i>"));
    let _ = fs::remove_dir_all(&dir);
}

/// A site whose three locales share `pool: "{a} and {b}"` (plus the default keys),
/// with `body` inserted at the top of the home page.
fn pool_site(name: &str, body: &str) -> PathBuf {
    let pool = "pool: \"{a} and {b}\"\n";
    let home = HOME.replace("<body>", &format!("<body>{body}"));
    site(
        name,
        &[
            ("templates/home.html.j2", &home),
            (
                "translations/es.yaml",
                &format!("home:\n  title: Inicio\ngreet: \"Hola, {{name}}\"\n{pool}"),
            ),
            (
                "translations/en.yaml",
                &format!("home:\n  title: Home\ngreet: \"Hello, {{name}}\"\n{pool}"),
            ),
            (
                "translations/de.yaml",
                &format!("home.title: Startseite\ngreet: \"Hallo, {{name}}\"\n{pool}"),
            ),
        ],
    )
}

fn render_err(dir: &Path) -> String {
    source_error(dir)
}

/// The message of the one source error a failing check reports.
fn source_error(dir: &Path) -> String {
    let report = molde::run_check(dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "fail", "{report}");
    assert_eq!(report["errors"][0]["layer"], "source", "{report}");
    report["errors"][0]["message"].as_str().unwrap().to_string()
}

#[test]
fn t_args_is_a_bag_whose_extra_entries_are_ignored() {
    let dir = pool_site(
        "args-bag",
        "<i>{{ t('pool', args={'a': 'A', 'b': 'B', 'unused': 'U'}) }}</i>",
    );
    let report = check(&dir);
    assert!(read(&dir, &report, "index.html").contains("<i>A and B</i>"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn t_args_missing_placeholder_is_a_render_error_naming_the_key() {
    let dir = pool_site("args-missing", "<i>{{ t('pool', args={'a': 'A'}) }}</i>");
    let msg = render_err(&dir);
    assert!(msg.contains("t('pool')") && msg.contains("{b}"), "{msg}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn t_kwarg_beats_args_and_stays_strict() {
    let dir = pool_site(
        "args-clash",
        "<i>{{ t('pool', b='K', args={'a': 'A', 'b': 'B'}) }}</i>",
    );
    let report = check(&dir);
    assert!(read(&dir, &report, "index.html").contains("<i>A and K</i>"));
    let _ = fs::remove_dir_all(&dir);
    // an explicit kwarg the string doesn't name is still an error
    let dir = pool_site(
        "args-strict",
        "<i>{{ t('pool', typo='X', args={'a': 'A', 'b': 'B'}) }}</i>",
    );
    let msg = render_err(&dir);
    assert!(msg.contains("typo"), "{msg}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn t_args_that_is_not_a_mapping_is_a_render_error() {
    let dir = pool_site("args-type", "<i>{{ t('pool', args=['A', 'B']) }}</i>");
    let msg = render_err(&dir);
    assert!(
        msg.contains("t('pool')") && msg.contains("`args=` needs a mapping"),
        "{msg}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn t_html_escapes_args_values() {
    let dir = pool_site(
        "args-html",
        "<i>{{ t_html('pool', args={'a': '<b>', 'b': '\"q\"'}) }}</i>",
    );
    let report = check(&dir);
    let home = read(&dir, &report, "index.html");
    assert!(
        home.contains("<i>&lt;b&gt; and &quot;q&quot;</i>"),
        "{home}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn mcp_tools_show_the_locale_dimension_only_under_locales() {
    let call = |site: PathBuf, name: &str, args: Value| {
        let s = molde::mcp::McpServer {
            out: site.join("out"),
            site,
        };
        let resp = s
            .handle(&serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": { "name": name, "arguments": args },
            }))
            .unwrap();
        assert_eq!(resp["result"]["isError"], false, "{resp}");
        serde_json::from_str::<Value>(resp["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap()
    };
    let dir = site("mcp", &[]);
    let routes = call(dir.clone(), "list_routes", serde_json::json!({}));
    let book = &routes["routes"][1];
    assert_eq!(book["locales"], serde_json::json!(["es", "en", "de"]));
    assert_eq!(book["localized"], serde_json::json!(["slug", "name"]));
    assert_eq!(
        book["pages_by_locale"],
        serde_json::json!({"de": 1, "en": 2, "es": 2})
    );
    assert_eq!(book["path"]["en"], "/books/{slug}/index.html");

    let page = call(
        dir.clone(),
        "explain_route",
        serde_json::json!({"path": "/en/books/emma/"}),
    );
    assert_eq!(page["locale"], "en");
    assert_eq!(
        page["localized_fields"],
        serde_json::json!(["slug", "name"])
    );
    assert_eq!(
        page["alternates"],
        serde_json::json!([
            {"locale": "es", "path": "/libros/emma/index.html", "index": true},
            {"locale": "en", "path": "/en/books/emma/index.html", "index": true},
        ])
    );

    let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/demo");
    let routes = call(demo, "list_routes", serde_json::json!({}));
    assert!(routes["routes"][0].get("locales").is_none(), "{routes}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn locale_versions_of_an_item_share_one_copy_of_its_data() {
    // a site in many locales must not hold one copy of every item per locale
    let dir = site("shared-items", &[]);
    let s = molde::testing::load_site(&dir).unwrap();
    let pages = molde::testing::expand_routes(&s).unwrap();
    let with_items: Vec<_> = pages.iter().filter(|p| p.item.is_some()).collect();
    let mut groups = 0;
    for p in &with_items {
        let siblings: Vec<_> = with_items
            .iter()
            .filter(|q| q.item.as_deref() == p.item.as_deref())
            .collect();
        if siblings.len() > 1 {
            groups += 1;
        }
        for q in siblings {
            assert!(
                std::sync::Arc::ptr_eq(p.item.as_ref().unwrap(), q.item.as_ref().unwrap()),
                "{} and {} hold separate copies of the same item",
                p.path,
                q.path
            );
        }
    }
    assert!(
        groups > 0,
        "the test site has no item in more than one locale"
    );
    let _ = fs::remove_dir_all(&dir);
}
