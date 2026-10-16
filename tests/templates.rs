//! Template environment: `tojson` exists and is safe inside
//! <script type="application/ld+json"> — the JSON-LD slot every SEO site needs.
use std::fs;
use std::path::PathBuf;

fn site(tag: &str, data: &str, template: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("molde-tmpl-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /{slug}/index.html\n    template: p.html.j2\n    foreach: books\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), data).unwrap();
    fs::write(dir.join("templates/p.html.j2"), template).unwrap();
    dir
}

/// The body of the one ld+json block, exactly as a browser's HTML parser sees it:
/// the script element ends at the FIRST `</script`, whatever the JSON says.
fn ld_json_block(html: &str) -> &str {
    let open = r#"<script type="application/ld+json">"#;
    let start = html.find(open).expect("no ld+json block") + open.len();
    let len = html[start..].find("</script").expect("unterminated block");
    &html[start..start + len]
}

const HOSTILE: &str = r#"Pride & Prejudice <b>"quoted"</b> </script><script>alert(1)</script>"#;

#[test]
fn tojson_ld_json_is_script_safe_and_round_trips() {
    let data = serde_json::json!({
        "books": [{ "slug": "t", "name": HOSTILE, "isbns": ["I1", "I&2"] }]
    });
    let dir = site(
        "ldjson",
        &data.to_string(),
        "<html><head><script type=\"application/ld+json\">\
         {{ {\"@context\": \"https://schema.org\", \"@type\": \"Book\", \
             \"name\": item.name, \"isbn\": item.isbns} | tojson }}\
         </script></head><body></body></html>",
    );
    let report = molde::build(&dir, &dir.join("out")).unwrap();
    let html = fs::read_to_string(report.build_dir.join("t/index.html")).unwrap();
    let block = ld_json_block(&html);

    // the hostile `</script>` could not end the element early, and nothing was
    // HTML-escaped a second time (autoescape is on for .html.j2)
    for raw in ["<", ">", "&"] {
        assert!(!block.contains(raw), "raw {raw:?} in ld+json: {block}");
    }
    assert!(!block.contains("&amp;"), "double escape: {block}");
    assert_eq!(
        html.matches("</script>").count(),
        1,
        "only the real closing tag may remain: {html}"
    );

    let parsed: serde_json::Value = serde_json::from_str(block).expect("ld+json must parse");
    assert_eq!(
        parsed,
        serde_json::json!({
            "@context": "https://schema.org",
            "@type": "Book",
            "name": HOSTILE,
            "isbn": ["I1", "I&2"],
        })
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn tojson_whole_item_round_trips() {
    // a pre-built object in data.json can be emitted as-is
    let item = serde_json::json!({
        "slug": "x", "ld": { "@type": "Thing", "name": HOSTILE, "n": 3, "ok": true, "none": null }
    });
    let dir = site(
        "item",
        &serde_json::json!({ "books": [item] }).to_string(),
        "<script type=\"application/ld+json\">{{ item.ld | tojson(indent=2) }}</script>",
    );
    let report = molde::build(&dir, &dir.join("out")).unwrap();
    let html = fs::read_to_string(report.build_dir.join("x/index.html")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(ld_json_block(&html)).unwrap();
    assert_eq!(parsed, item["ld"]);
    let _ = fs::remove_dir_all(&dir);
}

/// A review found that the `json` feature must not switch .json/.js/.yaml templates to JSON
/// autoescape. Non-HTML outputs render plain, exactly as before tojson existed.
#[test]
fn non_html_outputs_stay_plain_text() {
    let dir = std::env::temp_dir().join(format!("molde-tmpl-plain-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /a.json\n    template: a.json.j2\n  - path: /a.js\n    template: a.js.j2\n  - path: /a.yaml\n    template: a.yaml.j2\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), r#"{"name": "Apple", "list": ["x"]}"#).unwrap();
    fs::write(
        dir.join("templates/a.json.j2"),
        r#"{"name": "{{ data.name }}", "list": {{ data.list | tojson }}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("templates/a.js.j2"),
        r#"const n = "{{ data.name }}";"#,
    )
    .unwrap();
    fs::write(dir.join("templates/a.yaml.j2"), "name: {{ data.name }}\n").unwrap();
    let report = molde::build(&dir, &dir.join("out")).unwrap();
    let read = |f: &str| fs::read_to_string(report.build_dir.join(f)).unwrap();
    assert_eq!(read("a.json"), r#"{"name": "Apple", "list": ["x"]}"#);
    assert_eq!(read("a.js"), r#"const n = "Apple";"#);
    assert_eq!(read("a.yaml"), "name: Apple"); // minijinja drops the final newline
}

/// `page` (issue: templates had no way to know their own URL): `page.url` is the
/// canonical byte for byte, and `page.path` exists without a base_url.
#[test]
fn page_path_and_url_match_the_canonical() {
    let data = r#"{"books": [{"slug": "dune"}]}"#;
    let dir = site(
        "page",
        data,
        "<html><head>{{ std_head }}</head><body>\
         <script type=\"application/ld+json\">{{ {\"url\": page.url} | tojson }}</script>\
         <p id=\"path\">{{ page.path | safe }}</p></body></html>",
    );
    fs::write(dir.join("site.yaml"), "base_url: https://example.com/\n").unwrap();
    let report = molde::build(&dir, &dir.join("out")).unwrap();
    let html = fs::read_to_string(report.build_dir.join("dune/index.html")).unwrap();
    let ld: serde_json::Value = serde_json::from_str(ld_json_block(&html)).unwrap();
    assert_eq!(ld["url"], "https://example.com/dune/");
    assert!(
        html.contains(r#"<link rel="canonical" href="https://example.com/dune/">"#),
        "{html}"
    );
    assert!(html.contains(r#"<p id="path">/dune/</p>"#), "{html}");

    fs::remove_file(dir.join("site.yaml")).unwrap();
    let report = molde::build(&dir, &dir.join("out2")).unwrap();
    let html = fs::read_to_string(report.build_dir.join("dune/index.html")).unwrap();
    assert!(html.contains(r#"<p id="path">/dune/</p>"#), "{html}");
    assert!(
        html.contains(r#"{"url":null}"#) || html.contains(r#"{}"#),
        "{html}"
    );
}

/// Autoescape keeps '/' readable: data-built hrefs used to render as `&#x2f;a&#x2f;`.
/// The five characters that can break out of text or a quoted attribute still escape.
#[test]
fn html_autoescape_leaves_slashes_readable() {
    let data = serde_json::json!({
        "books": [{ "slug": "t", "href": "/genres/sci-fi/", "name": HOSTILE }]
    });
    let dir = site(
        "slash",
        &data.to_string(),
        "<a href=\"{{ item.href }}\" title=\"{{ item.name }}\">{{ item.name }}</a>{{ 1.5 }}",
    );
    let report = molde::build(&dir, &dir.join("out")).unwrap();
    let html = fs::read_to_string(report.build_dir.join("t/index.html")).unwrap();
    assert!(
        html.starts_with(r#"<a href="/genres/sci-fi/" title=""#),
        "{html}"
    );
    assert!(!html.contains("&#x2f;"), "{html}");
    let escaped = "Pride &amp; Prejudice &lt;b&gt;&quot;quoted&quot;&lt;/b&gt; &lt;/script&gt;";
    assert_eq!(html.matches(escaped).count(), 2, "{html}");
    assert!(
        !html.contains("<b>") && !html.contains("<script>"),
        "{html}"
    );
    assert!(html.ends_with("</a>1.5"), "{html}");
}
