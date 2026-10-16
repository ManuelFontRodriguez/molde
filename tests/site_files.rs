//! Errors in the site's own files name the file, and a site.yaml that exists
//! but cannot be read fails instead of silently switching capabilities off.
use std::fs;
use std::path::{Path, PathBuf};

fn site(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("molde-sitefiles-{tag}-{}", std::process::id()));
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

fn load_error(dir: &Path) -> String {
    match molde::testing::load_site(dir) {
        Ok(_) => panic!("expected load_site to fail"),
        Err(e) => format!("{e:#}"),
    }
}

#[test]
fn parse_errors_name_the_file() {
    let dir = site("names");
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /x\n   template: t\n",
    )
    .unwrap();
    let err = load_error(&dir);
    assert!(err.starts_with("parsing routes.yaml: "), "{err}");

    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: i.j2\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), "{bad").unwrap();
    let err = load_error(&dir);
    assert!(err.starts_with("parsing data.json: "), "{err}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unknown_top_level_key_in_routes_yaml_fails() {
    // a typo such as `route:` or `rotues:` used to be ignored without a word
    let dir = site("unknown");
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: i.j2\nredirects: []\n",
    )
    .unwrap();
    let err = load_error(&dir);
    assert!(err.contains("unknown field `redirects`"), "{err}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unreadable_site_yaml_fails_absent_one_does_not() {
    let dir = site("siteyaml");
    assert!(
        molde::testing::load_site(&dir).is_ok(),
        "no site.yaml is fine"
    );
    // present but unreadable (a directory here, so the test also holds as root)
    fs::create_dir_all(dir.join("site.yaml")).unwrap();
    let err = load_error(&dir);
    assert!(err.starts_with("reading site.yaml: "), "{err}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn base_url_must_be_an_absolute_url() {
    // without a scheme every canonical and sitemap URL is broken (`<loc>x.com/…`)
    let dir = site("baseurl");
    for bad in [
        "x.example",
        "",
        "https://",
        "ftp://x.example",
        "https:// x.example",
    ] {
        fs::write(dir.join("site.yaml"), format!("base_url: {bad:?}\n")).unwrap();
        let err = load_error(&dir);
        assert!(err.contains("must be an absolute URL"), "{bad:?}: {err}");
    }
    for ok in [
        "https://x.example",
        "http://localhost:8080/",
        "https://x.example/blog",
    ] {
        fs::write(dir.join("site.yaml"), format!("base_url: {ok:?}\n")).unwrap();
        assert!(molde::testing::load_site(&dir).is_ok(), "{ok:?}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn base_url_scheme_is_case_insensitive_and_has_no_query() {
    let dir = site("baseurl2");
    fs::write(dir.join("site.yaml"), "base_url: \"HTTPS://Example.com\"\n").unwrap();
    assert!(molde::testing::load_site(&dir).is_ok());
    for bad in ["https://example.com?x=1", "https://example.com#top"] {
        fs::write(dir.join("site.yaml"), format!("base_url: {bad:?}\n")).unwrap();
        assert!(load_error(&dir).contains("no ?query or #fragment"), "{bad}");
    }
    let _ = fs::remove_dir_all(&dir);
}
