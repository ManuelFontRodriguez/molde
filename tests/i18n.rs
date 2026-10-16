//! The `locale` capability through the real pipeline: site.yaml key on -> filters
//! exist in templates; key off -> only `year`; a locale without CLDR data -> loud
//! failure where a filter formats in it, and only there.
use std::fs;
use std::path::{Path, PathBuf};

fn site(name: &str, site_yaml: &str, template: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("molde-i18n-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: p.html.j2\n",
    )
    .unwrap();
    fs::write(
        dir.join("data.json"),
        r#"{"d": "2026-11-03", "amount": "0.0850", "pct": "3.90"}"#,
    )
    .unwrap();
    fs::write(dir.join("site.yaml"), site_yaml).unwrap();
    fs::write(dir.join("templates").join("p.html.j2"), template).unwrap();
    dir
}

fn render(dir: &Path) -> String {
    let s = molde::testing::load_site(dir).unwrap();
    let env = molde::testing::engine(&s).unwrap();
    let pages = molde::testing::expand_routes(&s).unwrap();
    molde::testing::render_page(&env, &s, &pages[0]).unwrap()
}

#[test]
fn locale_es_formats_dates_and_numbers() {
    let dir = site(
        "es",
        "locale: es-ES\n",
        "{{ data.d | date_format }}|{{ data.d | date_format(style=\"medium\") }}|\
         {{ data.d | date_format(style=\"short\") }}|{{ data.amount | num_format }}|\
         {{ data.d | year }}|{{ \"N/D\" | date_format }}",
    );
    // autoescape (on for .html.j2) leaves "/" alone: dates read as written
    assert_eq!(
        render(&dir),
        "3 de noviembre de 2026|3 nov 2026|3/11/26|0,0850|2026|N/D"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn year_works_without_locale() {
    let dir = site("nolocale", "", "{{ data.d | year }}");
    assert_eq!(render(&dir), "2026");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn no_locale_means_no_format_filters() {
    let dir = site("off", "", "{{ data.d | date_format }}");
    let s = molde::testing::load_site(&dir).unwrap();
    let env = molde::testing::engine(&s).unwrap();
    let pages = molde::testing::expand_routes(&s).unwrap();
    let err = molde::testing::render_page(&env, &s, &pages[0]).unwrap_err();
    assert!(
        format!("{err:#}").contains("date_format"),
        "expected unknown-filter error, got: {err:#}"
    );
    let _ = fs::remove_dir_all(&dir);
}

fn render_err(dir: &Path) -> String {
    let s = molde::testing::load_site(dir).unwrap();
    let env = molde::testing::engine(&s).unwrap();
    let pages = molde::testing::expand_routes(&s).unwrap();
    format!(
        "{:#}",
        molde::testing::render_page(&env, &s, &pages[0]).unwrap_err()
    )
}

#[test]
fn locale_without_cldr_data_fails_only_when_a_filter_renders() {
    // the engine builds and a page without the filters renders
    let dir = site("tlh", "locale: tlh\n", "{{ data.d | year }}");
    assert_eq!(render(&dir), "2026");
    let _ = fs::remove_dir_all(&dir);
    for (i, tmpl) in [
        "{{ data.d | date_format }}",
        "{{ data.amount | num_format }}",
    ]
    .iter()
    .enumerate()
    {
        let dir = site(&format!("tlh{i}"), "locale: tlh\n", tmpl);
        let msg = render_err(&dir);
        let filter = if i == 0 { "date_format" } else { "num_format" };
        assert!(
            msg.contains(filter)
                && msg.contains("'tlh' has no formatting data")
                && msg.contains("CLDR 48.0.0")
                && msg.contains("p.html.j2"),
            "error must name the filter, the locale, the data and the template: {msg}"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn any_cldr_locale_formats() {
    // ja-JP had no table before CLDR; now every locale with basic coverage formats
    let dir = site(
        "ja",
        "locale: ja-JP\n",
        "{{ data.d | date_format(style=\"full\") }}|{{ data.d | date_format(style=\"month\") }}|\
         {{ \"1234.5\" | num_format(decimals=2) }}",
    );
    assert_eq!(render(&dir), "2026年11月3日火曜日|2026年11月|1,234.50");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unknown_style_errors_loudly() {
    let dir = site(
        "style",
        "locale: es-ES\n",
        "{{ data.d | date_format(style=\"huge\") }}",
    );
    let msg = render_err(&dir);
    assert!(
        msg.contains("style 'huge' unknown") && msg.contains("full, long, medium, short, month"),
        "error must list valid styles: {msg}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn date_styles_and_patterns() {
    let dir = site(
        "styles",
        "locale: es-ES\n",
        "{{ data.d | date_format(style=\"full\") }}|{{ data.d | date_format(style=\"month\") }}|\
         {{ data.d | date_format(pattern=\"EEEE, dd MMMM yyyy\") }}|\
         {{ data.d | date_format(pattern=\"MMMM\") }}|{{ data.d | date_format(pattern=\"LLLL\") }}|\
         {{ data.d | date_format(pattern=\"d 'de' MMMM\", locale=\"en\") }}|\
         {{ \"2026-02-30\" | date_format }}",
    );
    assert_eq!(
        render(&dir),
        "martes, 3 de noviembre de 2026|noviembre de 2026|martes, 03 noviembre 2026|\
         noviembre|noviembre|3 de November|2026-02-30"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn date_format_argument_errors() {
    for (name, tmpl, want) in [
        (
            "both",
            "{{ data.d | date_format(style=\"long\", pattern=\"MMMM\") }}",
            "pass `style` or `pattern`, not both",
        ),
        (
            "field",
            "{{ data.d | date_format(pattern=\"HH:mm\") }}",
            "date_format",
        ),
        (
            "kwarg",
            "{{ data.d | date_format(format=\"long\") }}",
            "format",
        ),
    ] {
        let dir = site(&format!("dateerr-{name}"), "locale: en\n", tmpl);
        let msg = render_err(&dir);
        assert!(msg.contains(want), "{name}: expected {want:?} in: {msg}");
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn num_format_decimals_and_grouping() {
    let dir = site(
        "nums",
        "locale: de-DE\n",
        "{{ data.amount | num_format }}|{{ \"1234567.5\" | num_format }}|\
         {{ \"1234.5\" | num_format(decimals=2) }}|{{ \"0.125\" | num_format(decimals=2) }}|\
         {{ \"1234.5\" | num_format(decimals=2, grouping=false) }}|\
         {{ \"1234567.5\" | num_format(grouping=true) }}|{{ 1234.5 | num_format(decimals=1) }}|\
         {{ 42 | num_format }}|{{ \"-3.90\" | num_format }}|{{ \"N/D\" | num_format(decimals=2) }}|\
         {{ \"1234.5\" | num_format(decimals=2, locale=\"fr\") }}",
    );
    assert_eq!(
        render(&dir),
        "0,0850|1234567,5|1.234,50|0,12|1234,50|1.234.567,5|1.234,5|42|-3,90|N/D|1\u{202F}234,50"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn num_format_argument_errors() {
    for (name, tmpl, want) in [
        (
            "big",
            "{{ \"1\" | num_format(decimals=101) }}",
            "decimals must be 0-100",
        ),
        (
            "type",
            "{{ [1] | num_format }}",
            "needs a decimal string or a number",
        ),
        ("kwarg", "{{ \"1\" | num_format(digits=2) }}", "digits"),
    ] {
        let dir = site(&format!("numerr-{name}"), "locale: en\n", tmpl);
        let msg = render_err(&dir);
        assert!(msg.contains(want), "{name}: expected {want:?} in: {msg}");
        let _ = fs::remove_dir_all(&dir);
    }
}
