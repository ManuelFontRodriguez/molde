//! diff_builds: the review unit for a change — added/changed pages with the
//! first differing line, against two immutable builds.
use std::fs;

#[test]
fn diff_reports_changed_and_added_pages() {
    let dir = std::env::temp_dir().join(format!("molde-diff-{}", std::process::id()));
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
        "<html><head><title>t</title></head>\n<body>v1</body></html>",
    )
    .unwrap();

    let out = dir.join("out");
    molde::build(&dir, &out).expect("build 1 failed");

    // change the page and add a route
    fs::write(
        dir.join("templates/index.html.j2"),
        "<html><head><title>t</title></head>\n<body>v2</body></html>",
    )
    .unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: index.html.j2\n  - path: /about.html\n    template: about.html.j2\n",
    )
    .unwrap();
    fs::write(
        dir.join("templates/about.html.j2"),
        "<html><head><title>about</title></head><body>hi</body></html>",
    )
    .unwrap();
    molde::build(&dir, &out).expect("build 2 failed");

    let diff = molde::diff_builds(&out, None, None, false).expect("diff failed");
    assert_eq!(diff["added"], serde_json::json!(["about.html"]));
    assert_eq!(diff["removed"].as_array().unwrap().len(), 0);
    assert_eq!(diff["unchanged"], 0);
    let changed = diff["changed"].as_array().unwrap();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0]["page"], "index.html");
    assert_eq!(changed[0]["first_diff"]["line"], 2);
    assert_eq!(changed[0]["first_diff"]["from"], "<body>v1</body></html>");
    assert_eq!(changed[0]["first_diff"]["to"], "<body>v2</body></html>");

    // --full: the changed lines themselves, reviewable without opening files
    let diff = molde::diff_builds(&out, None, None, true).expect("full diff failed");
    let lines = diff["changed"][0]["diff"].as_array().unwrap();
    assert_eq!(lines[0], "-2: <body>v1</body></html>");
    assert_eq!(lines[1], "+2: <body>v2</body></html>");
    assert_eq!(diff["changed"][0]["diff_truncated"], false);

    // a single build has nothing to diff against
    let single = std::env::temp_dir().join(format!("molde-diff1-{}", std::process::id()));
    let _ = fs::remove_dir_all(&single);
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: index.html.j2\n",
    )
    .unwrap();
    molde::build(&dir, &single).expect("build failed");
    assert!(molde::diff_builds(&single, None, None, false).is_err());

    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&single);
}

#[test]
fn identical_rebuild_is_deduplicated() {
    // re-verification must not pollute the diff baseline or accumulate build dirs
    let dir = std::env::temp_dir().join(format!("molde-dedup-{}", std::process::id()));
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
        "<html><head><title>t</title></head><body>x</body></html>",
    )
    .unwrap();

    let out = dir.join("out");
    let first = molde::build(&dir, &out).expect("build 1 failed");
    let second = molde::build(&dir, &out).expect("build 2 failed");
    assert_eq!(
        first.build_dir, second.build_dir,
        "identical build minted a new id"
    );
    assert_eq!(molde::testing::list_builds(&out).unwrap().len(), 1);
    assert!(std::fs::read_link(out.join("current")).is_ok());

    // an actual change still gets a fresh build
    fs::write(
        dir.join("templates/index.html.j2"),
        "<html><head><title>t</title></head><body>y</body></html>",
    )
    .unwrap();
    let third = molde::build(&dir, &out).expect("build 3 failed");
    assert_ne!(second.build_dir, third.build_dir);
    assert_eq!(molde::testing::list_builds(&out).unwrap().len(), 2);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn diff_accepts_only_listed_build_ids() {
    // --from/--to (and the MCP diff_build arguments) come from the caller, possibly
    // an agent steered by untrusted content: a path must never reach the file system
    let dir = std::env::temp_dir().join(format!("molde-diff-ids-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let site = dir.join("site");
    fs::create_dir_all(site.join("templates")).unwrap();
    fs::write(
        site.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: index.html.j2\n",
    )
    .unwrap();
    fs::write(site.join("data.json"), "{}").unwrap();
    for body in ["v1", "v2"] {
        fs::write(
            site.join("templates/index.html.j2"),
            format!("<html><head><title>t</title></head><body>{body}</body></html>"),
        )
        .unwrap();
        molde::build(&site, &dir.join("out")).expect("build failed");
    }
    // two directories outside the out dir with text files a diff would print
    for (name, secret) in [("secretA", "token-a"), ("secretB", "token-b")] {
        fs::create_dir_all(dir.join(name)).unwrap();
        fs::write(dir.join(name).join("key.txt"), secret).unwrap();
    }

    let out = dir.join("out");
    let ids = molde::testing::list_builds(&out).unwrap();
    assert_eq!(ids.len(), 2);
    let abs_a = dir.join("secretA").display().to_string();
    let abs_b = dir.join("secretB").display().to_string();
    let attempts: [(&str, &str); 5] = [
        ("../../secretA", "../../secretB"),
        (&abs_a, &abs_b),
        ("..", &ids[1]),
        (&ids[0], "."),
        (&ids[0], "../builds"),
    ];
    for (from, to) in attempts {
        let err = molde::diff_builds(&out, Some(from), Some(to), true)
            .expect_err("a path outside the listed builds must be rejected");
        let msg = format!("{err:#}");
        assert!(msg.contains("not found"), "unexpected error: {msg}");
        assert!(!msg.contains("token-"), "leaked file contents: {msg}");
    }
    // a symlink inside builds/ that points outside is not a build either
    std::os::unix::fs::symlink(dir.join("secretA"), out.join("builds/linked")).unwrap();
    assert_eq!(molde::testing::list_builds(&out).unwrap(), ids);
    assert!(molde::diff_builds(&out, Some(&ids[0]), Some("linked"), true).is_err());
    // a lone --to outside the builds is rejected too
    assert!(molde::diff_builds(&out, None, Some("../../secretB"), true).is_err());
    // the listed ids still work
    let diff = molde::diff_builds(&out, Some(&ids[0]), Some(&ids[1]), false).unwrap();
    assert_eq!(diff["changed"][0]["page"], "index.html");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn failed_render_leaves_no_build_behind() {
    // A template error must not leave an empty build that `diff` then
    // treats as the newest one ("every page removed")
    let dir = std::env::temp_dir().join(format!("molde-diff-failed-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::create_dir_all(dir.join("static")).unwrap();
    fs::write(dir.join("static/style.css"), "body{}").unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: index.html.j2\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    let out = dir.join("out");
    for body in ["v1", "v2"] {
        fs::write(
            dir.join("templates/index.html.j2"),
            format!("<html><head><title>t</title></head><body>{body}</body></html>"),
        )
        .unwrap();
        molde::build(&dir, &out).expect("build failed");
    }
    let before = molde::testing::list_builds(&out).unwrap();
    let entries_before = fs::read_dir(out.join("builds")).unwrap().count();

    // a runtime error: static/ is already copied when the page fails to render
    fs::write(
        dir.join("templates/index.html.j2"),
        "<html><body>{{ missing.field }}</body></html>",
    )
    .unwrap();
    assert!(molde::build(&dir, &out).is_err());

    assert_eq!(molde::testing::list_builds(&out).unwrap(), before);
    assert_eq!(
        fs::read_dir(out.join("builds")).unwrap().count(),
        entries_before,
        "the failed render left a directory in out/builds"
    );
    let diff = molde::diff_builds(&out, None, None, false).unwrap();
    assert_eq!(diff["from"], before[0].as_str());
    assert_eq!(diff["to"], before[1].as_str());
    assert_eq!(diff["removed"].as_array().unwrap().len(), 0);

    let _ = fs::remove_dir_all(&dir);
}
