//! Route `description` and `lastmod`: meta description + og:description
//! from a per-page pattern, and a data-derived sitemap <lastmod>.
use std::fs;
use std::path::{Path, PathBuf};

const HEAD_TEMPLATE: &str = "<html><head>\n{{ std_head }}\n</head><body>x</body></html>";

fn site(tag: &str, routes: &str, data: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("molde-meta-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(dir.join("routes.yaml"), routes).unwrap();
    fs::write(dir.join("data.json"), data).unwrap();
    fs::write(dir.join("site.yaml"), "base_url: https://x.example\n").unwrap();
    fs::write(dir.join("templates/p.html.j2"), HEAD_TEMPLATE).unwrap();
    dir
}

fn check(dir: &Path) -> serde_json::Value {
    molde::run_check(dir, &dir.join("out")).unwrap()
}

fn built(report: &serde_json::Value, rel: &str) -> String {
    let dir = PathBuf::from(report["build"]["dir"].as_str().unwrap());
    fs::read_to_string(dir.join(rel)).unwrap()
}

fn errors_of<'a>(report: &'a serde_json::Value, check: &str) -> Vec<&'a serde_json::Value> {
    report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["check"] == check)
        .collect()
}

const BOOKS: &str = r#"{"books": [
    {"slug": "a", "name": "Pride & Prejudice \"Austen\"", "as_of": "2026-09-22"},
    {"slug": "b", "name": "Beta", "as_of": "2026-09-21T08:30:00Z"}
]}"#;

#[test]
fn description_emits_meta_and_og_escaped_once() {
    let dir = site(
        "desc",
        "routes:\n\
         \x20 - path: /c/{slug}/index.html\n    template: p.html.j2\n    foreach: books\n\
         \x20   title: \"{{ item.name }}\"\n\
         \x20   description: \"  Notes on {{ item.name }}.\\n\"\n",
        BOOKS,
    );
    let report = check(&dir);
    assert_eq!(report["verdict"], "pass", "{report}");
    let html = built(&report, "c/a/index.html");
    let want = "Notes on Pride &amp; Prejudice &quot;Austen&quot;.";
    assert!(
        html.contains(&format!(r#"<meta name="description" content="{want}">"#)),
        "{html}"
    );
    assert!(
        html.contains(&format!(
            r#"<meta property="og:description" content="{want}">"#
        )),
        "{html}"
    );
    assert!(!html.contains("&amp;amp;"), "double escape: {html}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn description_without_og_block_and_on_noindex() {
    // untitled route: no OG block, so no og:description; the meta still ships.
    // noindex route: meta description kept, still no OG
    let dir = site(
        "desc-noog",
        "routes:\n\
         \x20 - path: /index.html\n    template: h.html.j2\n    description: Home page\n\
         \x20 - path: /404.html\n    template: p.html.j2\n    title: Gone\n    index: false\n\
         \x20   description: Home page\n",
        "{}",
    );
    fs::write(
        dir.join("templates/h.html.j2"),
        "<html><head><title>hand</title>{{ std_head }}</head></html>",
    )
    .unwrap();
    let report = check(&dir);
    // the shared description is fine: the 404 is not indexable
    assert_eq!(report["verdict"], "pass", "{report}");
    let home = built(&report, "index.html");
    assert!(home.contains(r#"<meta name="description" content="Home page">"#));
    assert!(!home.contains("og:"), "{home}");
    let gone = built(&report, "404.html");
    assert!(gone.contains(r#"<meta name="description" content="Home page">"#));
    assert!(!gone.contains("og:"), "{gone}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn description_missing_and_duplicate() {
    let dir = site(
        "desc-bad",
        "routes:\n\
         \x20 - path: /c/{slug}/index.html\n    template: p.html.j2\n    foreach: books\n\
         \x20   title: \"{{ item.name }}\"\n    description: \"{{ item.summary }}\"\n\
         \x20 - path: /x/{slug}/index.html\n    template: p.html.j2\n    foreach: books\n\
         \x20   title: \"x {{ item.name }}\"\n    description: Same for all\n",
        BOOKS,
    );
    let report = check(&dir);
    let missing = errors_of(&report, "description.missing");
    assert_eq!(missing.len(), 2, "{report}");
    assert_eq!(missing[0]["page"], "c/a/index.html");
    assert_eq!(missing[0]["route"], "/c/{slug}/index.html");
    assert!(missing[0]["hint"].as_str().unwrap().contains("misspelled"));
    let dup = errors_of(&report, "description.duplicate");
    assert_eq!(dup.len(), 1, "{report}");
    assert_eq!(dup[0]["page"], "x/b/index.html");
    assert_eq!(dup[0]["duplicate_of"], "x/a/index.html");
    assert_eq!(dup[0]["description"], "Same for all");
    assert!(dup[0]["hint"].as_str().unwrap().contains("item-dependent"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn lastmod_comes_from_data_and_lands_in_sitemap() {
    let dir = site(
        "lastmod",
        "routes:\n\
         \x20 - path: /index.html\n    template: p.html.j2\n    title: Home\n\
         \x20 - path: /c/{slug}/index.html\n    template: p.html.j2\n    foreach: books\n\
         \x20   title: \"{{ item.name }}\"\n    lastmod: \"{{ item.as_of }}\"\n",
        BOOKS,
    );
    let report = check(&dir);
    assert_eq!(report["verdict"], "pass", "{report}");
    let sitemap = built(&report, "sitemap.xml");
    assert!(
        sitemap.contains("  <url><loc>https://x.example/</loc></url>\n"),
        "no pattern, no lastmod: {sitemap}"
    );
    assert!(
        sitemap
            .contains("<url><loc>https://x.example/c/a/</loc><lastmod>2026-09-22</lastmod></url>"),
        "{sitemap}"
    );
    assert!(
        sitemap.contains("<lastmod>2026-09-21T08:30:00Z</lastmod>"),
        "{sitemap}"
    );

    // byte-stable: a rebuild with unchanged data dedups to the same build
    let again = check(&dir);
    assert_eq!(again["build"]["dir"], report["build"]["dir"]);

    let s = molde::testing::load_site(&dir).unwrap();
    let lastmods: Vec<Option<String>> = molde::testing::expand_routes(&s)
        .unwrap()
        .into_iter()
        .map(|p| p.lastmod)
        .collect();
    assert_eq!(
        lastmods,
        [
            None,
            Some("2026-09-22".to_string()),
            Some("2026-09-21T08:30:00Z".to_string())
        ]
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn lastmod_empty_is_omitted_invalid_is_reported() {
    let dir = site(
        "lastmod-bad",
        "routes:\n\
         \x20 - path: /c/{slug}/index.html\n    template: p.html.j2\n    foreach: books\n\
         \x20   title: \"{{ item.slug }}\"\n    lastmod: \"{{ item.updated }}\"\n",
        r#"{"books": [
            {"slug": "a"},
            {"slug": "b", "updated": "22/09/2026"},
            {"slug": "c", "updated": "2026-02-30"},
            {"slug": "d", "updated": "2026-09-22"}
        ]}"#,
    );
    let report = check(&dir);
    let invalid = errors_of(&report, "sitemap.lastmod_invalid");
    assert_eq!(invalid.len(), 2, "{report}");
    assert_eq!(invalid[0]["page"], "c/b/index.html");
    assert_eq!(invalid[0]["lastmod"], "22/09/2026");
    assert_eq!(invalid[0]["route"], "/c/{slug}/index.html");
    assert!(invalid[0]["hint"].as_str().unwrap().contains("YYYY-MM-DD"));
    assert_eq!(invalid[1]["lastmod"], "2026-02-30");
    assert_eq!(report["errors"].as_array().unwrap().len(), 2, "{report}");

    let sitemap = built(&report, "sitemap.xml");
    assert!(sitemap.contains("<url><loc>https://x.example/c/a/</loc></url>"));
    assert!(!sitemap.contains("22/09/2026") && !sitemap.contains("02-30"));
    assert!(sitemap.contains("<lastmod>2026-09-22</lastmod>"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn lastmod_formats() {
    for ok in [
        "2026-09-23",
        "2024-02-29",
        "2026-09-23T09:00:00Z",
        "2026-09-23t09:00:00z",
        "2026-09-23T09:00:00.123+02:00",
        "2026-09-23T23:59:60-05:30",
    ] {
        assert!(molde::testing::is_valid_lastmod(ok), "{ok}");
    }
    for bad in [
        "",
        "2026-9-23",
        "2025-02-29",
        "2026-13-01",
        "2026-09-23T09:00Z",
        "2026-09-23T09:00:00",
        "2026-09-23 09:00:00Z",
        "2026-09-23T24:00:00Z",
        "yesterday",
        // Unicode digits look like a date to `\d`; they are not one (and once crashed)
        "٢٠٢٦-٠١-٠١",
        "２０２６-０１-０１",
        "2026-09-23T09:00:00.５Z",
    ] {
        assert!(!molde::testing::is_valid_lastmod(bad), "{bad}");
    }
}

#[test]
fn a_unicode_digit_lastmod_is_reported_not_a_crash() {
    let dir = std::env::temp_dir().join(format!("molde-meta-unicode-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /s/{t}/index.html\n    template: p.j2\n    foreach: items\n\
         \x20   title: \"{{ item.t }}\"\n    lastmod: \"{{ item.d }}\"\n",
    )
    .unwrap();
    fs::write(
        dir.join("data.json"),
        r#"{"items":[{"t":"a","d":"٢٠٢٦-٠١-٠١"}]}"#,
    )
    .unwrap();
    fs::write(dir.join("site.yaml"), "base_url: https://x.example\n").unwrap();
    fs::write(
        dir.join("templates/p.j2"),
        "<html><head>{{ std_head }}</head><body>x</body></html>",
    )
    .unwrap();
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "fail", "{report}");
    assert_eq!(
        report["errors"][0]["check"], "sitemap.lastmod_invalid",
        "{report}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn list_routes_reports_the_new_fields() {
    let dir = site(
        "mcp",
        "routes:\n\
         \x20 - path: /c/{slug}/index.html\n    template: p.html.j2\n    foreach: books\n\
         \x20   title: \"{{ item.name }}\"\n    description: \"About {{ item.name }}\"\n\
         \x20   lastmod: \"{{ item.as_of }}\"\n",
        BOOKS,
    );
    let mcp = molde::mcp::McpServer {
        site: dir.clone(),
        out: dir.join("out"),
    };
    let call = |name: &str, args: serde_json::Value| -> serde_json::Value {
        let resp = mcp
            .handle(&serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": { "name": name, "arguments": args },
            }))
            .unwrap();
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
    };
    let routes = call("list_routes", serde_json::json!({}));
    let r = &routes["routes"][0];
    assert_eq!(r["description"], "About {{ item.name }}");
    assert_eq!(r["lastmod"], "{{ item.as_of }}");
    assert_eq!(r["index"], true);
    let page = call("explain_route", serde_json::json!({ "path": "/c/a/" }));
    assert_eq!(page["lastmod"], "2026-09-22");
    assert_eq!(page["index"], true);
    let _ = fs::remove_dir_all(&dir);
}
