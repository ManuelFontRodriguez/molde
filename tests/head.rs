//! web-standards phase 1: std_head emission and the checks that ship with it.
//! Each test pins one behaviour of the web-standards capability (docs/reference.md).
use std::fs;
use std::path::{Path, PathBuf};

fn site(name: &str, routes: &str, templates: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("molde-head-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(dir.join("routes.yaml"), routes).unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    fs::write(dir.join("site.yaml"), "base_url: https://x.example\n").unwrap();
    for (file, content) in templates {
        fs::write(dir.join("templates").join(file), content).unwrap();
    }
    dir
}

fn head_errors(dir: &Path) -> Vec<serde_json::Value> {
    let s = molde::testing::load_site(dir).unwrap();
    let pages = molde::testing::expand_routes(&s).unwrap();
    let report = molde::build(dir, &dir.join("out")).unwrap();
    molde::testing::check_head(&report.build_dir, &s, &pages).unwrap()
}

#[test]
fn escaping_is_single() {
    // S3: the demo's "Pride & Prejudice" must be escaped exactly once in element AND attribute
    let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/demo");
    let out = std::env::temp_dir().join(format!("molde-head-esc-{}", std::process::id()));
    let report = molde::build(&demo, &out).expect("build failed");
    let html = fs::read_to_string(
        report
            .build_dir
            .join("books/pride-and-prejudice/index.html"),
    )
    .unwrap();
    assert!(
        html.contains("<title>Pride &amp; Prejudice by Jane Austen</title>"),
        "element escape: {html}"
    );
    assert!(
        html.contains(
            r#"<meta property="og:title" content="Pride &amp; Prejudice by Jane Austen">"#
        ),
        "attribute escape: {html}"
    );
    assert!(!html.contains("&amp;amp;"), "double escape: {html}");
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn forgotten_std_head_fails_canonical_missing() {
    let dir = site(
        "noslot",
        "routes:\n  - path: /index.html\n    template: p.j2\n    title: T\n",
        &[(
            "p.j2",
            "<html><head><title>T</title></head><body>x</body></html>",
        )],
    );
    let errors = head_errors(&dir);
    assert!(
        errors.iter().any(|e| e["check"] == "canonical.missing"),
        "expected canonical.missing: {errors:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn handwritten_canonical_without_og_fails() {
    // S1: a correct hand-written canonical must not slip past the slot check —
    // the titled route still owes a full OG block
    let dir = site(
        "handwritten",
        "routes:\n  - path: /index.html\n    template: p.j2\n    title: T\n",
        &[(
            "p.j2",
            // attribute order deliberately reversed (A2)
            r#"<html><head><title>T</title><link href="https://x.example/" rel="canonical"></head><body>x</body></html>"#,
        )],
    );
    let errors = head_errors(&dir);
    let og_missing: Vec<_> = errors
        .iter()
        .filter(|e| e["check"] == "og.missing")
        .collect();
    assert_eq!(og_missing.len(), 3, "expected 3 og.missing: {errors:?}");
    assert!(
        !errors
            .iter()
            .any(|e| e["check"].as_str().unwrap().starts_with("canonical")),
        "reversed-attribute canonical should be recognized: {errors:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn untitled_route_emits_canonical_only() {
    // S2: OG is all-or-nothing — no title, no og:* tags, and no og errors
    let dir = site(
        "untitled",
        "routes:\n  - path: /index.html\n    template: p.j2\n",
        &[(
            "p.j2",
            "<html><head><title>hand</title>\n{{ std_head }}\n</head><body>x</body></html>",
        )],
    );
    let report = molde::build(&dir, &dir.join("out2")).unwrap();
    let html = fs::read_to_string(report.build_dir.join("index.html")).unwrap();
    assert!(html.contains(r#"<link rel="canonical" href="https://x.example/">"#));
    assert!(!html.contains("og:"), "untitled route leaked OG: {html}");
    let errors = head_errors(&dir);
    assert!(
        errors.is_empty(),
        "untitled route should be conformant: {errors:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn wrong_canonical_is_mismatch() {
    let dir = site(
        "mismatch",
        "routes:\n  - path: /index.html\n    template: p.j2\n",
        &[(
            "p.j2",
            r#"<html><head><title>t</title><link rel="canonical" href="https://elsewhere.example/"></head><body>x</body></html>"#,
        )],
    );
    let errors = head_errors(&dir);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0]["check"], "canonical.mismatch");
    assert_eq!(errors[0]["expected"], "https://x.example/");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn non_index_route_url_shape() {
    // S4: /about.html keeps its extension in canonical AND sitemap, via one page_url
    let dir = site(
        "about",
        "routes:\n  - path: /about.html\n    template: p.j2\n    title: About\n",
        &[(
            "p.j2",
            "<html><head>\n{{ std_head }}\n</head><body><a href=\"/about.html\">self</a></body></html>",
        )],
    );
    let report = molde::build(&dir, &dir.join("out2")).unwrap();
    let html = fs::read_to_string(report.build_dir.join("about.html")).unwrap();
    assert!(
        html.contains(r#"href="https://x.example/about.html""#),
        "{html}"
    );
    let sitemap = fs::read_to_string(report.build_dir.join("sitemap.xml")).unwrap();
    assert!(
        sitemap.contains("<loc>https://x.example/about.html</loc>"),
        "{sitemap}"
    );
    let errors = head_errors(&dir);
    assert!(errors.is_empty(), "{errors:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn leftover_template_title_is_multiple() {
    // migration guard: route title + hand-written template title = two <title> tags
    let dir = site(
        "twotitles",
        "routes:\n  - path: /index.html\n    template: p.j2\n    title: T\n",
        &[(
            "p.j2",
            "<html><head><title>old</title>\n{{ std_head }}\n</head><body>x</body></html>",
        )],
    );
    let report = molde::build(&dir, &dir.join("out2")).unwrap();
    let errors = molde::testing::check_titles(&report.build_dir).unwrap();
    assert!(
        errors.iter().any(|e| e["check"] == "titles.multiple"),
        "expected titles.multiple: {errors:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn svg_and_script_titles_are_not_page_titles() {
    // An inline icon's accessible name is SVG's <title>, not the document's
    let dir = site(
        "svgtitle",
        "routes:\n  - path: /index.html\n    template: p.j2\n    title: Home\n",
        &[(
            "p.j2",
            "<html><head>\n{{ std_head }}\n</head><body>\
             <svg viewBox=\"0 0 1 1\"><title>Search icon</title></svg>\
             <SVG><TITLE>Upper</TITLE></SVG>\
             <!-- <title>commented out</title> -->\
             <script>const t = '<title>x</title>';</script>\
             <template><title>t</title></template>\
             <math><title>m</title></math></body></html>",
        )],
    );
    let report = molde::build(&dir, &dir.join("out")).unwrap();
    let errors = molde::testing::check_titles(&report.build_dir).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn base_url_with_a_path_maps_sitemap_locs_to_files() {
    // A site served from a subdirectory lists https://host/blog/... in its
    // sitemap; the files are still at the build root
    let dir = site(
        "subdir",
        "routes:\n  - path: /index.html\n    template: p.j2\n    title: Home\n  \
         - path: /posts/a/index.html\n    template: p.j2\n    title: A\n",
        &[(
            "p.j2",
            "<html><head>\n{{ std_head }}\n</head><body>x</body></html>",
        )],
    );
    fs::write(dir.join("site.yaml"), "base_url: https://x.example/blog/\n").unwrap();
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    let sitemap_errors: Vec<_> = report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["check"].as_str().unwrap().starts_with("sitemap."))
        .collect();
    assert!(sitemap_errors.is_empty(), "{sitemap_errors:?}");

    // a loc outside the base is dangling, even when a file at its path exists
    let s = molde::testing::load_site(&dir).unwrap();
    let pages = molde::testing::expand_routes(&s).unwrap();
    let build = molde::testing::render_site(&dir, &dir.join("out2"))
        .unwrap()
        .build_dir;
    let xml = fs::read_to_string(build.join("sitemap.xml")).unwrap();
    fs::write(
        build.join("sitemap.xml"),
        xml.replace(
            "</urlset>",
            "  <url><loc>https://x.example/posts/a/</loc></url>\n  \
             <url><loc>https://x.example/blogger/</loc></url>\n</urlset>",
        ),
    )
    .unwrap();
    let errors =
        molde::testing::check_sitemap_at(&build, &pages, Some("https://x.example/blog/")).unwrap();
    let dangling: Vec<_> = errors
        .iter()
        .filter(|e| e["check"] == "sitemap.dangling")
        .map(|e| e["loc"].as_str().unwrap())
        .collect();
    assert_eq!(
        dangling,
        ["https://x.example/posts/a/", "https://x.example/blogger/"],
        "{errors:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn only_a_whole_index_html_file_name_is_dropped_from_urls() {
    // `/reindex.html` once became `https://x.example/re`, failing a correct site
    assert_eq!(
        molde::testing::page_url("https://x.example", "/index.html"),
        "https://x.example/"
    );
    assert_eq!(
        molde::testing::page_url("https://x.example", "/a/index.html"),
        "https://x.example/a/"
    );
    assert_eq!(
        molde::testing::page_url("https://x.example", "/reindex.html"),
        "https://x.example/reindex.html"
    );
    assert_eq!(
        molde::testing::page_url("https://x.example", "/a/myindex.html"),
        "https://x.example/a/myindex.html"
    );
    let dir = site(
        "reindex",
        "routes:\n  - path: /reindex.html\n    template: p.j2\n    title: R\n",
        &[(
            "p.j2",
            "<html><head>\n{{ std_head }}\n</head><body>x</body></html>",
        )],
    );
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "pass", "{report}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn text_escaped_twice_is_reported_once_per_page() {
    // an & escaped by hand before autoescape
    let dir = site(
        "doubleescape",
        "routes:\n  - path: /index.html\n    template: p.html.j2\n    title: Home\n",
        &[(
            "p.html.j2",
            "<html><head>\n{{ std_head }}\n</head><body>\
             <h1>{{ data.name | replace(\"&\", \"&amp;\") }}</h1><p>{{ data.dept }}</p>\
             <pre>&amp;amp; inside pre is shown on purpose</pre>\
             <code>&amp;lt;b&amp;gt;</code></body></html>",
        )],
    );
    // one value escaped by hand before autoescape, one already escaped in the data
    fs::write(
        dir.join("data.json"),
        r#"{"name": "Fish & Chips", "dept": "R&amp;D"}"#,
    )
    .unwrap();
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    let errors: Vec<_> = report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["check"] == "html.double_escaped")
        .collect();
    assert_eq!(errors.len(), 1, "{report}");
    assert_eq!(errors[0]["page"], "index.html");
    assert_eq!(errors[0]["found"], "&amp;amp;");
    assert_eq!(errors[0]["count"], 2);
    assert!(errors[0]["context"]
        .as_str()
        .unwrap()
        .contains("Fish &amp;amp; Chips"));
    // escaped once is the normal, correct case
    fs::write(
        dir.join("templates/p.html.j2"),
        "<html><head>\n{{ std_head }}\n</head><body><h1>{{ data.name }}</h1></body></html>",
    )
    .unwrap();
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "pass", "{report}");
    let _ = fs::remove_dir_all(&dir);
}

/// The ids of the head errors, in report order.
fn ids(errors: &[serde_json::Value]) -> Vec<&str> {
    errors
        .iter()
        .map(|e| e["check"].as_str().unwrap())
        .collect()
}

const ONE_PAGE: &str = "routes:\n  - path: /index.html\n    template: p.j2\n    title: Home\n";

#[test]
fn a_second_canonical_or_og_tag_is_a_duplicate_listing_every_value() {
    let dir = site(
        "dup",
        ONE_PAGE,
        &[(
            "p.j2",
            "<html><head>{{ std_head }}\n<link rel='canonical' href='https://x.example/other/'>\n\
             <meta property=og:type content=article></head><body>x</body></html>",
        )],
    );
    let errors = head_errors(&dir);
    assert_eq!(
        ids(&errors),
        ["canonical.duplicate", "og.duplicate"],
        "{errors:?}"
    );
    assert_eq!(
        errors[0]["found"],
        serde_json::json!(["https://x.example/", "https://x.example/other/"])
    );
    assert_eq!(errors[1]["property"], "og:type");
    assert_eq!(
        errors[1]["found"],
        serde_json::json!(["website", "article"])
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn og_title_that_contradicts_the_title_is_a_mismatch() {
    // one og:title, hand-written, saying something else (entities decoded on both sides)
    let dir = site(
        "ogtitle",
        "routes:\n  - path: /index.html\n    template: p.j2\n",
        &[(
            "p.j2",
            "<html><head><title>A &amp; B</title>\
             <link rel=\"canonical\" href=\"https://x.example/\">\
             <meta property=\"og:url\" content=\"https://x.example/\">\
             <meta property=\"og:title\" content=\"Something else\">\
             <meta property=\"og:type\" content=\"website\"></head><body>x</body></html>",
        )],
    );
    let errors = head_errors(&dir);
    assert_eq!(ids(&errors), ["og.title_mismatch"], "{errors:?}");
    assert_eq!(errors[0]["found"], "Something else");
    assert_eq!(errors[0]["expected"], "A & B");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn head_tags_are_read_in_any_quoting_and_attribute_case() {
    // a correct head written by hand with single quotes, unquoted values and upper-case
    // attribute names passes; before, its tags were invisible and it failed
    let dir = site(
        "quoting",
        ONE_PAGE,
        &[(
            "p.j2",
            "<html><head><title>Home</title>\
             <link REL='canonical' href='https://x.example/'>\
             <meta property='og:url' content='https://x.example/'>\
             <meta Property=og:title content=Home>\
             <meta property='og:type' content='website'></head><body>x</body></html>",
        )],
    );
    let errors = head_errors(&dir);
    assert!(errors.is_empty(), "{errors:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn head_tags_in_comments_scripts_and_templates_are_not_tags() {
    let dir = site(
        "not-tags",
        ONE_PAGE,
        &[(
            "p.j2",
            "<html><head>{{ std_head }}\n\
             <!-- <link rel=\"canonical\" href=\"https://old.example/\"> -->\n\
             <script>var t = '<meta property=\"og:title\" content=\"x\">';</script>\n\
             <template><link rel=\"canonical\" href=\"https://x.example/a/\"></template>\n\
             </head><body>x</body></html>",
        )],
    );
    let errors = head_errors(&dir);
    assert!(errors.is_empty(), "{errors:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn titles_are_compared_as_a_browser_shows_them() {
    let page = |title: &str, og: &str| {
        format!(
            "<html><head><title>{title}</title>\
             <link rel=\"canonical\" href=\"https://x.example/\">\
             <meta property=\"og:url\" content=\"https://x.example/\">\
             <meta property=\"og:title\" content=\"{og}\">\
             <meta property=\"og:type\" content=\"website\"></head><body>x</body></html>"
        )
    };
    for (title, og) in [
        ("Page\n   not found &mdash; Demo", "Page not found — Demo"), // whitespace, named entity
        ("A &amp; B", "A &amp; B"),                                   // entities on both sides
        ("Dune | Demo library", "Dune"),                              // the brand suffix left out
        ("It&rsquo;s here", "It’s here"),
    ] {
        let dir = site(
            "shown",
            "routes:\n  - path: /index.html\n    template: p.j2\n",
            &[("p.j2", &page(title, og))],
        );
        let errors = head_errors(&dir);
        assert!(errors.is_empty(), "{title:?} / {og:?}: {errors:?}");
        let _ = fs::remove_dir_all(&dir);
    }
    // a contradiction still fails, with both sides as shown
    let dir = site(
        "shown-bad",
        "routes:\n  - path: /index.html\n    template: p.j2\n",
        &[(
            "p.j2",
            &page("Dividend &mdash; stocks", "Dividend calendar"),
        )],
    );
    let errors = head_errors(&dir);
    assert_eq!(ids(&errors), ["og.title_mismatch"]);
    assert_eq!(errors[0]["expected"], "Dividend — stocks");
    let _ = fs::remove_dir_all(&dir);
}
