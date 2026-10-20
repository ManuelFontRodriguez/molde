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
            Box::new(|d| {
                fs::write(
                    d.join("robots.txt"),
                    "User-agent: *\nAllow: /\nSitemap: https://example.org/gone.xml\n",
                )
                .unwrap()
            }),
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
    // ...and its 32 broken links are that one cause, not 32 errors
    assert_eq!(
        errors(&r),
        map(&[("verify.base_url_mismatch", 1)]),
        "{:#}",
        r["errors"]
    );
    assert_eq!(r["errors"][0]["given"], Value::Null);
    assert_eq!(r["errors"][0]["found"], "https://user.github.io/repo/");
}

/// A planted defect run with its own options.
type Case = (&'static str, Box<dyn Fn(&Path)>, molde::VerifyOptions, Want);

/// What the held-out review (VitePress, Docusaurus, SvelteKit, Gatsby, stress sites)
/// found verify getting wrong, each planted in a copy of a real build.
#[test]
fn held_out_review_cases() {
    let o = || opts(BASE);
    let none = molde::VerifyOptions::default;
    let cases: Vec<Case> = vec![
        // a locale's own 404 page (Docusaurus, Hugo i18n) is a not-found page
        (
            "nested-404",
            Box::new(|d| {
                fs::create_dir_all(d.join("fr")).unwrap();
                fs::write(d.join("fr/404.html"), "<!doctype html><html><head><title>Introuvable</title>\
                    <link rel=\"canonical\" href=\"https://example.org/fr/404.html/\">\
                    <link rel=\"alternate\" hreflang=\"en\" href=\"https://example.org/404.html/\"></head></html>").unwrap();
            }),
            o(),
            &[],
        ),
        // an image sitemap's <image:loc> names an image, not a page
        (
            "image-loc",
            Box::new(|d| {
                edit(d, "sitemap.xml", "</url>",
                "<image:image><image:loc>https://cdn.example/x.png</image:loc></image:image></url>")
            }),
            o(),
            &[],
        ),
        // a robots.txt without Sitemap lines is valid when the build has no sitemap...
        (
            "robots-no-sitemap",
            Box::new(|d| {
                fs::remove_file(d.join("sitemap.xml")).unwrap();
                fs::write(d.join("robots.txt"), "User-agent: *\nDisallow:\n").unwrap();
            }),
            o(),
            &[],
        ),
        // ...and next to a sitemap it is a warning (Hugo's and Docsy's default); a named
        // sitemap that isn't there is an error
        (
            "robots-unnamed-sitemap",
            Box::new(|d| fs::write(d.join("robots.txt"), "User-agent: *\nDisallow:\n").unwrap()),
            o(),
            &[],
        ),
        (
            "robots-named-elsewhere",
            Box::new(|d| {
                fs::remove_file(d.join("sitemap.xml")).unwrap();
                fs::write(
                    d.join("robots.txt"),
                    "Sitemap: https://example.org/gone.xml\n",
                )
                .unwrap();
            }),
            o(),
            &[("robots.sitemap_missing", 1)],
        ),
        // a meta refresh is checked without --base-url, however its content is spelled
        (
            "refresh-no-base",
            Box::new(|d| {
                for (i, content) in [
                    "0;Url=/posts/",
                    "0; url = /posts/",
                    "0,url=/posts/",
                    "0;URL='/nope/'",
                ]
                .iter()
                .enumerate()
                {
                    fs::write(d.join(format!("r{i}.html")), format!(
                        "<!doctype html><html><head><title>r</title><meta http-equiv=\"refresh\" content=\"{content}\"></head></html>")).unwrap();
                }
            }),
            none(),
            &[("redirect.target_missing", 1)],
        ),
        // the code sample on a docs page is text, not a link
        (
            "code-sample",
            Box::new(|d| {
                edit(d, POST, "</body>",
                "<pre><code>&lt;link rel=&quot;icon&quot; href=&quot;/favicon.ico&quot;&gt;</code></pre></body>")
            }),
            o(),
            &[],
        ),
        // round 2: a noindex frame document (mdBook's toc.html) needs no title
        (
            "noindex-untitled",
            Box::new(|d| {
                fs::write(
                    d.join("toc.html"),
                    "<!doctype html><html><head>\
                <meta name=\"robots\" content=\"noindex\"></head><body></body></html>",
                )
                .unwrap()
            }),
            o(),
            &[],
        ),
        // round 2: the --spa-fallback file is a bare shell (React Router's)
        (
            "spa-fallback-shell",
            Box::new(|d| {
                fs::write(
                    d.join("__spa-fallback.html"),
                    "<!doctype html><html><head></head><body><div id=\"root\"></div></body></html>",
                )
                .unwrap()
            }),
            o(),
            &[],
        ),
        // a non-ASCII relative link used to panic
        (
            "non-ascii",
            Box::new(|d| edit(d, POST, "</body>", "<a href=\"カテゴリ/\">x</a></body>")),
            o(),
            &[("links.internal", 1)],
        ),
    ];
    for (tag, plant, opts, want) in cases {
        assert_eq!(planted(tag, plant, opts), map(want), "{tag}");
    }
    // the unnamed sitemap is still said, as a warning
    let dir = copy("hugo-realistic", "robots-warning");
    fs::write(dir.join("robots.txt"), "User-agent: *\n").unwrap();
    let r = molde::run_verify(&dir, &opts(BASE)).unwrap();
    assert_eq!(
        ids(&r, "warnings").get("robots.sitemap_missing"),
        Some(&1),
        "{r:#}"
    );
    // and a fallback named by the flag is a not-found page too
    fs::write(
        dir.join("app.html"),
        "<!doctype html><html><head></head></html>",
    )
    .unwrap();
    let spa = molde::VerifyOptions {
        spa_fallback: Some("/app.html".into()),
        ..opts(BASE)
    };
    let r = molde::run_verify(&dir, &spa).unwrap();
    assert!(
        !errors(&r).contains_key("titles.missing"),
        "{:#}",
        r["errors"]
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A shared value (a theme's default description) is one warning, not one per page.
#[test]
fn a_value_many_pages_share_is_one_warning() {
    let dir = copy("hugo-realistic", "shared-desc");
    let description = regex::Regex::new(r#"<meta name="description" content="[^"]*""#).unwrap();
    for f in walk(&dir) {
        if f.ends_with("index.html") && f.contains("posts/post-") {
            let s = fs::read_to_string(dir.join(&f)).unwrap();
            let s = description.replace(&s, r#"<meta name="description" content="The default""#);
            fs::write(dir.join(&f), s.as_ref()).unwrap();
        }
    }
    let r = molde::run_verify(&dir, &opts(BASE)).unwrap();
    let shared: Vec<&Value> = r["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["description"] == "The default")
        .collect();
    assert_eq!(shared.len(), 1, "{:#}", r["warnings"]);
    assert!(shared[0]["count"].as_u64().unwrap() > 10, "{:#}", shared[0]);
    assert_eq!(shared[0]["examples"].as_array().unwrap().len(), 5);
    let _ = fs::remove_dir_all(&dir);
}

fn walk(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for e in walkdir::WalkDir::new(dir) {
        let e = e.unwrap();
        if e.file_type().is_file() {
            out.push(
                e.path()
                    .strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    out
}

/// A refusal stops before reading the build, as check's stops do: `build` is null, and
/// the project root names the folder its generator writes.
#[test]
fn refusals_say_where_to_look() {
    let root = std::env::temp_dir().join(format!("molde-verify-refuse-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let r = molde::run_verify(&root, &opts(BASE)).unwrap();
    assert_eq!(r["build"], Value::Null);
    assert!(r["errors"][0]["hint"].is_string());
    for (marker, out) in [
        ("mkdocs.yml", "site/"),
        ("_config.yml", "_site/"),
        ("docusaurus.config.ts", "build/"),
        ("next.config.mjs", "out/"),
        (".eleventy.js", "_site/"),
    ] {
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(marker), "").unwrap();
        let r = molde::run_verify(&root, &opts(BASE)).unwrap();
        assert_eq!(errors(&r), map(&[("verify.project_root", 1)]), "{marker}");
        assert_eq!(r["build"], Value::Null);
        let message = r["errors"][0]["message"].as_str().unwrap();
        assert!(message.contains(out), "{marker}: {message}");
    }
    let _ = fs::remove_dir_all(&root);
}

/// What the review of the verify branch found, each planted in a copy of a real build.
#[test]
fn branch_review_cases() {
    let o = || opts(BASE);
    let page = |canonical: &str| {
        format!("<!doctype html><html><head><title>T{canonical}</title><link rel=\"canonical\" href=\"{canonical}\"></head><body>x</body></html>")
    };
    let cases: Vec<Case> = vec![
        // every canonical hard-coded to the home page: one error, not five silent aliases
        (
            "shared-canonical",
            Box::new(move |d| {
                for i in 0..5 {
                    fs::create_dir_all(d.join(format!("s{i}"))).unwrap();
                    fs::write(
                        d.join(format!("s{i}/index.html")),
                        page("https://example.org/"),
                    )
                    .unwrap();
                }
            }),
            o(),
            &[("canonical.shared", 1)],
        ),
        // a bare refresh stub needs no title
        (
            "refresh-stub",
            Box::new(|d| {
                fs::write(
                    d.join("go.html"),
                    "<!doctype html><meta http-equiv=\"refresh\" content=\"0; url=/posts/\">",
                )
                .unwrap()
            }),
            o(),
            &[],
        ),
        // a <noscript> refresh doesn't make a content page a redirect: its title is checked
        (
            "noscript-refresh",
            Box::new(|d| {
                fs::write(d.join("app.html"),
                "<!doctype html><html><head><noscript><meta http-equiv=\"refresh\" content=\"0; url=/posts/\"></noscript></head><body>x</body></html>").unwrap()
            }),
            o(),
            &[("titles.missing", 1)],
        ),
        // <base href="#"> is no base: a broken relative link is still found
        (
            "base-fragment",
            Box::new(|d| edit(d, POST, "</head>", "<base href=\"#\"></head>")),
            o(),
            &[],
        ),
        (
            "base-fragment-broken",
            Box::new(|d| {
                edit(
                    d,
                    POST,
                    "</body>",
                    "<base href=\"#\"><a href=\"gone/\">x</a></body>",
                )
            }),
            o(),
            &[("links.internal", 1)],
        ),
        // a Sitemap line on another host isn't the build's to judge
        (
            "robots-cdn-sitemap",
            Box::new(|d| {
                fs::write(
                    d.join("robots.txt"),
                    "Sitemap: https://cdn.example/sitemap.xml\n",
                )
                .unwrap()
            }),
            o(),
            &[],
        ),
    ];
    for (tag, plant, opts, want) in cases {
        assert_eq!(planted(tag, plant, opts), map(want), "{tag}");
    }
}

/// A symlinked file or folder inside the build is served; one leaving the build is not.
#[cfg(unix)]
#[test]
fn symlinks_inside_the_build_are_files() {
    let dir = copy("hugo-realistic", "symlinks");
    std::os::unix::fs::symlink(dir.join("posts"), dir.join("articles")).unwrap();
    std::os::unix::fs::symlink(dir.join("index.html"), dir.join("home.html")).unwrap();
    std::os::unix::fs::symlink("/", dir.join("outside")).unwrap();
    edit(&dir, POST, "</body>", "<a href=\"/articles/\">a</a><a href=\"/home.html\">h</a><a href=\"/outside/etc/hosts\">o</a></body>");
    let r = molde::run_verify(&dir, &opts(BASE)).unwrap();
    let broken: Vec<&str> = r["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["broken_href"].as_str())
        .collect();
    // (through `articles/` the post exists twice, so its one broken link is found twice)
    assert!(
        !broken.is_empty() && broken.iter().all(|b| *b == "/outside/etc/hosts"),
        "{:#}",
        r["errors"]
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The base path compares decoded; and with a wrong --base-url, a sitemap entry that is
/// wrong under both addresses is kept alone, not with the wrong address's other entries.
#[test]
fn encoded_base_paths_and_mismatch_keys() {
    let r = molde::run_verify(
        &fixture("hugo-realistic-basepath"),
        &opts("https://user.github.io/re%70o/"),
    )
    .unwrap();
    assert_eq!(
        errors(&r),
        map(&[("links.outside_base", 1)]),
        "{:#}",
        r["errors"]
    );
    let dir = copy("hugo-realistic", "mismatch-loc");
    edit(
        &dir,
        "sitemap.xml",
        "</urlset>",
        "<url><loc>https://example.org/gone/</loc></url></urlset>",
    );
    let r = molde::run_verify(&dir, &opts("https://example.com/")).unwrap();
    let dangling = r["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["check"] == "sitemap.dangling")
        .count();
    assert!(dangling <= 1, "{:#}", r["errors"]);
    // a published repository root: refused, unless asked
    fs::write(dir.join("package.json"), "{}").unwrap();
    assert_eq!(
        errors(&molde::run_verify(&dir, &opts(BASE)).unwrap()),
        map(&[("verify.project_root", 1)])
    );
    let allowed = molde::VerifyOptions {
        allow_project_root: true,
        ..opts(BASE)
    };
    assert!(
        !errors(&molde::run_verify(&dir, &allowed).unwrap()).contains_key("verify.project_root")
    );
    let _ = fs::remove_dir_all(&dir);
}

/// What the adversarial review of the verify rules found (sites s1–s24 of that review),
/// each planted in a copy of a real build.
#[test]
fn adversarial_review_cases() {
    let o = || opts(BASE);
    let canon = |c: &str| {
        format!("<!doctype html><html><head><title>T {c}</title><link rel=\"canonical\" href=\"https://example.org{c}\"></head><body>x</body></html>")
    };
    let put = |d: &Path, rel: &str, html: &str| {
        let p = d.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, html).unwrap();
    };
    let cases: Vec<Case> = vec![
        // related pages naming one page are the known patterns, not a template bug
        (
            "shared-related",
            Box::new(move |d| {
                for l in ["de", "fr", "es"] {
                    put(
                        d,
                        &format!("{l}/posts/post-05/index.html"),
                        &canon("/posts/post-05/"),
                    );
                }
                for v in ["1.0", "2.0", "3.0"] {
                    put(d, &format!("{v}/guide/index.html"), &canon("/posts/"));
                }
                for n in 2..5 {
                    put(d, &format!("posts/page-{n}/index.html"), &canon("/posts/"));
                    put(
                        d,
                        &format!("posts/post-05/v{n}/index.html"),
                        &canon("/posts/post-05/"),
                    );
                }
            }),
            o(),
            &[],
        ),
        // ...but content pages under archive/ and category/ naming the home page are
        (
            "shared-unrelated",
            Box::new(move |d| {
                for i in 0..5 {
                    put(d, &format!("archive/a{i}/index.html"), &canon("/"));
                }
            }),
            o(),
            &[("canonical.shared", 1)],
        ),
        // a JavaScript redirect with only a <noscript> refresh is a redirect stub
        (
            "noscript-stub",
            Box::new(move |d| {
                put(d, "go.html", "<!doctype html><html><head><script>location.replace('/nope/')</script>\
                <noscript><meta http-equiv=\"refresh\" content=\"0; url=/nope/\"></noscript></head><body></body></html>")
            }),
            o(),
            &[("redirect.target_missing", 1)],
        ),
        // <base-button href> is a custom element, not <base>: its href is a link
        (
            "base-custom-element",
            Box::new(|d| {
                edit(d, POST, "</body>", "<base-button href=\"/posts/\">b</base-button><a href=\"../post-06/\">x</a></body>")
            }),
            o(),
            &[],
        ),
        // a noindex print view naming its page as canonical needs no title
        (
            "noindex-print-view",
            Box::new(move |d| {
                put(d, "posts/post-05/print/index.html", "<!doctype html><html><head>\
                <meta name=\"robots\" content=\"noindex\"><link rel=\"canonical\" href=\"https://example.org/posts/post-05/\"></head><body>x</body></html>")
            }),
            o(),
            &[],
        ),
        // a page that reloads itself is not a redirect: it is still held to a title
        (
            "self-refresh",
            Box::new(move |d| {
                put(d, "live/index.html", "<!doctype html><html><head>\
                <meta http-equiv=\"refresh\" content=\"30; url=/live/\"></head><body>x</body></html>")
            }),
            o(),
            &[("titles.missing", 1)],
        ),
        // a refresh resolves from the page's <base>
        (
            "refresh-base",
            Box::new(move |d| {
                put(d, "r/go.html", "<!doctype html><html><head><base href=\"/posts/\">\
                <meta http-equiv=\"refresh\" content=\"0; url=post-05/\"></head><body></body></html>")
            }),
            o(),
            &[],
        ),
    ];
    for (tag, plant, opts, want) in cases {
        assert_eq!(planted(tag, plant, opts), map(want), "{tag}");
    }
}

/// A site whose every page is noindex is a warning: a staging setting that may have shipped.
#[test]
fn a_whole_noindex_site_is_said() {
    let dir = std::env::temp_dir().join(format!("molde-verify-allnoindex-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for p in ["index.html", "a.html"] {
        fs::write(dir.join(p), "<!doctype html><html><head><title>t</title><meta name=\"robots\" content=\"noindex\"></head></html>").unwrap();
    }
    let r = molde::run_verify(&dir, &molde::VerifyOptions::default()).unwrap();
    assert_eq!(
        ids(&r, "warnings").get("verify.all_noindex"),
        Some(&1),
        "{r:#}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Sixteen levels of two links to one folder: each real folder is walked once (the walk
/// used to grow as 2^16). And .git is never part of the build, node_modules not when the
/// folder is a published repository.
#[cfg(unix)]
#[test]
fn the_walk_visits_each_real_folder_once() {
    let dir = std::env::temp_dir().join(format!("molde-verify-diamond-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.html"), "<!doctype html><html><head><title>t</title></head><body><a href=\"/.git/x.html\">g</a></body></html>").unwrap();
    let mut level = dir.join("l0");
    fs::create_dir_all(&level).unwrap();
    for i in 1..=16 {
        let next = dir.join(format!("l{i}"));
        fs::create_dir_all(&next).unwrap();
        std::os::unix::fs::symlink(&next, level.join("a")).unwrap();
        std::os::unix::fs::symlink(&next, level.join("b")).unwrap();
        level = next;
    }
    fs::create_dir_all(dir.join(".git")).unwrap();
    fs::write(
        dir.join(".git/x.html"),
        "<!doctype html><html><head></head></html>",
    )
    .unwrap();
    let started = std::time::Instant::now();
    let r = molde::run_verify(&dir, &molde::VerifyOptions::default()).unwrap();
    assert!(started.elapsed().as_secs() < 10, "{:?}", started.elapsed());
    assert_eq!(
        errors(&r),
        map(&[("links.internal", 1)]),
        "{:#}",
        r["errors"]
    );
    let _ = fs::remove_dir_all(&dir);
}
