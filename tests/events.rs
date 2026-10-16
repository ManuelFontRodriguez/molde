//! events module: hints trigger the verified pipeline, throttling works, and a
//! failing hint never moves `current`.
use molde::events::EventsServer;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[test]
fn hint_builds_then_throttles() {
    let out = std::env::temp_dir().join(format!("molde-ev-demo-{}", std::process::id()));
    let _ = fs::remove_dir_all(&out);
    let s = EventsServer::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/demo"),
        out.clone(),
        Duration::from_secs(3600),
    );

    let (status, body) = s.handle("POST", "/hint");
    assert_eq!(status, 200, "hint failed: {body}");
    assert_eq!(body["verdict"], "pass");
    assert_eq!(body["published"], true);

    // inside min_interval: throttled, no second build
    let (status, body) = s.handle("POST", "/hint");
    assert_eq!(status, 429, "expected throttle: {body}");
    assert_eq!(body["status"], "throttled");
    assert_eq!(molde::testing::list_builds(&out).unwrap().len(), 1);

    let (status, body) = s.handle("GET", "/status");
    assert_eq!(status, 200);
    assert_eq!(body["report"]["verdict"], "pass");

    let (status, _) = s.handle("GET", "/nope");
    assert_eq!(status, 404);

    let _ = fs::remove_dir_all(&out);
}

#[test]
fn failing_hint_keeps_previous_build_live() {
    let dir = std::env::temp_dir().join(format!("molde-ev-gate-{}", std::process::id()));
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
    let s = EventsServer::new(dir.clone(), out.clone(), Duration::ZERO);

    let (status, body) = s.handle("POST", "/hint");
    assert_eq!(status, 200, "hint failed: {body}");
    assert_eq!(body["published"], true);
    let good = fs::read_link(out.join("current")).unwrap();

    fs::write(
        dir.join("templates/index.html.j2"),
        r#"<html><head><title>ok</title></head><body><a href="/broken/">v2</a></body></html>"#,
    )
    .unwrap();
    let (status, body) = s.handle("POST", "/hint");
    assert_eq!(status, 200, "hint failed: {body}");
    assert_eq!(body["verdict"], "fail");
    assert_eq!(body["published"], false);
    assert_eq!(
        fs::read_link(out.join("current")).unwrap(),
        good,
        "a failing hint went live"
    );

    let _ = fs::remove_dir_all(&dir);
}
