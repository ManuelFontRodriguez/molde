//! `group_by`: one page per distinct value of an item field (genre or tag pages),
//! following the data with no hand-kept list.
use std::fs;
use std::path::PathBuf;

fn site(tag: &str, data: &serde_json::Value) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("molde-groupby-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n\
         \x20 - path: /index.html\n    template: home.html.j2\n    title: Home\n\
         \x20 - path: /genres/{genre}/index.html\n    template: genre.html.j2\n\
         \x20   foreach: books\n    group_by: genre\n    title: \"{{ item.genre }}\"\n\
         \x20 - path: /tags/{tags}/index.html\n    template: tag.html.j2\n\
         \x20   foreach: books\n    group_by: tags\n    title: \"Tag {{ item.tags }}\"\n",
    )
    .unwrap();
    fs::write(dir.join("site.yaml"), "base_url: https://x.example\n").unwrap();
    fs::write(dir.join("data.json"), data.to_string()).unwrap();
    fs::write(
        dir.join("templates/home.html.j2"),
        "<html><head>{{ std_head }}</head><body>home</body></html>",
    )
    .unwrap();
    fs::write(
        dir.join("templates/genre.html.j2"),
        "<html><head>{{ std_head }}</head><body><h1>{{ item.genre }}</h1>\
         {% for s in item.items %}<p>{{ s.title }}</p>{% endfor %}</body></html>",
    )
    .unwrap();
    fs::write(
        dir.join("templates/tag.html.j2"),
        "<html><head>{{ std_head }}</head><body>{{ item.items | length }}</body></html>",
    )
    .unwrap();
    dir
}

fn books(extra: &[(&str, &str, &[&str])]) -> serde_json::Value {
    let mut list = vec![
        serde_json::json!({ "title": "Dune", "genre": "Fantasy", "tags": ["classic", "english"] }),
        serde_json::json!({ "title": "Ulysses", "genre": "Poetry", "tags": ["classic"] }),
        serde_json::json!({ "title": "Emma", "genre": "Fantasy", "tags": [] }),
    ];
    for (t, s, tags) in extra {
        list.push(serde_json::json!({ "title": t, "genre": s, "tags": tags }));
    }
    serde_json::json!({ "books": list })
}

#[test]
fn one_page_per_value_in_order_of_first_appearance() {
    let dir = site("pages", &books(&[]));
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "pass", "{report}");
    let current = dir.join("out/current");
    let fantasy = fs::read_to_string(current.join("genres/Fantasy/index.html")).unwrap();
    assert!(fantasy.contains("<p>Dune</p><p>Emma</p>"), "{fantasy}");
    assert!(fs::read_to_string(current.join("genres/Poetry/index.html"))
        .unwrap()
        .contains("<p>Ulysses</p>"));
    // a list field: an item in every group it names, none for an empty list
    assert!(fs::read_to_string(current.join("tags/classic/index.html"))
        .unwrap()
        .contains(">2<"));
    assert!(fs::read_to_string(current.join("tags/english/index.html"))
        .unwrap()
        .contains(">1<"));
    let sitemap = fs::read_to_string(current.join("sitemap.xml")).unwrap();
    for loc in [
        "genres/Fantasy/",
        "genres/Poetry/",
        "tags/classic/",
        "tags/english/",
    ] {
        assert!(
            sitemap.contains(&format!("https://x.example/{loc}")),
            "{loc}"
        );
    }
    let s = molde::testing::load_site(&dir).unwrap();
    let pages = molde::testing::expand_routes(&s).unwrap();
    let genres: Vec<&str> = pages
        .iter()
        .filter(|p| p.route == 1)
        .map(|p| p.path.as_str())
        .collect();
    assert_eq!(
        genres,
        ["/genres/Fantasy/index.html", "/genres/Poetry/index.html"]
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_new_value_in_the_data_gets_its_page_without_touching_the_site() {
    // a book in a genre nobody listed
    let dir = site("mutation", &books(&[("Walden", "Memoir", &["new"])]));
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "pass", "{report}");
    assert!(dir.join("out/current/genres/Memoir/index.html").is_file());
    assert!(dir.join("out/current/tags/new/index.html").is_file());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_or_unusable_value_is_reported_never_dropped() {
    for (tag, bad) in [
        (
            "nogenre",
            serde_json::json!({ "books": [{ "title": "A", "tags": [] }] }),
        ),
        (
            "emptygenre",
            serde_json::json!({ "books": [{ "title": "A", "genre": "", "tags": [] }] }),
        ),
        (
            "badtag",
            serde_json::json!({ "books": [{ "title": "A", "genre": "S", "tags": [{}] }] }),
        ),
        (
            "slash",
            serde_json::json!({ "books": [{ "title": "A", "genre": "a/b", "tags": [] }] }),
        ),
    ] {
        let dir = site(tag, &bad);
        let report = molde::run_check(&dir, &dir.join("out")).unwrap();
        assert_eq!(
            report["errors"][0]["check"], "config.invalid",
            "{tag}: {report}"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn group_by_needs_foreach() {
    let dir = site("noforeach", &books(&[]));
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /x/index.html\n    template: home.html.j2\n    group_by: genre\n",
    )
    .unwrap();
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert!(
        report["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("group_by needs foreach"),
        "{report}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A site whose genre pages use `{genre|slug}` and link to them with `| slug`.
fn slug_site(tag: &str, data: &serde_json::Value) -> PathBuf {
    let dir = site(tag, data);
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n\
         \x20 - path: /index.html\n    template: home.html.j2\n    title: Home\n\
         \x20 - path: /genres/{genre|slug}/index.html\n    template: genre.html.j2\n\
         \x20   foreach: books\n    group_by: genre\n    title: \"{{ item.genre }}\"\n",
    )
    .unwrap();
    fs::write(
        dir.join("templates/home.html.j2"),
        "<html><head>{{ std_head }}</head><body>\
         {% for s in data.books %}<a href=\"/genres/{{ s.genre | slug }}/\">{{ s.genre }}</a>\
         {% endfor %}</body></html>",
    )
    .unwrap();
    dir
}

#[test]
fn a_slug_placeholder_makes_any_value_a_path_and_templates_link_to_it() {
    // "Science Fiction" can't be a path segment as is
    let data = serde_json::json!({ "books": [
        { "title": "Dune", "genre": "Science Fiction", "tags": [] },
        { "title": "Ulysses", "genre": "Poésie & Théâtre", "tags": [] },
        { "title": "Emma", "genre": "Science Fiction", "tags": [] },
    ]});
    let dir = slug_site("slug", &data);
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "pass", "{report}");
    let current = dir.join("out/current");
    let page = fs::read_to_string(current.join("genres/science-fiction/index.html")).unwrap();
    // the page's item keeps the value as written
    assert!(
        page.contains("<h1>Science Fiction</h1><p>Dune</p><p>Emma</p>"),
        "{page}"
    );
    assert!(current.join("genres/poesie-theatre/index.html").is_file());
    // a new value in the data gets its page with no change to the site
    let mut more = data.clone();
    more["books"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({ "title": "Walden", "genre": "True Crime", "tags": [] }));
    fs::write(dir.join("data.json"), more.to_string()).unwrap();
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "pass", "{report}");
    assert!(current.join("genres/true-crime/index.html").is_file());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn two_values_with_one_slug_are_reported_never_merged() {
    let data = serde_json::json!({ "books": [
        { "title": "A", "genre": "True Crime", "tags": [] },
        { "title": "B", "genre": "true-crime", "tags": [] },
    ]});
    let dir = slug_site("collide", &data);
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "fail", "{report}");
    assert_eq!(report["errors"][0]["check"], "paths.duplicate", "{report}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_value_with_nothing_to_slug_or_an_unknown_filter_is_a_config_error() {
    let dir = slug_site(
        "emptyslug",
        &serde_json::json!({ "books": [
            { "title": "A", "genre": "Поэзия", "tags": [] },
        ]}),
    );
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["errors"][0]["check"], "config.invalid", "{report}");
    assert!(
        report["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("slug field"),
        "{report}"
    );
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /s/{genre|upper}/index.html\n    template: home.html.j2\n\
         \x20   foreach: books\n    group_by: genre\n",
    )
    .unwrap();
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert!(
        report["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("only `slug`"),
        "{report}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn malformed_or_unfillable_placeholders_are_config_errors() {
    for (tag, path, foreach, want) in [
        (
            "double",
            "/g/{genre|slug|slug}/index.html",
            true,
            "malformed placeholder",
        ),
        (
            "empty-filter",
            "/g/{genre|}/index.html",
            true,
            "malformed placeholder",
        ),
        (
            "braces",
            "/g/{{genre}}/index.html",
            true,
            "malformed placeholder",
        ),
        (
            "no-foreach",
            "/g/{genre|slug}/index.html",
            false,
            "no `foreach`",
        ),
    ] {
        let dir = slug_site(tag, &serde_json::json!({ "books": [] }));
        let route = if foreach {
            format!("routes:\n  - path: {path}\n    template: home.html.j2\n    foreach: books\n")
        } else {
            format!("routes:\n  - path: {path}\n    template: home.html.j2\n")
        };
        fs::write(dir.join("routes.yaml"), route).unwrap();
        let report = molde::run_check(&dir, &dir.join("out")).unwrap();
        assert_eq!(
            report["errors"][0]["check"], "config.invalid",
            "{tag}: {report}"
        );
        assert!(
            report["errors"][0]["message"]
                .as_str()
                .unwrap()
                .contains(want),
            "{tag}: {report}"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn the_slug_filter_refuses_what_a_path_would_refuse() {
    let dir = slug_site("filter-null", &serde_json::json!({ "books": [] }));
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: home.html.j2\n    title: Home\n",
    )
    .unwrap();
    for (tag, data) in [
        ("null", serde_json::json!({ "books": [], "x": null })),
        ("missing", serde_json::json!({ "books": [] })),
        ("list", serde_json::json!({ "books": [], "x": [1, "Ab"] })),
    ] {
        fs::write(dir.join("data.json"), data.to_string()).unwrap();
        fs::write(
            dir.join("templates/home.html.j2"),
            "<html><head>{{ std_head }}</head><body>{{ data.x | slug }}</body></html>",
        )
        .unwrap();
        let report = molde::run_check(&dir, &dir.join("out")).unwrap();
        assert_eq!(report["verdict"], "fail", "{tag}: {report}");
        assert!(
            report
                .to_string()
                .contains("slug needs a string or an integer"),
            "{tag}: {report}"
        );
    }
    fs::write(dir.join("data.json"), r#"{"books": [], "x": 1999}"#).unwrap();
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "pass", "{report}");
    let _ = fs::remove_dir_all(&dir);
}
