//! Golden-file test: the build is a pure function, so the demo site must render
//! byte-identically to the committed golden files. Update goldens deliberately with:
//!   UPDATE_GOLDEN=1 cargo test
use std::fs;
use std::path::{Path, PathBuf};

fn collect_html(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            matches!(
                e.path().extension().and_then(|x| x.to_str()),
                Some("html") | Some("xml") | Some("txt") | Some("css")
            )
        })
        .map(|e| e.path().strip_prefix(dir).unwrap().to_path_buf())
        .collect();
    files.sort();
    files
}

#[test]
fn demo_site_matches_golden() {
    let site = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/demo");
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/demo");
    let out = std::env::temp_dir().join(format!("molde-golden-{}", std::process::id()));

    let report = molde::build(&site, &out).expect("build failed");
    assert_eq!(report.pages, 9, "unexpected page count");

    // conformance must pass on the golden site
    let errors = molde::testing::check_links(&report.build_dir).expect("check failed");
    assert!(errors.is_empty(), "broken links in demo: {errors:?}");
    let errors = molde::testing::check_titles(&report.build_dir).expect("check failed");
    assert!(errors.is_empty(), "title problems in demo: {errors:?}");
    let loaded = molde::testing::load_site(&site).expect("load failed");
    let pages = molde::testing::expand_routes(&loaded).expect("expand failed");
    let errors = molde::testing::check_sitemap(&report.build_dir, &pages).expect("check failed");
    assert!(errors.is_empty(), "sitemap problems in demo: {errors:?}");
    let errors =
        molde::testing::check_head(&report.build_dir, &loaded, &pages).expect("check failed");
    assert!(errors.is_empty(), "head problems in demo: {errors:?}");

    let built = collect_html(&report.build_dir);

    if std::env::var("UPDATE_GOLDEN").is_ok() {
        let _ = fs::remove_dir_all(&golden);
        for rel in &built {
            let dst = golden.join(rel);
            fs::create_dir_all(dst.parent().unwrap()).unwrap();
            fs::copy(report.build_dir.join(rel), dst).unwrap();
        }
        panic!("goldens updated — rerun without UPDATE_GOLDEN to verify");
    }

    let expected = collect_html(&golden);
    assert_eq!(built, expected, "page set differs from golden");
    for rel in &built {
        let got = fs::read_to_string(report.build_dir.join(rel)).unwrap();
        let want = fs::read_to_string(golden.join(rel)).unwrap();
        assert_eq!(got, want, "content drift in {}", rel.display());

        // every page ships a precompressed sibling that decompresses to the same bytes
        let mut gz_name = rel.file_name().unwrap().to_os_string();
        gz_name.push(".gz");
        let gz = fs::read(report.build_dir.join(rel.with_file_name(gz_name)))
            .expect("missing .gz sibling");
        let mut decompressed = String::new();
        std::io::Read::read_to_string(
            &mut flate2::read::GzDecoder::new(&gz[..]),
            &mut decompressed,
        )
        .unwrap();
        assert_eq!(decompressed, got, "gz drift in {}", rel.display());
    }

    let _ = fs::remove_dir_all(&out);
}

/// The multilingual example (locales, translations, localized paths, pagination,
/// group_by, budgets, JSON-LD) passes every check and renders byte for byte as committed.
#[test]
fn multilingual_site_matches_golden() {
    let site = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/multilingual");
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/multilingual");
    let out = std::env::temp_dir().join(format!("molde-golden-ml-{}", std::process::id()));
    let _ = fs::remove_dir_all(&out);
    let report = molde::run_check(&site, &out).expect("check failed to run");
    assert_eq!(report["verdict"], "pass", "{report:#}");
    assert_eq!(report["build"]["pages"], 35, "unexpected page count");
    let build_dir = PathBuf::from(report["build"]["dir"].as_str().unwrap());
    let built = collect_html(&build_dir);
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        let _ = fs::remove_dir_all(&golden);
        for rel in &built {
            let dst = golden.join(rel);
            fs::create_dir_all(dst.parent().unwrap()).unwrap();
            fs::copy(build_dir.join(rel), dst).unwrap();
        }
        panic!("goldens updated — rerun without UPDATE_GOLDEN to verify");
    }
    assert_eq!(built, collect_html(&golden), "page set differs from golden");
    for rel in &built {
        let got = fs::read_to_string(build_dir.join(rel)).unwrap();
        let want = fs::read_to_string(golden.join(rel)).unwrap();
        assert_eq!(got, want, "content drift in {}", rel.display());
    }
    let _ = fs::remove_dir_all(&out);
}

/// The broken example's report is exactly the committed one: every planted mistake is
/// found, with the page, route and template, and nothing else is reported.
#[test]
fn broken_example_reports_what_was_planted() {
    let site = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/broken");
    let expected_file = site.join("expected-report.json");
    let out = std::env::temp_dir().join(format!("molde-golden-broken-{}", std::process::id()));
    let _ = fs::remove_dir_all(&out);
    let mut report = molde::run_check(&site, &out).expect("check failed to run");
    assert!(
        !out.join("current").exists(),
        "a failing site must not publish"
    );
    report.as_object_mut().unwrap().remove("build");
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        let mut text = serde_json::to_string_pretty(&report).unwrap();
        text.push('\n');
        fs::write(&expected_file, text).unwrap();
        panic!("expected report updated — rerun without UPDATE_GOLDEN to verify");
    }
    let expected: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&expected_file).unwrap()).unwrap();
    assert_eq!(report, expected, "the broken example's report changed");
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn broken_link_is_caught() {
    // negative test: a site with a dangling href must fail conformance
    let dir = std::env::temp_dir().join(format!("molde-neg-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: index.html.j2\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    fs::write(
        dir.join("templates/index.html.j2"),
        r#"<html><body><a href="/does/not/exist/">nope</a></body></html>"#,
    )
    .unwrap();

    let out = dir.join("out");
    let report = molde::build(&dir, &out).expect("build failed");
    let errors = molde::testing::check_links(&report.build_dir).expect("check failed");
    assert_eq!(
        errors.len(),
        1,
        "expected exactly one broken link, got: {errors:?}"
    );
    assert_eq!(errors[0]["check"], "links.internal");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn duplicate_and_missing_titles_are_caught() {
    // negative test: two pages sharing a <title> plus one page without any
    let dir = std::env::temp_dir().join(format!("molde-titles-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /a/index.html\n    template: titled.html.j2\n  - path: /b/index.html\n    template: titled.html.j2\n  - path: /c/index.html\n    template: untitled.html.j2\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    fs::write(
        dir.join("templates/titled.html.j2"),
        "<html><head><title>Same title</title></head><body>x</body></html>",
    )
    .unwrap();
    fs::write(
        dir.join("templates/untitled.html.j2"),
        "<html><head></head><body>x</body></html>",
    )
    .unwrap();

    let out = dir.join("out");
    let report = molde::build(&dir, &out).expect("build failed");
    let errors = molde::testing::check_titles(&report.build_dir).expect("check failed");
    assert_eq!(
        errors.len(),
        2,
        "expected duplicate + missing, got: {errors:?}"
    );
    assert_eq!(errors[0]["check"], "titles.duplicate");
    assert_eq!(errors[0]["duplicate_of"], "a/index.html");
    assert_eq!(errors[1]["check"], "titles.missing");
    assert_eq!(errors[1]["page"], "c/index.html");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sitemap_disagreement_is_caught() {
    // negative test: tamper with a passing build — delete a listed page (dangling loc)
    // and drop in an unlisted one (missing_page)
    let dir = std::env::temp_dir().join(format!("molde-sitemap-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: page.html.j2\n  - path: /gone/index.html\n    template: page.html.j2\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    fs::write(dir.join("site.yaml"), "base_url: https://x.example\n").unwrap();
    fs::write(
        dir.join("templates/page.html.j2"),
        "<html><head><title>t</title></head><body>x</body></html>",
    )
    .unwrap();

    let out = dir.join("out");
    let report = molde::build(&dir, &out).expect("build failed");
    fs::remove_file(report.build_dir.join("gone/index.html")).unwrap();
    fs::write(
        report.build_dir.join("extra.html"),
        "<html><head><title>e</title></head><body>x</body></html>",
    )
    .unwrap();

    let errors = molde::testing::check_sitemap(&report.build_dir, &[]).expect("check failed");
    assert_eq!(
        errors.len(),
        2,
        "expected dangling + missing, got: {errors:?}"
    );
    assert_eq!(errors[0]["check"], "sitemap.dangling");
    assert_eq!(errors[0]["loc"], "https://x.example/gone/");
    assert_eq!(errors[1]["check"], "sitemap.missing_page");
    assert_eq!(errors[1]["page"], "extra.html");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn errors_carry_hint_and_source() {
    // the report is a todo list: every page-anchored error names its fixable
    // source (route + template) and ships a hint
    let dir = std::env::temp_dir().join(format!("molde-hints-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: index.html.j2\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    fs::write(
        dir.join("templates/index.html.j2"),
        r#"<html><head><title>t</title></head><body><a href="/nope/">x</a></body></html>"#,
    )
    .unwrap();

    let report = molde::run_check(&dir, &dir.join("out")).expect("check failed");
    assert_eq!(report["verdict"], "fail");
    let e = &report["errors"][0];
    assert_eq!(e["check"], "links.internal");
    assert!(e["hint"].as_str().unwrap().contains("route"), "{e}");
    assert_eq!(e["route"], "/index.html");
    assert_eq!(e["template"], "index.html.j2");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn failed_check_never_publishes() {
    // run_check on a good site publishes; breaking the site and re-checking must
    // leave `current` on the last good build — stale-but-consistent by construction
    let dir = std::env::temp_dir().join(format!("molde-gate-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: index.html.j2\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    fs::write(
        dir.join("templates/index.html.j2"),
        "<html><head><title>ok</title></head><body>v1</body></html>",
    )
    .unwrap();

    let out = dir.join("out");
    let report = molde::run_check(&dir, &out).expect("check failed");
    assert_eq!(report["verdict"], "pass");
    assert_eq!(report["published"], true);
    let good = fs::read_link(out.join("current")).unwrap();

    fs::write(
        dir.join("templates/index.html.j2"),
        r#"<html><head><title>ok</title></head><body><a href="/broken/">v2</a></body></html>"#,
    )
    .unwrap();
    let report = molde::run_check(&dir, &out).expect("check failed");
    assert_eq!(report["verdict"], "fail");
    assert_eq!(report["published"], false);
    assert_eq!(
        fs::read_link(out.join("current")).unwrap(),
        good,
        "a failing build went live"
    );

    let _ = fs::remove_dir_all(&dir);
}
