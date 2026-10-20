//! `molde verify` on real generator output (tests/verify/fixtures, docs/design/verify.md §9):
//! every fixture reports exactly its recorded errors, and every check catches its own
//! defect when one is planted in a copy of a real build.
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/verify/fixtures/built")
        .join(name)
}

fn opts(base: &str) -> molde::VerifyOptions {
    molde::VerifyOptions {
        base_url: Some(base.into()),
        ..Default::default()
    }
}

fn ids(report: &Value, key: &str) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for e in report[key].as_array().unwrap() {
        *m.entry(e["check"].as_str().unwrap().to_string())
            .or_default() += 1;
    }
    m
}

fn errors(report: &Value) -> BTreeMap<String, usize> {
    ids(report, "errors")
}

/// Check ids and how many times each is expected.
type Want = &'static [(&'static str, usize)];
/// One planted defect: a name, how to plant it, what verify must report.
type Plant = (&'static str, Box<dyn Fn(&Path)>, Want);

fn map(pairs: &[(&str, usize)]) -> BTreeMap<String, usize> {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

/// The release gate: each fixture's recorded errors (tests/verify/fixtures/README.md),
/// none extra and none missing.
#[test]
fn every_fixture_reports_exactly_its_recorded_errors() {
    let cases: [(&str, molde::VerifyOptions, Want); 8] = [
        ("hugo-quickstart", opts("https://example.org/"), &[]),
        ("hugo-realistic", opts("https://example.org/"), &[]),
        // the content links /posts/my-first-post/ root-relative: one cause, 21 pages
        (
            "hugo-realistic-basepath",
            opts("https://user.github.io/repo/"),
            &[("links.outside_base", 1)],
        ),
        ("eleventy-base-blog", opts("https://example.com/"), &[]),
        ("astro-blog", opts("https://example.com/"), &[]),
        (
            "nextjs-export",
            molde::VerifyOptions {
                clean_urls: true,
                ..opts("https://example.com/")
            },
            &[],
        ),
        (
            "nextjs-export-trailing-slash",
            opts("https://example.com/"),
            &[],
        ),
        ("zola-getting-started", opts("https://example.com/"), &[]),
    ];
    for (name, o, want) in cases {
        let r = molde::run_verify(&fixture(name), &o).unwrap();
        assert_eq!(errors(&r), map(want), "{name}: {:#}", r["errors"]);
    }
    let r = molde::run_verify(
        &fixture("hugo-realistic-basepath"),
        &opts("https://user.github.io/repo/"),
    )
    .unwrap();
    let e = &r["errors"][0];
    assert_eq!(e["broken_href"], "/posts/my-first-post/");
    assert_eq!(e["target"], "/posts/my-first-post/");
    assert_eq!(e["count"], 21);
}

/// The genuine defects the starters ship are reported as warnings by default, and as
/// errors with --unique (titles shared by home, section and about pages).
#[test]
fn starters_duplicates_are_warnings_unless_unique() {
    let r = molde::run_verify(
        &fixture("zola-getting-started"),
        &opts("https://example.com/"),
    )
    .unwrap();
    assert_eq!(ids(&r, "warnings"), map(&[("titles.duplicate", 3)]));
    let strict = molde::VerifyOptions {
        unique: true,
        ..opts("https://example.com/")
    };
    let r = molde::run_verify(&fixture("zola-getting-started"), &strict).unwrap();
    assert_eq!(errors(&r), map(&[("titles.duplicate", 3)]));
    let r = molde::run_verify(&fixture("eleventy-base-blog"), &strict).unwrap();
    assert_eq!(
        errors(&r),
        map(&[("titles.duplicate", 2), ("description.duplicate", 2)])
    );
    // a list page doesn't hide a real pair: Hugo's home and posts section share a title
    let r = molde::run_verify(
        &fixture("hugo-quickstart"),
        &molde::VerifyOptions {
            unique: true,
            ..opts("https://example.org/")
        },
    )
    .unwrap();
    assert_eq!(
        errors(&r),
        map(&[("titles.duplicate", 1)]),
        "{:#}",
        r["errors"]
    );
}

/// A writable copy of a fixture, to plant one defect in.
fn copy(name: &str, tag: &str) -> PathBuf {
    let dst = std::env::temp_dir().join(format!("molde-verify-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dst);
    copy_dir(&fixture(name), &dst);
    dst
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            fs::copy(e.path(), target).unwrap();
        }
    }
}

fn edit(dir: &Path, rel: &str, from: &str, to: &str) {
    let p = dir.join(rel);
    let s = fs::read_to_string(&p).unwrap();
    assert!(s.contains(from), "{rel} lacks {from:?}");
    fs::write(p, s.replacen(from, to, 1)).unwrap();
}

/// Plant one defect in a copy of the realistic Hugo build: verify reports exactly that.
fn planted(tag: &str, plant: impl Fn(&Path), o: molde::VerifyOptions) -> BTreeMap<String, usize> {
    let dir = copy("hugo-realistic", tag);
    plant(&dir);
    let r = molde::run_verify(&dir, &o).unwrap();
    let _ = fs::remove_dir_all(&dir);
    errors(&r)
}

const BASE: &str = "https://example.org/";
const POST: &str = "posts/post-05/index.html";

#[test]
fn each_check_catches_its_own_defect_in_a_real_build() {
    let o = || opts(BASE);
    let cases: Vec<Plant> = vec![
        (
            "link",
            Box::new(|d| edit(d, POST, "</body>", "<a href=\"/posts/nope/\">x</a></body>")),
            &[("links.internal", 1)],
        ),
        (
            "own-abs-link",
            Box::new(|d| {
                edit(
                    d,
                    POST,
                    "</body>",
                    "<a href=\"https://example.org/nope/\">x</a></body>",
                )
            }),
            &[("links.internal", 1)],
        ),
        (
            "title-missing",
            Box::new(|d| edit(d, POST, "<title>", "<titlex>")),
            &[("titles.missing", 1)],
        ),
        (
            "title-multiple",
            Box::new(|d| edit(d, POST, "</head>", "<title>Again</title></head>")),
            &[("titles.multiple", 1)],
        ),
        (
            "escaped",
            Box::new(|d| edit(d, POST, "</body>", "<p>AT&amp;amp;T</p></body>")),
            &[("html.double_escaped", 1)],
        ),
        (
            "jsonld",
            Box::new(|d| {
                edit(
                    d,
                    POST,
                    "</head>",
                    "<script type=\"application/ld+json\">{\"a\":}</script></head>",
                )
            }),
            &[("jsonld.invalid", 1)],
        ),
        (
            "canonical-dup",
            Box::new(|d| {
                edit(
                    d,
                    POST,
                    "</head>",
                    "<link rel='canonical' href='https://example.org/posts/post-05/'></head>",
                )
            }),
            &[("canonical.duplicate", 1)],
        ),
        (
            "og-dup",
            Box::new(|d| {
                edit(
                    d,
                    POST,
                    "</head>",
                    "<meta property=og:type content=article></head>",
                )
            }),
            &[("og.duplicate", 1)],
        ),
        (
            "canonical-off-host",
            Box::new(|d| {
                edit(
                    d,
                    POST,
                    "<link rel=\"canonical\" href=\"https://example.org/posts/post-05/\"",
                    "<link rel=\"canonical\" href=\"https://elsewhere.example/post-05/\"",
                )
            }),
            &[("canonical.off_host", 1)],
        ),
        // the alias /old/post-3/ leads to post 3: remove post 3
        (
            "redirect-target",
            Box::new(|d| fs::remove_dir_all(d.join("posts/post-03")).unwrap()),
            &[
                ("redirect.target_missing", 1),
                ("sitemap.dangling", 1),
                ("links.internal", 1),
            ],
        ),
        (
            "noindex-listed",
            Box::new(|d| {
                edit(
                    d,
                    POST,
                    "</head>",
                    "<meta name=\"robots\" content=\"noindex\"></head>",
                )
            }),
            &[("sitemap.noindex_listed", 1)],
        ),
        (
            "dangling",
            Box::new(|d| {
                edit(
                    d,
                    "sitemap.xml",
                    "</urlset>",
                    "<url><loc>https://example.org/gone/</loc></url></urlset>",
                )
            }),
            &[("sitemap.dangling", 1)],
        ),
        (
            "robots-blocks",
            Box::new(|d| {
                fs::write(
                    d.join("robots.txt"),
                    "User-agent: *\nDisallow: /\nSitemap: https://example.org/sitemap.xml\n",
                )
                .unwrap()
            }),
            &[("robots.blocks_all", 1)],
        ),
        (
            "robots-sitemap",
            Box::new(|d| fs::write(d.join("robots.txt"), "User-agent: *\nAllow: /\n").unwrap()),
            &[("robots.sitemap_missing", 1)],
        ),
        // post 5's canonical hands its ranking to post 6, which asks not to be indexed
        // (post 5 is then a duplicate of 6, so the sitemap shouldn't list it either)
        (
            "canonical-noindex",
            Box::new(|d| {
                edit(
                    d,
                    POST,
                    "href=\"https://example.org/posts/post-05/\"",
                    "href=\"https://example.org/posts/post-06/\"",
                );
                edit(
                    d,
                    "posts/post-06/index.html",
                    "</head>",
                    "<meta name=\"robots\" content=\"noindex\"></head>",
                );
            }),
            &[
                ("canonical.target_noindex", 1),
                ("sitemap.noindex_listed", 1),
                ("sitemap.non_canonical_listed", 1),
            ],
        ),
        (
            "canonical-missing-target",
            Box::new(|d| {
                edit(
                    d,
                    POST,
                    "href=\"https://example.org/posts/post-05/\"",
                    "href=\"https://example.org/posts/gone/\"",
                )
            }),
            &[("canonical.target_missing", 1)],
        ),
    ];
    for (tag, plant, want) in cases {
        assert_eq!(planted(tag, plant, o()), map(want), "{tag}");
    }
}

#[test]
fn ignores_spa_fallback_and_the_project_root() {
    let plant = |d: &Path| {
        edit(
            d,
            POST,
            "</body>",
            "<a href=\"/api/users/\">x</a><a href=\"/app/settings\">y</a></body>",
        )
    };
    assert_eq!(
        planted("plain", plant, opts(BASE)),
        map(&[("links.internal", 2)])
    );
    let ignore = molde::VerifyOptions {
        ignore: vec!["/api/**".into()],
        ..opts(BASE)
    };
    assert_eq!(
        planted("ignore", plant, ignore),
        map(&[("links.internal", 1)])
    );
    // the SPA fallback serves unknown pages, not missing files
    let spa = molde::VerifyOptions {
        spa_fallback: Some("/index.html".into()),
        ..opts(BASE)
    };
    let plant_asset = |d: &Path| {
        edit(
            d,
            POST,
            "</body>",
            "<a href=\"/app/settings\">y</a><img src=\"/gone.png\"></body>",
        )
    };
    assert_eq!(
        planted("spa", plant_asset, spa),
        map(&[("links.internal", 1)])
    );

    let root = std::env::temp_dir().join(format!("molde-verify-root-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let r = molde::run_verify(&root, &opts(BASE)).unwrap();
    assert_eq!(errors(&r), map(&[("verify.no_pages", 1)]));
    fs::write(root.join("hugo.toml"), "").unwrap();
    let r = molde::run_verify(&root, &opts(BASE)).unwrap();
    assert_eq!(errors(&r), map(&[("verify.project_root", 1)]));
    assert!(r["errors"][0]["message"]
        .as_str()
        .unwrap()
        .contains("public/"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn one_cause_one_error() {
    // Next.js without trailing slashes, verified as if the host didn't serve clean URLs:
    // its links name pages without .html; one error says to try --clean-urls
    let dir = copy("nextjs-export", "clean");
    for i in 0..3 {
        fs::write(
            dir.join(format!("p{i}.html")),
            "<!doctype html><html><head><title>P</title></head></html>",
        )
        .unwrap();
    }
    edit(
        &dir,
        "index.html",
        "</body>",
        "<a href=\"/p0\">0</a><a href=\"/p1\">1</a><a href=\"/p2\">2</a></body>",
    );
    let r = molde::run_verify(&dir, &opts("https://example.com/")).unwrap();
    assert_eq!(
        errors(&r),
        map(&[("links.clean_urls", 1)]),
        "{:#}",
        r["errors"]
    );
    let r = molde::run_verify(
        &dir,
        &molde::VerifyOptions {
            clean_urls: true,
            ..opts("https://example.com/")
        },
    )
    .unwrap();
    assert_eq!(errors(&r), map(&[]));
    let _ = fs::remove_dir_all(&dir);
    // a wrong --base-url is one error naming the address the pages use
    let r = molde::run_verify(&fixture("hugo-realistic"), &opts("https://example.com/")).unwrap();
    assert_eq!(errors(&r), map(&[("verify.base_url_mismatch", 1)]));
    assert_eq!(r["errors"][0]["found"], "https://example.org/");
    // the base path left out: the pages claim /repo/, and say so once
    let r = molde::run_verify(
        &fixture("hugo-realistic-basepath"),
        &opts("https://user.github.io/"),
    )
    .unwrap();
    assert_eq!(
        errors(&r),
        map(&[("verify.base_url_mismatch", 1)]),
        "{:#}",
        r["errors"]
    );
    assert_eq!(r["errors"][0]["found"], "https://user.github.io/repo/");
    // and with no --base-url at all, the report suggests the one the pages use
    let r = molde::run_verify(
        &fixture("hugo-realistic-basepath"),
        &molde::VerifyOptions::default(),
    )
    .unwrap();
    let s = r["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|s| s["suggestion"].as_str())
        .unwrap();
    assert!(s.contains("--base-url https://user.github.io/repo/"), "{s}");
}
