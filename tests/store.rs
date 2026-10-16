//! One writer per out dir, and a crash's leftover temp link never jams publish.
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

fn site(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("molde-store-{tag}-{}", std::process::id()));
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

#[test]
fn a_second_writer_waits_for_the_first() {
    let dir = site("lock");
    let out = dir.join("out");
    let held = molde::testing::lock_out(&out).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let (started_tx, started) = std::sync::mpsc::channel();
    let (d, o) = (dir.clone(), out.clone());
    let worker = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        let report = molde::run_check(&d, &o).unwrap();
        tx.send(report).unwrap();
    });
    // no sleeping: the worker is running and wants the lock; nothing it could write
    // exists while we hold it, and we say when we let go
    started.recv().unwrap();
    assert!(rx.try_recv().is_err());
    assert!(!out.join("current").exists());
    assert!(!out.join("builds").exists());
    let released = std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos();
    drop(held);
    // …and it runs once released; a build id is the time it started, so a build made
    // while we held the lock would carry an id from before `released`
    let report = rx.recv_timeout(Duration::from_secs(30)).unwrap();
    assert_eq!(report["verdict"], "pass");
    let dir_name = report["build"]["dir"].as_str().unwrap();
    let id: u128 = dir_name.rsplit('/').next().unwrap().parse().unwrap();
    assert!(id >= released, "built at {id}, lock released at {released}");
    worker.join().unwrap();
    assert!(out.join("current/index.html").is_file());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn leftover_temp_link_does_not_block_publish() {
    let dir = site("stale");
    let out = dir.join("out");
    let report = molde::build(&dir, &out).unwrap();
    let id = report
        .build_dir
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    // a run killed between creating the temp link and renaming it
    std::os::unix::fs::symlink("builds/gone", out.join(format!(".current-{id}"))).unwrap();
    molde::testing::publish(&out, &report.build_dir).unwrap();
    assert_eq!(
        fs::read_link(out.join("current")).unwrap(),
        PathBuf::from("builds").join(&id)
    );
    assert!(!out.join(format!(".current-{id}")).exists());
    let _ = fs::remove_dir_all(&dir);
}
