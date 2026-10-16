//! paths.duplicate: two routes expanding to the same output path
//! must refuse to build — the later write silently replaced the earlier page
//! while the report claimed both.
use std::fs;
use std::path::PathBuf;

fn colliding_site(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("molde-routes-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    // the exact repro: two foreach routes over the same pattern,
    // collections that share a slug
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n\
         \x20 - path: /{slug}/index.html\n\
         \x20   template: t.html.j2\n\
         \x20   foreach: a\n\
         \x20 - path: /{slug}/index.html\n\
         \x20   template: t.html.j2\n\
         \x20   foreach: b\n",
    )
    .unwrap();
    fs::write(
        dir.join("data.json"),
        r#"{"a": [{"slug": "x"}], "b": [{"slug": "x"}, {"slug": "only-b"}]}"#,
    )
    .unwrap();
    fs::write(
        dir.join("templates/t.html.j2"),
        "<html><head><title>{{ item.slug }}</title></head><body>ok</body></html>",
    )
    .unwrap();
    dir
}

#[test]
fn check_reports_duplicate_paths_and_publishes_nothing() {
    let dir = colliding_site("check");
    let out = dir.join("out");

    let report = molde::run_check(&dir, &out).unwrap();
    assert_eq!(report["verdict"], "fail", "{report}");
    assert_eq!(report["published"], false);
    assert!(report["build"].is_null(), "nothing was rendered");
    assert!(
        !out.join("current").exists(),
        "a colliding site must never publish"
    );

    let errors = report["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "one error per colliding path: {report}");
    let e = &errors[0];
    assert_eq!(e["check"], "paths.duplicate");
    assert_eq!(e["layer"], "routes");
    assert_eq!(e["page"], "x/index.html");
    // both offending routes named, with their collections — the fixable source
    assert_eq!(e["routes"][0]["foreach"], "a");
    assert_eq!(e["routes"][1]["foreach"], "b");
    assert_eq!(e["pages_colliding"], 2);
    // generic consumers key on route/template like every other check
    assert_eq!(e["route"], "/{slug}/index.html");
    assert!(e["template"].is_string());
    assert!(e["hint"].is_string());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn slash_and_case_variants_still_collide() {
    // write_page trims the leading slash, and macOS/Windows dev filesystems are
    // case-insensitive — both variants hit the same file on disk
    let dir = colliding_site("norm");
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n\
         \x20 - path: /about/index.html\n\
         \x20   template: t.html.j2\n\
         \x20 - path: about/index.html\n\
         \x20   template: t.html.j2\n\
         \x20 - path: /{slug}/index.html\n\
         \x20   template: t.html.j2\n\
         \x20   foreach: a\n\
         \x20 - path: /{slug}/index.html\n\
         \x20   template: t.html.j2\n\
         \x20   foreach: b\n",
    )
    .unwrap();
    fs::write(
        dir.join("data.json"),
        r#"{"a": [{"slug": "Dune"}], "b": [{"slug": "dune"}]}"#,
    )
    .unwrap();
    fs::write(
        dir.join("templates/t.html.j2"),
        "<html><head><title>t</title></head><body>ok</body></html>",
    )
    .unwrap();

    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "fail", "{report}");
    let checks: Vec<&str> = report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["check"].as_str().unwrap())
        .collect();
    assert_eq!(
        checks,
        ["paths.duplicate", "paths.duplicate"],
        "slash variant AND case variant each collide: {report}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn n_way_collision_is_one_error() {
    let dir = colliding_site("nway");
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n\
         \x20 - path: /{slug}/index.html\n\
         \x20   template: t.html.j2\n\
         \x20   foreach: a\n",
    )
    .unwrap();
    fs::write(
        dir.join("data.json"),
        r#"{"a": [{"slug": "x"}, {"slug": "x"}, {"slug": "x"}]}"#,
    )
    .unwrap();

    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    let errors = report["errors"].as_array().unwrap();
    assert_eq!(
        errors.len(),
        1,
        "one error per path, not per pair: {report}"
    );
    assert_eq!(errors[0]["pages_colliding"], 3);
    // same route repeated is ONE offender entry, not three
    assert_eq!(errors[0]["routes"].as_array().unwrap().len(), 1);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sitemap_route_is_reserved_under_web_standards() {
    let dir = colliding_site("reserved");
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n\
         \x20 - path: /index.html\n\
         \x20   template: t.html.j2\n\
         \x20 - path: /sitemap.xml\n\
         \x20   template: t.html.j2\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    // static routes: no `item` in scope, so the fixture's item-based template won't do
    fs::write(
        dir.join("templates/t.html.j2"),
        "<html><head><title>t</title></head><body>ok</body></html>",
    )
    .unwrap();
    fs::write(dir.join("site.yaml"), "base_url: https://example.com\n").unwrap();

    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "fail", "{report}");
    let e = &report["errors"][0];
    assert_eq!(e["check"], "paths.reserved");
    assert_eq!(e["page"], "sitemap.xml");
    assert!(e["hint"].is_string());

    // without base_url the framework emits no sitemap — the route is legitimate
    fs::remove_file(dir.join("site.yaml")).unwrap();
    let report = molde::run_check(&dir, &dir.join("out2")).unwrap();
    assert!(
        report["errors"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["check"] != "paths.reserved"),
        "{report}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn build_refuses_duplicate_paths() {
    let dir = colliding_site("build");
    let err = molde::build(&dir, &dir.join("out")).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("duplicate output paths"), "{msg}");
    assert!(
        msg.contains("/x/index.html"),
        "names the colliding page: {msg}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn duplicate_slugs_within_one_foreach_also_collide() {
    let dir = colliding_site("same-route");
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n\
         \x20 - path: /{slug}/index.html\n\
         \x20   template: t.html.j2\n\
         \x20   foreach: a\n",
    )
    .unwrap();
    fs::write(
        dir.join("data.json"),
        r#"{"a": [{"slug": "x"}, {"slug": "x"}]}"#,
    )
    .unwrap();

    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "fail", "{report}");
    assert_eq!(report["errors"][0]["check"], "paths.duplicate");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_page_file_where_another_page_needs_a_folder_is_refused() {
    // `/a` written as a file and `/a/b.html` inside a folder `a`: only one can exist.
    // Pages render in parallel, so this is refused up front, the same way every run.
    let dir = std::env::temp_dir().join(format!("molde-routes-filefolder-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /docs\n    template: t.j2\n  - path: /docs/intro.html\n    template: t.j2\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    fs::write(
        dir.join("templates/t.j2"),
        "<html><head><title>t</title></head><body>x</body></html>",
    )
    .unwrap();
    for _ in 0..3 {
        let report = molde::run_check(&dir, &dir.join("out")).unwrap();
        assert_eq!(report["verdict"], "fail");
        assert!(report["build"].is_null());
        let e = &report["errors"][0];
        assert_eq!(e["check"], "paths.file_and_folder", "{report}");
        assert_eq!(e["page"], "docs");
        assert_eq!(e["inside"], "docs/intro.html");
        assert!(e["hint"].is_string());
    }
    let err = molde::build(&dir, &dir.join("out2")).unwrap_err();
    assert!(
        format!("{err:#}").contains("/docs is a file but /docs/intro.html needs it as a folder"),
        "{err:#}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn bad_paths_are_reported_when_the_site_loads() {
    // a path a URL cannot carry, or one naming a folder, used to fail late with bare
    // text (or slip into the sitemap); now `check` reports it, naming the route
    let dir = std::env::temp_dir().join(format!("molde-routes-badpath-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("templates/t.j2"),
        "<html><head><title>{% if item is defined %}{{ item.t }}{% else %}t{% endif %}</title></head><body>x</body></html>",
    )
    .unwrap();
    let route_cases = [
        ("/home/", "/home/index.html"),
        ("/q.html?x=1", "'?'"),
        ("/a b.html", "' '"),
        ("/a//b.html", "empty"),
    ];
    for (path, expect) in route_cases {
        fs::write(
            dir.join("routes.yaml"),
            format!("routes:\n  - path: {path:?}\n    template: t.j2\n"),
        )
        .unwrap();
        fs::write(dir.join("data.json"), "{}").unwrap();
        let report = molde::run_check(&dir, &dir.join("out")).unwrap();
        let e = &report["errors"][0];
        assert_eq!(e["check"], "config.invalid", "{path}: {report}");
        assert!(
            e["message"].as_str().unwrap().contains(expect),
            "{path}: {e}"
        );
    }
    // the same rule for a value filled in from data
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /s/{t}/index.html\n    template: t.j2\n    foreach: items\n",
    )
    .unwrap();
    for value in ["q?x", "h#x", "a b", ".", "nul\u{0}", &"x".repeat(300)] {
        fs::write(
            dir.join("data.json"),
            serde_json::json!({ "items": [{ "t": value }] }).to_string(),
        )
        .unwrap();
        let report = molde::run_check(&dir, &dir.join("out")).unwrap();
        assert_eq!(
            report["errors"][0]["check"], "config.invalid",
            "{value:?}: {report}"
        );
    }
    // a path written without its leading slash still works, as before
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: about.html\n    template: t.j2\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "pass", "{report}");
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /s/{t}/index.html\n    template: t.j2\n    foreach: items\n",
    )
    .unwrap();
    // letters in any script stay allowed
    fs::write(
        dir.join("data.json"),
        serde_json::json!({ "items": [{ "t": "café" }, { "t": "東京" }] }).to_string(),
    )
    .unwrap();
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "pass", "{report}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn invisible_characters_and_bad_paginate_paths_are_refused() {
    let dir = std::env::temp_dir().join(format!("molde-routes-invisible-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("templates/t.j2"),
        "<html><head><title>t</title></head><body>x</body></html>",
    )
    .unwrap();
    fs::write(
        dir.join("data.json"),
        serde_json::json!({ "items": [{ "t": "Dune\u{200B}" }], "rows": [1, 2, 3] }).to_string(),
    )
    .unwrap();
    // a zero-width space in a value from data, a right-to-left override in a route
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /s/{t}/index.html\n    template: t.j2\n    foreach: items\n",
    )
    .unwrap();
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["errors"][0]["check"], "config.invalid", "{report}");
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: \"/a/\\u202Egpj.html\"\n    template: t.j2\n",
    )
    .unwrap();
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["errors"][0]["check"], "config.invalid", "{report}");
    // paginate.path follows the same rules as a route path
    for bad in ["/p/{n}/", "/p /{n}.html", "/p/{n}%.html"] {
        fs::write(
            dir.join("routes.yaml"),
            format!("routes:\n  - path: /p/index.html\n    template: t.j2\n    paginate:\n      over: rows\n      per_page: 1\n      path: {bad:?}\n"),
        )
        .unwrap();
        let report = molde::run_check(&dir, &dir.join("out")).unwrap();
        assert_eq!(
            report["errors"][0]["check"], "config.invalid",
            "{bad}: {report}"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}
