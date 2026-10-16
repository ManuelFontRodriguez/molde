//! Route `index` field and non-HTML routes: noindex pages render but
//! stay out of the sitemap, carry a robots meta instead of canonical/OG, and a
//! 404 page can pass `molde check`.
use std::fs;
use std::path::{Path, PathBuf};

const HEAD_TEMPLATE: &str = "<html><head>\n{{ std_head }}\n</head><body>x</body></html>";

fn site(tag: &str, routes: &str, data: &str, templates: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("molde-index-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(dir.join("routes.yaml"), routes).unwrap();
    fs::write(dir.join("data.json"), data).unwrap();
    fs::write(dir.join("site.yaml"), "base_url: https://x.example\n").unwrap();
    for (file, content) in templates {
        fs::write(dir.join("templates").join(file), content).unwrap();
    }
    dir
}

fn check(dir: &Path) -> serde_json::Value {
    molde::run_check(dir, &dir.join("out")).unwrap()
}

fn checks_of(report: &serde_json::Value) -> Vec<String> {
    report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["check"].as_str().unwrap().to_string())
        .collect()
}

fn built(report: &serde_json::Value, rel: &str) -> String {
    let dir = PathBuf::from(report["build"]["dir"].as_str().unwrap());
    fs::read_to_string(dir.join(rel)).unwrap()
}

#[test]
fn routed_404_with_index_false_passes_check() {
    let dir = site(
        "404",
        "routes:\n\
         \x20 - path: /index.html\n    template: p.html.j2\n    title: Home\n\
         \x20 - path: /404.html\n    template: p.html.j2\n    title: Page not found\n    index: false\n",
        "{}",
        &[("p.html.j2", HEAD_TEMPLATE)],
    );
    let report = check(&dir);
    assert_eq!(report["verdict"], "pass", "{report}");

    let html = built(&report, "404.html");
    assert!(html.contains("<title>Page not found</title>"), "{html}");
    assert!(
        html.contains(r#"<meta name="robots" content="noindex">"#),
        "{html}"
    );
    assert!(
        !html.contains("canonical"),
        "noindex page claimed a canonical: {html}"
    );
    assert!(!html.contains("og:"), "noindex page emitted OG: {html}");

    let sitemap = built(&report, "sitemap.xml");
    assert!(
        sitemap.contains("<loc>https://x.example/</loc>"),
        "{sitemap}"
    );
    assert!(!sitemap.contains("404"), "noindex page listed: {sitemap}");

    // the indexable page is untouched
    let home = built(&report, "index.html");
    assert!(home.contains(r#"<link rel="canonical" href="https://x.example/">"#));
    assert!(!home.contains("robots"), "{home}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn index_pattern_decides_per_page() {
    let dir = site(
        "pattern",
        "routes:\n\
         \x20 - path: /c/{slug}/index.html\n    template: p.html.j2\n    foreach: books\n\
         \x20   title: \"{{ item.slug }}\"\n    index: \"{{ item.indexable }}\"\n",
        r#"{"books": [{"slug": "a", "indexable": true}, {"slug": "b", "indexable": false}]}"#,
        &[("p.html.j2", HEAD_TEMPLATE)],
    );
    let report = check(&dir);
    assert_eq!(report["verdict"], "pass", "{report}");
    let sitemap = built(&report, "sitemap.xml");
    assert!(sitemap.contains("https://x.example/c/a/"), "{sitemap}");
    assert!(!sitemap.contains("/c/b/"), "{sitemap}");
    assert!(built(&report, "c/b/index.html").contains(r#"content="noindex""#));
    assert!(built(&report, "c/a/index.html").contains("og:url"));

    // introspection sees the same decision the build made
    let s = molde::testing::load_site(&dir).unwrap();
    let pages = molde::testing::expand_routes(&s).unwrap();
    let flags: Vec<bool> = pages.iter().map(|p| p.index).collect();
    assert_eq!(flags, [true, false]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn index_pattern_rendering_anything_else_is_a_build_error() {
    let dir = site(
        "badpattern",
        "routes:\n\
         \x20 - path: /c/{slug}/index.html\n    template: p.html.j2\n    foreach: books\n\
         \x20   index: \"{{ item.indexible }}\"\n",
        r#"{"books": [{"slug": "a", "indexable": true}]}"#,
        &[("p.html.j2", HEAD_TEMPLATE)],
    );
    // the typo'd pattern fails the check as a source error, naming route and page
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "fail");
    assert_eq!(report["published"], false);
    assert_eq!(report["errors"][0]["check"], "config.invalid");
    let msg = report["errors"][0]["message"].as_str().unwrap().to_string();
    assert!(
        msg.contains("/c/{slug}/index.html"),
        "names the route: {msg}"
    );
    assert!(msg.contains("/c/a/index.html"), "names the page: {msg}");
    assert!(msg.contains("true") && msg.contains("false"), "{msg}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn noindex_page_without_meta_is_robots_missing_only() {
    // a template that forgot std_head: the only complaint is the missing robots
    // meta — not sitemap.missing_page, canonical.missing or og.missing
    let dir = site(
        "nometa",
        "routes:\n  - path: /404.html\n    template: p.html.j2\n    index: false\n",
        "{}",
        &[("p.html.j2", "<html><head><title>gone</title></head></html>")],
    );
    let report = check(&dir);
    assert_eq!(checks_of(&report), ["robots.missing"], "{report}");
    let e = &report["errors"][0];
    assert_eq!(e["page"], "404.html");
    assert_eq!(e["route"], "/404.html");
    assert!(e["hint"].as_str().unwrap().contains("std_head"), "{e}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn indexable_page_with_handwritten_noindex_is_contradiction() {
    let dir = site(
        "contra",
        "routes:\n  - path: /index.html\n    template: p.html.j2\n    title: Home\n",
        "{}",
        &[(
            "p.html.j2",
            "<html><head>{{ std_head }}<meta content=\"NOINDEX, follow\" name=\"robots\"></head></html>",
        )],
    );
    assert_eq!(checks_of(&check(&dir)), ["sitemap.noindex_listed"]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn static_html_passes_only_with_noindex_meta() {
    let dir = site(
        "static",
        "routes:\n  - path: /index.html\n    template: p.html.j2\n    title: Home\n",
        "{}",
        &[("p.html.j2", HEAD_TEMPLATE)],
    );
    fs::create_dir_all(dir.join("static")).unwrap();
    fs::write(
        dir.join("static/404.html"),
        "<html><head><title>Not found</title><meta name=\"robots\" content=\"noindex\"></head></html>",
    )
    .unwrap();
    fs::write(
        dir.join("static/legacy.html"),
        "<html><head><title>Legacy</title></head></html>",
    )
    .unwrap();
    let report = check(&dir);
    let errors = report["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "{report}");
    assert_eq!(errors[0]["check"], "sitemap.missing_page");
    assert_eq!(errors[0]["page"], "legacy.html");
    assert!(errors[0]["hint"].as_str().unwrap().contains("noindex"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn non_html_routes_skip_head_and_sitemap_but_stay_link_targets() {
    let dir = site(
        "nonhtml",
        "routes:\n\
         \x20 - path: /index.html\n    template: p.html.j2\n    title: Home\n\
         \x20 - path: /llms.txt\n    template: llms.txt.j2\n    title: ignored\n\
         \x20 - path: /feed.xml\n    template: feed.xml.j2\n",
        "{}",
        &[
            (
                "p.html.j2",
                "<html><head>{{ std_head }}</head><body><a href=\"/llms.txt\">llms</a></body></html>",
            ),
            ("llms.txt.j2", "# site\n[{{ std_head }}]\n"),
            ("feed.xml.j2", "<feed/>"),
        ],
    );
    let report = check(&dir);
    assert_eq!(report["verdict"], "pass", "{report}");
    // minijinja drops the template's final newline
    assert_eq!(
        built(&report, "llms.txt"),
        "# site\n[]",
        "std_head must be empty"
    );
    let sitemap = built(&report, "sitemap.xml");
    assert!(
        !sitemap.contains("llms") && !sitemap.contains("feed"),
        "{sitemap}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn non_html_route_still_collides() {
    let dir = site(
        "nonhtml-dup",
        "routes:\n\
         \x20 - path: /llms.txt\n    template: a.j2\n\
         \x20 - path: /LLMS.txt\n    template: a.j2\n",
        "{}",
        &[("a.j2", "x")],
    );
    assert_eq!(checks_of(&check(&dir)), ["paths.duplicate"]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn yaml_index_values_parse() {
    // bool, quoted bool-string, pattern — all accepted; list_routes reports them
    let dir = site(
        "yaml",
        "routes:\n\
         \x20 - path: /a.html\n    template: p.html.j2\n    index: true\n\
         \x20 - path: /b.html\n    template: p.html.j2\n    index: \"false\"\n\
         \x20 - path: /c.html\n    template: p.html.j2\n",
        "{}",
        &[("p.html.j2", HEAD_TEMPLATE)],
    );
    let s = molde::testing::load_site(&dir).unwrap();
    let flags: Vec<bool> = molde::testing::expand_routes(&s)
        .unwrap()
        .iter()
        .map(|p| p.index)
        .collect();
    assert_eq!(flags, [true, false, true]);

    let mcp = molde::mcp::McpServer {
        site: dir.clone(),
        out: dir.join("out"),
    };
    let resp = mcp
        .handle(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "list_routes", "arguments": {} },
        }))
        .unwrap();
    let routes: serde_json::Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let r = &routes["routes"];
    assert_eq!(r[0]["index"], true);
    assert_eq!(r[1]["index"], "false");
    assert_eq!(r[1]["noindex_pages"], 1);
    assert_eq!(r[2]["index"], true);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn static_noindex_is_recognised_in_every_attribute_quoting() {
    let dir = site(
        "static-quoting",
        "routes:\n  - path: /index.html\n    template: p.html.j2\n    title: Home\n",
        "{}",
        &[("p.html.j2", HEAD_TEMPLATE)],
    );
    fs::create_dir_all(dir.join("static")).unwrap();
    for (name, meta) in [
        ("double", r#"<meta name="robots" content="noindex">"#),
        ("single", "<meta name='robots' content='noindex, follow'>"),
        ("unquoted", "<meta name=robots content=noindex>"),
        ("unquoted-closed", "<META CONTENT=NOINDEX NAME=Robots />"),
        ("spaced", r#"<meta name = "robots" content = "none">"#),
    ] {
        fs::write(
            dir.join(format!("static/{name}.html")),
            format!("<html><head><title>{name}</title>{meta}</head></html>"),
        )
        .unwrap();
    }
    // the same unquoted tag, but allowing indexing, is still reported
    fs::write(
        dir.join("static/indexable.html"),
        "<html><head><title>x</title><meta name=robots content=index></head></html>",
    )
    .unwrap();
    let report = check(&dir);
    let errors = report["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "{report}");
    assert_eq!(errors[0]["check"], "sitemap.missing_page");
    assert_eq!(errors[0]["page"], "indexable.html");
    let _ = fs::remove_dir_all(&dir);
}
