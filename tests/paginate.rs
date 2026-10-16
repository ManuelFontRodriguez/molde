//! `paginate:` routes — one list, page 1 at the route's own path, pages
//! 2..n at `paginate.path`, and a `pagination` context in templates and patterns.
use molde::mcp::McpServer;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

fn site(tag: &str, routes: &str, data: &Value, template: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("molde-paginate-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(dir.join("routes.yaml"), routes).unwrap();
    fs::write(dir.join("data.json"), data.to_string()).unwrap();
    fs::write(dir.join("site.yaml"), "base_url: https://x.example\n").unwrap();
    fs::write(dir.join("templates/top.html.j2"), template).unwrap();
    dir
}

fn books(n: usize) -> Value {
    json!({ "books": (1..=n).map(|i| json!({ "name": format!("C{i}") })).collect::<Vec<_>>() })
}

const ROUTES: &str = "routes:
  - path: /top/index.html
    template: top.html.j2
    title: \"Top{% if pagination.n > 1 %}, page {{ pagination.n }}{% endif %}\"
    paginate:
      over: books
      per_page: 2
      path: /top/page/{n}/index.html
";

const TEMPLATE: &str = "<html><head>{{ std_head }}</head><body>\
<ol start=\"{{ pagination.offset + 1 }}\">{% for c in pagination.items %}<li>{{ c.name }}</li>{% endfor %}</ol>\
<p>{{ pagination.n }}/{{ pagination.pages }} of {{ pagination.total }} by {{ pagination.per_page }}</p>\
<nav>{% if pagination.prev %}<a rel=\"prev\" href=\"{{ pagination.prev }}\">prev</a>{% endif %}\
{% for l in pagination.links %}<a href=\"{{ l.path }}\">{{ l.n }}</a>{% endfor %}\
{% if pagination.next %}<a rel=\"next\" href=\"{{ pagination.next }}\">next</a>{% endif %}</nav>\
[{{ item is defined }}]</body></html>";

#[test]
fn pages_slice_the_list_and_link_each_other() {
    let dir = site("slice", ROUTES, &books(5), TEMPLATE);
    let s = molde::testing::load_site(&dir).unwrap();
    let paths: Vec<String> = molde::testing::expand_routes(&s)
        .unwrap()
        .into_iter()
        .map(|p| p.path)
        .collect();
    assert_eq!(
        paths,
        [
            "/top/index.html",
            "/top/page/2/index.html",
            "/top/page/3/index.html"
        ]
    );

    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    // unique titles, sitemap, canonicals and every pager link resolve
    assert_eq!(report["verdict"], "pass", "{report:#}");
    let read = |p: &str| fs::read_to_string(dir.join("out/current").join(p)).unwrap();

    let first = read("top/index.html");
    assert!(first.contains("<title>Top</title>"), "{first}");
    assert!(
        first.contains(r#"<ol start="1"><li>C1</li><li>C2</li></ol>"#),
        "{first}"
    );
    assert!(first.contains("<p>1/3 of 5 by 2</p>"), "{first}");
    assert!(!first.contains(r#"rel="prev""#), "{first}");
    assert!(
        first.contains(r#"<a rel="next" href="/top/page/2/">"#),
        "{first}"
    );
    assert!(
        first.contains(
            r#"<a href="/top/">1</a><a href="/top/page/2/">2</a><a href="/top/page/3/">3</a>"#
        ),
        "{first}"
    );
    // a paginate route has no `item`
    assert!(first.contains("[False]"), "{first}");

    let second = read("top/page/2/index.html");
    assert!(second.contains("<title>Top, page 2</title>"), "{second}");
    assert!(
        second.contains(r#"<ol start="3"><li>C3</li><li>C4</li></ol>"#),
        "{second}"
    );
    assert!(
        second.contains(r#"<a rel="prev" href="/top/">"#),
        "{second}"
    );
    assert!(
        second.contains(r#"<link rel="canonical" href="https://x.example/top/page/2/">"#),
        "{second}"
    );

    let last = read("top/page/3/index.html");
    assert!(last.contains(r#"<ol start="5"><li>C5</li></ol>"#), "{last}");
    assert!(!last.contains(r#"rel="next""#), "{last}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn empty_list_still_renders_page_one() {
    let dir = site("empty", ROUTES, &books(0), TEMPLATE);
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "pass", "{report:#}");
    assert_eq!(report["build"]["pages"], 1);
    let html = fs::read_to_string(dir.join("out/current/top/index.html")).unwrap();
    assert!(html.contains(r#"<ol start="1"></ol>"#), "{html}");
    assert!(html.contains("<p>1/1 of 0 by 2</p>"), "{html}");
    assert!(
        html.contains(r#"<nav><a href="/top/">1</a></nav>"#),
        "{html}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn exact_multiple_has_no_trailing_empty_page() {
    let dir = site("exact", ROUTES, &books(4), TEMPLATE);
    let s = molde::testing::load_site(&dir).unwrap();
    assert_eq!(molde::testing::expand_routes(&s).unwrap().len(), 2);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn every_route_pattern_sees_pagination() {
    let routes = format!(
        "{ROUTES}    description: \"Books {{{{ pagination.offset + 1 }}}} to {{{{ pagination.offset + pagination.items | length }}}}\"
    index: \"{{{{ pagination.n == 1 }}}}\"
    lastmod: \"{{% if pagination.n == 1 %}}2026-09-23{{% endif %}}\"
"
    );
    let dir = site("patterns", &routes, &books(3), TEMPLATE);
    let s = molde::testing::load_site(&dir).unwrap();
    let pages = molde::testing::expand_routes(&s).unwrap();
    assert_eq!(
        pages.iter().map(|p| p.index).collect::<Vec<_>>(),
        [true, false]
    );
    assert_eq!(pages[0].lastmod.as_deref(), Some("2026-09-23"));
    assert_eq!(pages[1].lastmod, None);
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "pass", "{report:#}");
    let first = fs::read_to_string(dir.join("out/current/top/index.html")).unwrap();
    assert!(
        first.contains(r#"<meta name="description" content="Books 1 to 2">"#),
        "{first}"
    );
    let second = fs::read_to_string(dir.join("out/current/top/page/2/index.html")).unwrap();
    assert!(second.contains(r#"content="noindex""#), "{second}");
    let sitemap = fs::read_to_string(dir.join("out/current/sitemap.xml")).unwrap();
    assert!(!sitemap.contains("/top/page/2/"), "{sitemap}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn pagination_is_undefined_on_other_routes() {
    let routes = "routes:\n  - path: /index.html\n    template: top.html.j2\n";
    let dir = site("plain", routes, &books(1), "[{{ pagination is defined }}]");
    let html = molde::render_url(&dir, "/").unwrap();
    assert_eq!(html, "[False]");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn pagination_is_undefined_in_patterns_of_other_routes() {
    let routes = "routes:
  - path: /index.html
    template: top.html.j2
    title: \"Home{% if pagination is defined %}, page {{ pagination.n }}{% endif %}\"
";
    let dir = site("plain-pattern", routes, &books(1), "{{ std_head }}");
    let html = molde::render_url(&dir, "/").unwrap();
    assert!(html.contains("<title>Home</title>"), "{html}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_page_colliding_with_another_route_names_the_paginate_block() {
    let routes = format!("{ROUTES}  - path: /top/page/2/index.html\n    template: top.html.j2\n");
    let dir = site("dup", &routes, &books(3), "x");
    let s = molde::testing::load_site(&dir).unwrap();
    let pages = molde::testing::expand_routes(&s).unwrap();
    let errors = molde::testing::check_duplicate_paths(&s, &pages);
    assert_eq!(errors.len(), 1, "{errors:#?}");
    let e = &errors[0];
    assert_eq!(e["check"], "paths.duplicate");
    assert_eq!(
        e["routes"][0]["paginate"]["path"],
        "/top/page/{n}/index.html"
    );
    assert_eq!(e["routes"][1]["paginate"], Value::Null);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn misconfigured_paginate_fails_loudly() {
    let cases = [
        (
            "foreach",
            "    foreach: books\n    paginate: {over: books, per_page: 2, path: \"/p/{n}/index.html\"}\n",
            "can't be combined",
        ),
        (
            "zero",
            "    paginate: {over: books, per_page: 0, path: \"/p/{n}/index.html\"}\n",
            "per_page must be at least 1",
        ),
        (
            "no-n",
            "    paginate: {over: books, per_page: 2, path: \"/p/index.html\"}\n",
            "contain {n}",
        ),
        (
            "other-field",
            "    paginate: {over: books, per_page: 2, path: \"/p/{slug}/{n}/index.html\"}\n",
            "only use {n}",
        ),
        (
            "over",
            "    paginate: {over: nope, per_page: 2, path: \"/p/{n}/index.html\"}\n",
            "route 0 (/top/index.html): paginate.over 'nope' is not in data.json; the top-level arrays in data.json are: books",
        ),
        (
            "over-not-array",
            "    paginate: {over: generated_at, per_page: 2, path: \"/p/{n}/index.html\"}\n",
            "route 0 (/top/index.html): paginate.over 'generated_at' is in data.json but is not an array",
        ),
        (
            "route-placeholder",
            "    paginate: {over: books, per_page: 2, path: \"/p/{n}/index.html\"}\n",
            "can't use placeholders",
        ),
    ];
    for (tag, extra, want) in cases {
        let path = if tag == "route-placeholder" {
            "/{name}/index.html"
        } else {
            "/top/index.html"
        };
        let routes = format!("routes:\n  - path: {path}\n    template: top.html.j2\n{extra}");
        let mut data = books(3);
        data["generated_at"] = json!("c1");
        let dir = site(tag, &routes, &data, "x");
        let err = molde::testing::load_site(&dir)
            .and_then(|s| molde::testing::expand_routes(&s))
            .err()
            .unwrap_or_else(|| panic!("{tag}: must fail"));
        assert!(format!("{err:#}").contains(want), "{tag}: {err:#}");
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn mcp_reports_paginate_and_page_position() {
    let dir = site("mcp", ROUTES, &books(5), TEMPLATE);
    let s = McpServer {
        site: dir.clone(),
        out: dir.join("out"),
    };
    let call = |name: &str, args: Value| -> Value {
        let resp = s
            .handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                             "params": { "name": name, "arguments": args } }))
            .unwrap();
        assert_eq!(resp["result"]["isError"], false, "{resp}");
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
    };
    let routes = call("list_routes", json!({}));
    let r = &routes["routes"][0];
    assert_eq!(
        r["paginate"],
        json!({ "over": "books", "per_page": 2, "path": "/top/page/{n}/index.html" })
    );
    assert_eq!(r["pages"], 3);
    let page = call("explain_route", json!({ "path": "/top/page/2/" }));
    assert_eq!(page["pagination"]["n"], 2);
    assert_eq!(page["pagination"]["prev"], "/top/");
    assert_eq!(page["pagination"]["pages"], 3);
    assert_eq!(page["pagination"]["next"], "/top/page/3/");
    assert!(page["pagination"].get("items").is_none(), "{page}");
    assert!(page["pagination"].get("links").is_none(), "{page}");
    let _ = fs::remove_dir_all(Path::new(&dir));
}
