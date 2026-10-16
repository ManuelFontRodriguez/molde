//! Template and source errors come back as a failing JSON report with
//! fields, not as bare text; only the tool itself failing is an Err.
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn site(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("molde-srcerr-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: i.j2\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    fs::write(
        dir.join("templates/i.j2"),
        "<html><head><title>t</title></head><body>x</body></html>",
    )
    .unwrap();
    dir
}

/// The single error of a failing, unpublished report with no build.
fn only_error(report: &Value) -> &Value {
    assert_eq!(report["verdict"], "fail", "{report}");
    assert_eq!(report["published"], false);
    assert!(report["build"].is_null());
    let errors = report["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "{report}");
    assert_eq!(errors[0]["layer"], "source");
    assert!(errors[0]["hint"].is_string(), "{report}");
    &errors[0]
}

#[test]
fn template_syntax_error_is_a_report_entry() {
    let dir = site("syntax");
    fs::write(dir.join("templates/i.j2"), "<html>\n{% for %}\n</html>").unwrap();
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    let e = only_error(&report);
    assert_eq!(e["check"], "template.syntax");
    assert_eq!(e["template"], "i.j2");
    assert_eq!(e["line"], 2);
    assert_eq!(e["page"], "index.html");
    assert_eq!(e["route"], "/index.html");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn template_render_error_names_the_page() {
    let dir = site("render");
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /s/{t}/index.html\n    template: i.j2\n    foreach: books\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), r#"{"books":[{"t":"B1"}]}"#).unwrap();
    fs::write(
        dir.join("templates/i.j2"),
        "<html>\n\n{{ item.t.x.y }}</html>",
    )
    .unwrap();
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    let e = only_error(&report);
    assert_eq!(e["check"], "template.render");
    assert_eq!(e["template"], "i.j2");
    assert_eq!(e["line"], 3);
    assert_eq!(e["page"], "s/B1/index.html");
    assert_eq!(e["route"], "/s/{t}/index.html");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unparsable_site_files_are_config_parse_with_a_line() {
    let dir = site("parse");
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n   template: i.j2\n",
    )
    .unwrap();
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    let e = only_error(&report);
    assert_eq!(e["check"], "config.parse");
    assert_eq!(e["file"], "routes.yaml");
    assert!(e["line"].is_u64(), "{e}");

    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: i.j2\n",
    )
    .unwrap();
    fs::write(dir.join("site.yaml"), "base_url: [\n").unwrap();
    let e = only_error(&molde::run_check(&dir, &dir.join("out")).unwrap()).clone();
    assert_eq!(e["file"], "site.yaml");

    fs::remove_file(dir.join("site.yaml")).unwrap();
    fs::write(dir.join("data.json"), "{\n  \"a\": ,\n}").unwrap();
    let e = only_error(&molde::run_check(&dir, &dir.join("out")).unwrap()).clone();
    assert_eq!(e["file"], "data.json");
    assert_eq!(e["line"], 2);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn router_rejections_are_config_invalid() {
    let dir = site("invalid");
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /s/{t}/index.html\n    template: i.j2\n    foreach: missing\n",
    )
    .unwrap();
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    let e = only_error(&report);
    assert_eq!(e["check"], "config.invalid");
    assert!(e["message"].as_str().unwrap().contains("missing"), "{e}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_tool_failing_is_still_an_error() {
    // an out dir that cannot be created is not a site problem
    let dir = site("io");
    fs::write(dir.join("out"), "a file where the out dir should be").unwrap();
    assert!(molde::run_check(&dir, &dir.join("out")).is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_first_failing_page_is_reported_whatever_the_thread_timing() {
    // pages render on every core: with several broken pages the report must still
    // name the first one in page order, every time
    let dir = site("parallel");
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /s/{t}/index.html\n    template: i.j2\n    foreach: items\n",
    )
    .unwrap();
    let items: Vec<_> = (0..400)
        .map(|n| serde_json::json!({ "t": format!("p{n}"), "fail": n == 137 || n == 211 || n == 390 }))
        .collect();
    fs::write(
        dir.join("data.json"),
        serde_json::json!({ "items": items }).to_string(),
    )
    .unwrap();
    fs::write(
        dir.join("templates/i.j2"),
        "<html><head><title>{{ item.t }}</title></head><body>\
         {% if item.fail %}{{ nope.x }}{% endif %}</body></html>",
    )
    .unwrap();
    for _ in 0..10 {
        let report = molde::run_check(&dir, &dir.join("out")).unwrap();
        let e = only_error(&report);
        assert_eq!(e["check"], "template.render");
        assert_eq!(e["page"], "s/p137/index.html");
    }
    // and no build, finished or half-written (`.tmp-*`), was left behind
    let leftovers: Vec<_> = fs::read_dir(dir.join("out/builds"))
        .map(|d| d.filter_map(|e| e.ok()).map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_passing_parallel_build_writes_every_page_with_its_gz() {
    let dir = site("parallel-ok");
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /s/{t}/index.html\n    template: i.j2\n    foreach: items\n",
    )
    .unwrap();
    let items: Vec<_> = (0..500)
        .map(|n| serde_json::json!({ "t": format!("p{n}") }))
        .collect();
    fs::write(
        dir.join("data.json"),
        serde_json::json!({ "items": items }).to_string(),
    )
    .unwrap();
    fs::write(
        dir.join("templates/i.j2"),
        "<html><head><title>{{ item.t }}</title></head><body>{{ item.t }}</body></html>",
    )
    .unwrap();
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "pass", "{report}");
    let current = dir.join("out/current");
    for n in 0..500 {
        let page = current.join(format!("s/p{n}/index.html"));
        let html = fs::read_to_string(&page).unwrap();
        assert!(
            html.contains(&format!("<body>p{n}</body>")),
            "{}",
            page.display()
        );
        let mut gz = String::new();
        std::io::Read::read_to_string(
            &mut flate2::read::GzDecoder::new(
                fs::File::open(page.with_extension("html.gz")).unwrap(),
            ),
            &mut gz,
        )
        .unwrap();
        assert_eq!(gz, html);
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_reports_are_identical_run_after_run() {
    // checks read pages on every core: the report (errors, their order, and which
    // page a duplicate points at) must not depend on thread timing
    let dir = site("parallel-report");
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /s/{t}/index.html\n    template: i.j2\n    foreach: items\n",
    )
    .unwrap();
    let items: Vec<_> = (0..300)
        .map(|n| serde_json::json!({ "t": format!("p{n:03}"), "group": n % 7 }))
        .collect();
    fs::write(
        dir.join("data.json"),
        serde_json::json!({ "items": items }).to_string(),
    )
    .unwrap();
    // seven shared titles (titles.duplicate) and a broken link on every page
    fs::write(
        dir.join("templates/i.j2"),
        "<html><head><title>group {{ item.group }}</title></head>\
         <body><a href=\"/missing/{{ item.t }}/\">x</a></body></html>",
    )
    .unwrap();
    let first = molde::run_check(&dir, &dir.join("out")).unwrap();
    let errors = first["errors"].as_array().unwrap();
    assert_eq!(
        errors
            .iter()
            .filter(|e| e["check"] == "links.internal")
            .count(),
        300
    );
    let dups: Vec<_> = errors
        .iter()
        .filter(|e| e["check"] == "titles.duplicate")
        .collect();
    assert_eq!(dups.len(), 300 - 7);
    // the first page of each group (in page order) is the one every duplicate names
    for d in &dups {
        let page = d["page"].as_str().unwrap();
        let other = d["duplicate_of"].as_str().unwrap();
        assert!(other < page, "{other} should come before {page}");
    }
    for _ in 0..5 {
        let again = molde::run_check(&dir, &dir.join("out")).unwrap();
        assert_eq!(again["errors"], first["errors"]);
    }
    let _ = fs::remove_dir_all(&dir);
}
