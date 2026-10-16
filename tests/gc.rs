//! gc_builds: old builds are deleted, the newest N and the published one stay.
use std::fs;

#[test]
fn gc_keeps_newest_and_published_and_nothing_outside() {
    let dir = std::env::temp_dir().join(format!("molde-gc-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: i.j2\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    let out = dir.join("out");
    for v in 1..=4 {
        fs::write(
            dir.join("templates/i.j2"),
            format!("<html><head><title>t</title></head><body>v{v}</body></html>"),
        )
        .unwrap();
        molde::build(&dir, &out).unwrap();
    }
    let ids = molde::testing::list_builds(&out).unwrap();
    assert_eq!(ids.len(), 4);
    // the oldest build is the live one (e.g. later builds failed their checks)
    molde::testing::publish(&out, &out.join("builds").join(&ids[0])).unwrap();

    // never followed: a symlink in builds/ pointing outside, and a sibling dir
    fs::create_dir_all(dir.join("precious")).unwrap();
    fs::write(dir.join("precious/keep.txt"), "x").unwrap();
    std::os::unix::fs::symlink(dir.join("precious"), out.join("builds/9")).unwrap();
    // folders that are not builds: never deleted, whatever their age
    fs::create_dir_all(out.join("builds/notes")).unwrap();
    fs::create_dir_all(out.join("builds/backup-2026")).unwrap();
    // a render in progress (fresh) and one abandoned by a crash (old)
    fs::create_dir_all(out.join("builds/.tmp-fresh")).unwrap();
    fs::create_dir_all(out.join("builds/.tmp-old")).unwrap();
    let touched = std::process::Command::new("touch")
        .args(["-t", "202001010000"])
        .arg(out.join("builds/.tmp-old"))
        .status()
        .unwrap();
    assert!(touched.success());

    assert!(molde::gc_builds(&out, 0).is_err());
    let gc = molde::gc_builds(&out, 2).unwrap();
    assert_eq!(gc["removed"], serde_json::json!([ids[1]]));
    assert_eq!(gc["abandoned_removed"], serde_json::json!([".tmp-old"]));
    for id in [&ids[0], &ids[2], &ids[3]] {
        assert!(out.join("builds").join(id).is_dir(), "{id} was removed");
    }
    assert!(!out.join("builds").join(&ids[1]).exists());
    assert!(out.join("builds/.tmp-fresh").is_dir());
    assert!(dir.join("precious/keep.txt").is_file());
    assert!(out.join("builds/notes").is_dir());
    assert!(out.join("builds/backup-2026").is_dir());
    assert!(out.join("current/index.html").is_file(), "live build gone");

    // gc waits for a writer holding the out dir
    let held = molde::testing::lock_out(&out).unwrap();
    let o = out.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let (started_tx, started) = std::sync::mpsc::channel();
    let t = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        tx.send(molde::gc_builds(&o, 2).is_ok()).unwrap()
    });
    // no sleeping: once the worker runs, it has not finished and nothing is removed
    // while we hold the lock
    started.recv().unwrap();
    let builds = |out: &std::path::Path| fs::read_dir(out.join("builds")).unwrap().count();
    let before = builds(&out);
    assert!(rx.try_recv().is_err());
    assert_eq!(builds(&out), before);
    drop(held);
    assert!(rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap());
    t.join().unwrap();

    // a second run has nothing left to do
    let again = molde::gc_builds(&out, 2).unwrap();
    assert_eq!(again["removed"], serde_json::json!([]));

    let _ = fs::remove_dir_all(&dir);
}
