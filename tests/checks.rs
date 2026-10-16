//! JSON-LD must parse and carry its declared @types; site.yaml output budgets.
use std::fs;
use std::path::{Path, PathBuf};

fn site(name: &str, routes: &str, site_yaml: &str, templates: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("molde-checks-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(dir.join("routes.yaml"), routes).unwrap();
    fs::write(
        dir.join("data.json"),
        r#"{"books": [{"slug": "dune", "name": "Dune"}, {"slug": "t", "name": "Pride & Prejudice"}]}"#,
    )
    .unwrap();
    if !site_yaml.is_empty() {
        fs::write(dir.join("site.yaml"), site_yaml).unwrap();
    }
    for (file, content) in templates {
        fs::write(dir.join("templates").join(file), content).unwrap();
    }
    dir
}

fn errors(dir: &Path, check: &str) -> Vec<serde_json::Value> {
    let report = molde::run_check(dir, &dir.join("out")).unwrap();
    report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["check"] == check)
        .cloned()
        .collect()
}

const BOOK: &str = "<html><head><script type=\"application/ld+json\">\
    {{ {\"@context\": \"https://schema.org\", \"@graph\": [\
        {\"@type\": \"WebPage\", \"about\": {\"@type\": [\"Book\", \"CreativeWork\"], \"name\": item.name}}\
    ]} | tojson }}</script></head><body></body></html>";

#[test]
fn declared_types_are_found_at_any_depth() {
    let dir = site(
        "found",
        "routes:\n  - path: /{slug}/index.html\n    template: c.html.j2\n    foreach: books\n    jsonld: [WebPage, Book]\n",
        "",
        &[("c.html.j2", BOOK)],
    );
    assert!(errors(&dir, "jsonld.type_missing").is_empty());
    assert!(errors(&dir, "jsonld.invalid").is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn missing_type_names_missing_and_found() {
    let dir = site(
        "missing",
        "routes:\n  - path: /{slug}/index.html\n    template: c.html.j2\n    foreach: books\n    jsonld: Dataset\n",
        "",
        &[("c.html.j2", BOOK)],
    );
    let errs = errors(&dir, "jsonld.type_missing");
    assert_eq!(errs.len(), 2, "{errs:#?}");
    let e = &errs[0];
    assert_eq!(e["page"], "dune/index.html");
    assert_eq!(e["missing"], serde_json::json!(["Dataset"]));
    assert_eq!(
        e["found"],
        serde_json::json!(["Book", "CreativeWork", "WebPage"])
    );
    assert_eq!(e["route"], "/{slug}/index.html");
    assert_eq!(e["template"], "c.html.j2");
    assert!(e["hint"].is_string());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn page_without_any_block_misses_every_declared_type() {
    let dir = site(
        "noblock",
        "routes:\n  - path: /index.html\n    template: i.html.j2\n    jsonld: WebSite\n",
        "",
        &[("i.html.j2", "<html><head></head><body></body></html>")],
    );
    let errs = errors(&dir, "jsonld.type_missing");
    assert_eq!(errs.len(), 1, "{errs:#?}");
    assert_eq!(errs[0]["found"], serde_json::json!([]));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unparsable_block_is_invalid_even_without_a_declaration() {
    // a hand-written block with an unescaped data value: the classic breakage
    let dir = site(
        "invalid",
        "routes:\n  - path: /index.html\n    template: i.html.j2\n",
        "",
        &[(
            "i.html.j2",
            "<script type=\"application/ld+json\">{\"@type\": \"Thing\"}</script>\
             <script type=\"application/ld+json\">{\"name\": \"a \"quoted\" name\"}</script>",
        )],
    );
    let errs = errors(&dir, "jsonld.invalid");
    assert_eq!(errs.len(), 1, "{errs:#?}");
    assert_eq!(errs[0]["block"], 2);
    assert!(
        errs[0]["error"].as_str().unwrap().contains("line 1"),
        "{errs:#?}"
    );
    assert_eq!(errs[0]["template"], "i.html.j2");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn glob_semantics() {
    let m = |g: &str, p: &str| molde::testing::glob_regex(g).is_match(p);
    assert!(m("/*/index.html", "/dune/index.html"));
    assert!(!m("/*/index.html", "/a/b/index.html"));
    assert!(m("/**/index.html", "/index.html"));
    assert!(m("/**/index.html", "/a/b/index.html"));
    assert!(m("*.css", "/assets/site.css"));
    assert!(m("*.css", "/site.css"));
    assert!(!m("/*.css", "/assets/site.css"));
    assert!(m("/lists/**", "/lists/top-100/index.html"));
    assert!(m("/?.js", "/a.js"));
    assert!(!m("/?.js", "/ab.js"));
    assert!(!m("/a.js", "/aXjs"), "dots are literal");
}

#[test]
fn budgets_measure_raw_and_gzip_and_flag_unmatched_rules() {
    let big = format!("<html><body>{}</body></html>", "x".repeat(5000));
    let dir = site(
        "budgets",
        "routes:\n  - path: /{slug}/index.html\n    template: c.html.j2\n    foreach: books\n",
        "budgets:\n  - match: /*/index.html\n    max_bytes: 1000\n    max_gzip_bytes: 10\n  - match: /nothing/**\n    max_bytes: 1\n",
        &[("c.html.j2", &big)],
    );
    let exceeded = errors(&dir, "budget.exceeded");
    assert_eq!(exceeded.len(), 4, "{exceeded:#?}");
    let raw = &exceeded[0];
    assert_eq!(raw["file"], "/dune/index.html");
    assert_eq!(raw["metric"], "bytes");
    assert_eq!(raw["limit"], 1000);
    assert!(raw["size"].as_u64().unwrap() > 5000);
    assert_eq!(raw["route"], "/{slug}/index.html");
    let gz = &exceeded[1];
    assert_eq!(gz["metric"], "gzip_bytes");
    // the size of the .gz sibling, far below the raw size
    assert!(gz["size"].as_u64().unwrap() < 200, "{gz:#?}");
    let unmatched = errors(&dir, "budget.unmatched");
    assert_eq!(unmatched.len(), 1);
    assert_eq!(unmatched[0]["match"], "/nothing/**");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn budgets_within_limits_pass_and_gz_siblings_are_not_matched() {
    let dir = site(
        "within",
        "routes:\n  - path: /{slug}/index.html\n    template: c.html.j2\n    foreach: books\n",
        // `**` would also match dune/index.html.gz if siblings were measured
        "budgets:\n  - match: /**\n    max_gzip_bytes: 100000\n  - match: \"*.gz\"\n    max_bytes: 1\n",
        &[("c.html.j2", "<html><body>{{ item.name }}</body></html>")],
    );
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    let checks: Vec<&str> = report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["check"].as_str().unwrap())
        .filter(|c| c.starts_with("budget."))
        .collect();
    assert_eq!(checks, ["budget.unmatched"], "{report:#}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn budget_without_a_limit_fails_loading() {
    let dir = site(
        "nolimit",
        "routes:\n  - path: /index.html\n    template: i.html.j2\n",
        "budgets:\n  - match: /index.html\n",
        &[("i.html.j2", "hi")],
    );
    let err = molde::testing::load_site(&dir).err().expect("must fail");
    assert!(
        format!("{err:#}").contains("neither max_bytes nor max_gzip_bytes"),
        "{err:#}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn block_attribute_variants_iri_types_and_comments() {
    let dir = site(
        "variants",
        "routes:\n  - path: /index.html\n    template: i.html.j2\n    jsonld: [WebSite, Organization, Dataset, Book]\n",
        "",
        &[(
            "i.html.j2",
            "<SCRIPT TYPE='application/ld+json'>{\"@type\": \"WebSite\"}</SCRIPT>\
             <script id=x type=application/ld+json>{\"@type\": \"https://schema.org/Organization\"}</script>\
             <script type=\"application/ld+json; charset=utf-8\">{\"@type\": \"schema:Dataset\"}</script>\
             <script type=\"application/ld+json\">{{ {\"@type\": \"Book\", \"name\": \"</script>\"} | tojson }}</script>\
             <!-- <script type=\"application/ld+json\">{broken</script> -->\
             <script data-type=\"application/ld+json\">{not json</script>\
             <script type=\"application/ld+json-patch\">{not json</script>",
        )],
    );
    assert!(errors(&dir, "jsonld.invalid").is_empty());
    assert!(errors(&dir, "jsonld.type_missing").is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn commented_out_block_does_not_supply_a_type() {
    let dir = site(
        "comment",
        "routes:\n  - path: /index.html\n    template: i.html.j2\n    jsonld: WebSite\n",
        "",
        &[(
            "i.html.j2",
            "<!-- <script type=\"application/ld+json\">{\"@type\": \"WebSite\"}</script> -->",
        )],
    );
    assert_eq!(errors(&dir, "jsonld.type_missing").len(), 1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn bad_jsonld_declarations_and_unknown_keys_fail_loading() {
    let load_err = |name: &str, routes: &str, site_yaml: &str| {
        let dir = site(name, routes, site_yaml, &[("i.html.j2", "hi")]);
        let err = molde::testing::load_site(&dir).err().expect("must fail");
        let _ = fs::remove_dir_all(&dir);
        format!("{err:#}")
    };
    let route = "routes:\n  - path: /index.html\n    template: i.html.j2\n";
    let e = load_err("empty", &format!("{route}    jsonld: []\n"), "");
    assert!(e.contains("at least one non-empty @type"), "{e}");
    let e = load_err("blank", &format!("{route}    jsonld: \"\"\n"), "");
    assert!(e.contains("at least one non-empty @type"), "{e}");
    let e = load_err(
        "nonhtml",
        "routes:\n  - path: /feed.json\n    template: i.html.j2\n    jsonld: Dataset\n",
        "",
    );
    assert!(e.contains("only applies to .html pages"), "{e}");
    let e = load_err(
        "typo-site",
        route,
        "budget:\n  - match: /x\n    max_bytes: 1\n",
    );
    assert!(e.contains("unknown field `budget`"), "{e}");
    let e = load_err("typo-route", &format!("{route}    jsnold: WebSite\n"), "");
    assert!(e.contains("unknown field `jsnold`"), "{e}");
}

#[test]
fn trailing_slash_glob_means_the_whole_directory() {
    let m = |g: &str, p: &str| molde::testing::glob_regex(g).is_match(p);
    assert!(m("/lists/", "/lists/top-100/index.html"));
    assert!(m("lists/", "/a/lists/x.html"));
    assert!(!m("/lists/", "/other/index.html"));
}

#[test]
fn budget_errors_carry_page_and_route_for_any_route_output() {
    let dir = site(
        "nonhtml-budget",
        "routes:\n  - path: /feed.json\n    template: f.json.j2\n",
        "budgets:\n  - match: /feed.json\n    max_bytes: 1\n  - match: /static.gz\n    max_bytes: 1\n",
        &[("f.json.j2", "{\"items\": [1, 2, 3]}")],
    );
    // a .gz with no uncompressed twin is an ordinary file, matched like any other
    fs::create_dir_all(dir.join("static")).unwrap();
    fs::write(dir.join("static/static.gz"), "xxxxxxxx").unwrap();
    let exceeded = errors(&dir, "budget.exceeded");
    let feed = exceeded
        .iter()
        .find(|e| e["file"] == "/feed.json")
        .expect("feed");
    assert_eq!(feed["page"], "feed.json");
    assert_eq!(feed["route"], "/feed.json");
    assert_eq!(feed["template"], "f.json.j2");
    let gz = exceeded.iter().find(|e| e["file"] == "/static.gz");
    assert!(gz.is_some(), "{exceeded:#?}");
    assert!(gz.unwrap().get("route").is_none());
    let _ = fs::remove_dir_all(&dir);
}
