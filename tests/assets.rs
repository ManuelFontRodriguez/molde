//! static/ passthrough, robots.txt emission, and src-attribute link checking.
use std::fs;
use std::io::Read;
use std::path::PathBuf;

fn site(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("molde-assets-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: i.j2\n    title: Home\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    fs::write(dir.join("site.yaml"), "base_url: https://x.example\n").unwrap();
    fs::write(
        dir.join("templates/i.j2"),
        "<html><head>\n{{ std_head }}\n</head><body>x</body></html>",
    )
    .unwrap();
    dir
}

#[test]
fn static_files_copied_text_gzipped_binaries_not() {
    let dir = site("copy");
    fs::create_dir_all(dir.join("static/img")).unwrap();
    fs::write(dir.join("static/style.css"), "body { color: red; }").unwrap();
    fs::write(dir.join("static/img/logo.png"), [0x89u8, 0x50, 0x4E, 0x47]).unwrap();

    let report = molde::build(&dir, &dir.join("out")).unwrap();
    assert_eq!(report.assets, 2);
    assert!(report.build_dir.join("style.css").is_file());
    assert!(report.build_dir.join("img/logo.png").is_file());
    assert!(
        !report.build_dir.join("img/logo.png.gz").exists(),
        "binaries must not get .gz siblings"
    );
    let gz = fs::read(report.build_dir.join("style.css.gz")).expect("missing css .gz sibling");
    let mut out = String::new();
    flate2::read::GzDecoder::new(&gz[..])
        .read_to_string(&mut out)
        .unwrap();
    assert_eq!(out, "body { color: red; }");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn robots_emitted_with_base_url_and_static_wins() {
    // auto-emitted with the web-standards capability…
    let dir = site("robots");
    let report = molde::build(&dir, &dir.join("out")).unwrap();
    let robots = fs::read_to_string(report.build_dir.join("robots.txt")).unwrap();
    assert!(
        robots.contains("Sitemap: https://x.example/sitemap.xml"),
        "{robots}"
    );

    // …the author's static/robots.txt wins…
    fs::create_dir_all(dir.join("static")).unwrap();
    fs::write(
        dir.join("static/robots.txt"),
        "User-agent: *\nDisallow: /private/\n",
    )
    .unwrap();
    let report = molde::build(&dir, &dir.join("out")).unwrap();
    let robots = fs::read_to_string(report.build_dir.join("robots.txt")).unwrap();
    assert!(
        robots.contains("Disallow: /private/"),
        "static robots was overwritten"
    );

    // …and without base_url there is no robots at all
    fs::remove_file(dir.join("site.yaml")).unwrap();
    fs::remove_dir_all(dir.join("static")).unwrap();
    let report = molde::build(&dir, &dir.join("out")).unwrap();
    assert!(!report.build_dir.join("robots.txt").exists());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn broken_src_is_caught() {
    let dir = site("src");
    fs::write(
        dir.join("templates/i.j2"),
        "<html><head>\n{{ std_head }}\n</head><body><img src=\"/img/missing.png\"></body></html>",
    )
    .unwrap();
    let report = molde::build(&dir, &dir.join("out")).unwrap();
    let errors = molde::testing::check_links(&report.build_dir).unwrap();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0]["check"], "links.internal");
    assert_eq!(errors[0]["attribute"], "src");
    assert_eq!(errors[0]["broken_href"], "/img/missing.png");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn query_fragment_and_protocol_relative_refs_resolve_like_a_browser() {
    // `?query`/`#fragment` are not part of the file, on src as on href;
    // `//host/path` is another site, like https://
    let dir = site("query");
    fs::create_dir_all(dir.join("static")).unwrap();
    fs::write(dir.join("static/x.js"), "1").unwrap();
    fs::write(dir.join("static/x.css"), "a{}").unwrap();
    fs::write(
        dir.join("templates/i.j2"),
        "<html><head>\n{{ std_head }}\n<link rel=\"stylesheet\" href=\"/x.css?ver=1\">\n\
         <script src=\"/x.js?ver=0.011\"></script></head><body>\
         <a href=\"/?page=2#top\">home</a><a href=\"//example.com/x\">x</a>\
         <img src=\"//cdn.example.com/a.png\"><img src=\"/gone.png?v=2\"></body></html>",
    )
    .unwrap();
    let report = molde::build(&dir, &dir.join("out")).unwrap();
    let errors = molde::testing::check_links(&report.build_dir).unwrap();
    // only the genuinely missing file fails, reported as written
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0]["attribute"], "src");
    assert_eq!(errors[0]["broken_href"], "/gone.png?v=2");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn links_written_absolute_to_the_site_itself_are_checked() {
    // base_url is https://x.example: a URL written absolute to it, in either scheme or
    // protocol-relative, is a link into this build; another host stays another site
    let dir = site("own-address");
    fs::create_dir_all(dir.join("static")).unwrap();
    fs::write(dir.join("static/x.css"), "a{}").unwrap();
    fs::write(
        dir.join("templates/i.j2"),
        "<html><head>\n{{ std_head }}\n<link rel=\"stylesheet\" href=\"https://x.example/x.css?v=1\">\n\
         </head><body><a href=\"https://x.example/\">home</a>\
         <a href=\"https://x.example/gone/\">a</a><a href=\"http://X.example/gone2/\">b</a>\
         <img src=\"//x.example/gone.png\"><a href=\"https://other.example/gone/\">c</a></body></html>",
    )
    .unwrap();
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    assert_eq!(report["verdict"], "fail");
    let broken: Vec<&str> = report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["check"] == "links.internal")
        .map(|e| e["broken_href"].as_str().unwrap())
        .collect();
    assert_eq!(
        broken,
        [
            "https://x.example/gone/",
            "http://X.example/gone2/",
            "//x.example/gone.png"
        ]
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn traversal_in_data_or_routes_fails_the_build() {
    // a {field} value from DATA must never walk out of the build dir —
    // data.json is untrusted input to the router
    let dir = site("traversal");
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /s/{isbn}/index.html\n    template: i.j2\n    foreach: books\n",
    )
    .unwrap();
    fs::write(
        dir.join("data.json"),
        r#"{"books": [{"isbn": "../../evil"}]}"#,
    )
    .unwrap();
    let err = molde::build(&dir, &dir.join("out")).unwrap_err();
    assert!(
        format!("{err:#}").contains("not a safe path segment"),
        "{err:#}"
    );

    // and a hand-written route path with ".." is rejected too
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /a/../b/index.html\n    template: i.j2\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    let err = molde::build(&dir, &dir.join("out")).unwrap_err();
    assert!(format!("{err:#}").contains(r#"".." segment"#), "{err:#}");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn slash_in_pattern_field_is_rejected() {
    // even without dots, a '/' in a data field silently creates nested dirs —
    // a {field} is a single segment by contract
    let dir = site("slashes");
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /s/{isbn}/index.html\n    template: i.j2\n    foreach: books\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), r#"{"books": [{"isbn": "a/b"}]}"#).unwrap();
    let err = molde::build(&dir, &dir.join("out")).unwrap_err();
    assert!(
        format!("{err:#}").contains("not a safe path segment"),
        "{err:#}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_static_page_that_is_not_utf8_is_checked_not_fatal() {
    // a hand-written Latin-1 page used to stop `check` with bare "stream did not contain
    // valid UTF-8"; browsers show such pages, so the checks read it leniently
    let dir = site("latin1");
    fs::create_dir_all(dir.join("static")).unwrap();
    let mut page = b"<html><head><title>caf".to_vec();
    page.push(0xE9); // 'é' in Latin-1, invalid as UTF-8
    page.extend_from_slice(b"</title><meta name=\"robots\" content=\"noindex\"></head><body><a href=\"/missing/\">x</a></body></html>");
    fs::write(dir.join("static/old.html"), page).unwrap();
    let report = molde::run_check(&dir, &dir.join("out")).unwrap();
    let errors = report["errors"].as_array().unwrap();
    // the page is still checked: its broken link is found
    assert!(
        errors
            .iter()
            .any(|e| e["check"] == "links.internal" && e["page"] == "old.html"),
        "{report}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn links_are_read_the_way_a_browser_reads_them() {
    let dir = site("browserlinks");
    for folder in ["static/s/A&B", "static/caf\u{e9}"] {
        fs::create_dir_all(dir.join(folder)).unwrap();
        fs::write(dir.join(folder).join("index.html"), "<html><head><title>t</title><meta name=\"robots\" content=\"noindex\"></head><body>x</body></html>").unwrap();
    }
    fs::write(
        dir.join("templates/i.j2"),
        "<html><head>\n{{ std_head }}\n</head><body>\
         <a href=\"/s/A&amp;B/\">entity: fine</a>\
         <a href=\"/caf%C3%A9/\">percent-encoded: fine</a>\
         <a href='/caf%C3%A9/'>single-quoted, fine</a>\
         <a href='/gone-single/'>single-quoted, broken</a>\
         <a href=/gone-unquoted/>unquoted, broken</a>\
         <a href=\"/s/../../etc/passwd\">dot-dot, broken</a>\
         <a href=\"/bad%zzescape/\">malformed escape, broken</a>\
         </body></html>",
    )
    .unwrap();
    let report = molde::build(&dir, &dir.join("out")).unwrap();
    let errors = molde::testing::check_links(&report.build_dir).unwrap();
    let broken: Vec<&str> = errors
        .iter()
        .map(|e| e["broken_href"].as_str().unwrap())
        .collect();
    assert_eq!(
        broken,
        [
            "/gone-single/",
            "/gone-unquoted/",
            "/s/../../etc/passwd",
            "/bad%zzescape/"
        ],
        "{errors:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn numeric_entities_uppercase_attributes_and_script_code() {
    let dir = site("entities");
    fs::create_dir_all(dir.join("static/about")).unwrap();
    fs::write(dir.join("static/about/index.html"), "<html><head><title>a</title><meta name=\"robots\" content=\"noindex\"></head><body>x</body></html>").unwrap();
    fs::write(
        dir.join("templates/i.j2"),
        "<html><head>\n{{ std_head }}\n</head><body>\
         <a href=\"&#47;about/\">numeric entity: fine</a>\
         <a href=\"&#x2F;about&#x2f;\">hex entities: fine</a>\
         <A HREF=\"/gone-upper/\">uppercase, broken</A>\
         <a href=\"&#47;gone-entity/\">entity, broken</a>\
         <script>el.src=/gone-regex/.test(s) ? a : b; img.src=\"/in-script-quoted/\";</script>\
         <!-- <a href=/gone-in-comment/>old</a> -->\
         </body></html>",
    )
    .unwrap();
    let report = molde::build(&dir, &dir.join("out")).unwrap();
    let broken: Vec<String> = molde::testing::check_links(&report.build_dir)
        .unwrap()
        .iter()
        .map(|e| e["broken_href"].as_str().unwrap().to_string())
        .collect();
    // quoted URLs in scripts are still checked, as before; unquoted text in code is not
    assert_eq!(
        broken,
        ["/gone-upper/", "&#47;gone-entity/", "/in-script-quoted/"],
        "{broken:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn relative_srcset_and_css_references_are_checked() {
    // a site with all of these broken once passed `molde check`
    let dir = site("relative");
    fs::create_dir_all(dir.join("static/css")).unwrap();
    fs::create_dir_all(dir.join("static/img")).unwrap();
    fs::write(dir.join("static/img/logo.png"), [0u8]).unwrap();
    fs::write(
        dir.join("static/css/site.css"),
        "/* url(commented-out.png) */\nbody { background: url('../img/logo.png'); }\n\
         .x { background: url(\"/img/paper.png\"); }\n@import \"missing.css\";\n",
    )
    .unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: i.j2\n    title: Home\n\
         \x20 - path: /s/a/index.html\n    template: s.j2\n    title: A\n",
    )
    .unwrap();
    fs::write(
        dir.join("templates/i.j2"),
        "<html><head>\n{{ std_head }}\n<link rel=\"stylesheet\" href=\"css/site.css\"></head><body>\
         <img src=\"img/logo.png\" srcset=\"img/logo.png 1x, /img/logo@2x.png 2x\" alt=\"\">\
         <a href=\"s/a/\">fine</a><a href=\"mailto:x@example.com\">mail</a>\
         <div style=\"background:url(img/gone.png)\"></div></body></html>",
    )
    .unwrap();
    fs::write(
        dir.join("templates/s.j2"),
        "<html><head>\n{{ std_head }}\n</head><body>\
         <a href=\"../../\">home: fine</a><a href=\"../\">/s/: broken</a>\
         <a href=\"../../../../x\">above the root: broken</a></body></html>",
    )
    .unwrap();
    let report = molde::build(&dir, &dir.join("out")).unwrap();
    let mut found: Vec<(String, String, String)> = molde::testing::check_links(&report.build_dir)
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["page"].as_str().unwrap().to_string(),
                e["attribute"].as_str().unwrap().to_string(),
                e["broken_href"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    found.sort();
    let s = |a: &str, b: &str, c: &str| (a.to_string(), b.to_string(), c.to_string());
    assert_eq!(
        found,
        [
            s("css/site.css", "@import", "missing.css"),
            s("css/site.css", "url()", "/img/paper.png"),
            s("index.html", "srcset", "/img/logo@2x.png"),
            s("index.html", "url()", "img/gone.png"),
            s("s/a/index.html", "href", "../"),
            s("s/a/index.html", "href", "../../../../x"),
        ]
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn file_manager_litter_in_static_is_not_shipped() {
    let dir = site("litter");
    fs::create_dir_all(dir.join("static/img")).unwrap();
    fs::write(dir.join("static/style.css"), "a{}").unwrap();
    fs::write(dir.join("static/.DS_Store"), [0u8, 1]).unwrap();
    fs::write(dir.join("static/img/.DS_Store"), [0u8, 1]).unwrap();
    fs::write(dir.join("static/img/Thumbs.db"), [0u8, 1]).unwrap();
    fs::write(dir.join("static/img/logo.png"), [0x89u8, 0x50]).unwrap();
    let report = molde::build(&dir, &dir.join("out")).unwrap();
    assert_eq!(report.assets, 2, "only style.css and logo.png");
    for gone in [".DS_Store", "img/.DS_Store", "img/Thumbs.db"] {
        assert!(!report.build_dir.join(gone).exists(), "{gone} was copied");
    }
    assert!(report.build_dir.join("img/logo.png").is_file());
    let _ = fs::remove_dir_all(&dir);
}
