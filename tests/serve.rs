//! serve --watch / --reload surface: snippet injection is serve-time-only and
//! HTML-only, the build-id endpoint tracks publishes (and only publishes), and
//! the watcher's change detector ignores the out dir and dotfiles.
use std::fs;
use std::path::{Path, PathBuf};

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("molde-serve-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn demo_site() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/demo")
}

/// (status, body) from serve_response's raw HTTP bytes.
fn get(root: &Path, path: &str, reload: bool) -> (u16, Vec<u8>) {
    let raw = molde::testing::serve_response(root, path, reload);
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("no header/body split");
    let head = String::from_utf8_lossy(&raw[..split]);
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("no status");
    (status, raw[split + 4..].to_vec())
}

#[test]
fn reload_snippet_is_serve_time_and_html_only() {
    let out = tmp("inject");
    let report = molde::run_check(&demo_site(), &out).unwrap();
    assert_eq!(report["verdict"], "pass", "{report}");
    let root = out.join("current");

    // without --reload: byte-for-byte the build artifact, no endpoint
    let (status, body) = get(&root, "/", false);
    assert_eq!(status, 200);
    assert!(!body_str(&body).contains("/__molde/build"));
    let (status, _) = get(&root, "/__molde/build", false);
    assert_eq!(
        status, 404,
        "build endpoint must not exist without --reload"
    );

    // with --reload: snippet lands right before </body> — never ON DISK — and
    // seeds its baseline with the id of the build this response came from (a
    // publish landing before the first poll must still reload the tab)
    let (status, body) = get(&root, "/", true);
    assert_eq!(status, 200);
    let html = body_str(&body);
    let snippet = html.find("/__molde/build").expect("snippet missing");
    let closing = html.rfind("</body>").expect("no </body>");
    assert!(snippet < closing, "snippet must be inside <body>");
    let (_, id) = get(&root, "/__molde/build", true);
    assert!(
        html.contains(&format!("'{}'", body_str(&id))),
        "snippet must embed the served build's id as its baseline"
    );
    let on_disk = fs::read_to_string(root.join("index.html")).unwrap();
    assert!(
        !on_disk.contains("/__molde/build"),
        "reload snippet leaked into the build artifact"
    );

    // non-HTML responses are never touched
    let (status, body) = get(&root, "/robots.txt", true);
    assert_eq!(status, 200);
    assert!(!body_str(&body).contains("/__molde/build"));

    // traversal guard unchanged by the reload path
    let (status, _) = get(&root, "/../secrets", true);
    assert_eq!(status, 400);

    // a miss serves the site's /404.html with the real status, never the reload snippet
    let (status, body) = get(&root, "/no/such/page/", true);
    assert_eq!(status, 404);
    let html = body_str(&body);
    assert!(
        html.contains("<title>Page not found — Demo library</title>"),
        "{html}"
    );
    assert!(!html.contains("/__molde/build"));

    let _ = fs::remove_dir_all(&out);
}

#[test]
fn build_id_moves_only_when_a_new_build_publishes() {
    let dir = tmp("buildid");
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: index.html.j2\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    let page =
        |body: &str| format!("<html><head><title>ok</title></head><body>{body}</body></html>");
    fs::write(dir.join("templates/index.html.j2"), page("v1")).unwrap();
    let out = dir.join("out");

    molde::run_check(&dir, &out).unwrap();
    let root = out.join("current");
    let (_, id1) = get(&root, "/__molde/build", true);

    // unchanged site: dedup reuses the build -> no spurious browser refresh
    molde::run_check(&dir, &out).unwrap();
    assert_eq!(get(&root, "/__molde/build", true).1, id1);

    // failing check: previous build stays live, id must NOT move
    fs::write(
        dir.join("templates/index.html.j2"),
        page(r#"<a href="/broken/">v2</a>"#),
    )
    .unwrap();
    let report = molde::run_check(&dir, &out).unwrap();
    assert_eq!(report["verdict"], "fail");
    assert_eq!(get(&root, "/__molde/build", true).1, id1);

    // reverting the breaking edit: bytes equal the PUBLISHED build, so dedup must
    // resolve back to the live id (not the newer unpublished failing build) — a
    // byte-identical re-publish would spuriously reload every tab
    fs::write(dir.join("templates/index.html.j2"), page("v1")).unwrap();
    let report = molde::run_check(&dir, &out).unwrap();
    assert_eq!(report["verdict"], "pass");
    assert_eq!(get(&root, "/__molde/build", true).1, id1);

    // passing edit: id moves -> the polling snippet reloads
    fs::write(dir.join("templates/index.html.j2"), page("v3")).unwrap();
    molde::run_check(&dir, &out).unwrap();
    assert_ne!(get(&root, "/__molde/build", true).1, id1);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn snippet_respects_uppercase_body_tag() {
    let dir = tmp("upper");
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(
        dir.join("routes.yaml"),
        "routes:\n  - path: /index.html\n    template: index.html.j2\n",
    )
    .unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    fs::write(
        dir.join("templates/index.html.j2"),
        "<HTML><HEAD><title>ok</title></HEAD><BODY>v1</BODY></HTML>",
    )
    .unwrap();
    let out = dir.join("out");
    molde::run_check(&dir, &out).unwrap();

    let (status, body) = get(&out.join("current"), "/", true);
    assert_eq!(status, 200);
    let html = body_str(&body);
    let snippet = html.find("/__molde/build").expect("snippet missing");
    let closing = html.rfind("</BODY>").expect("no </BODY>");
    assert!(snippet < closing, "case-insensitive </body> match failed");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn fingerprint_sees_edits_but_not_out_or_dotfiles() {
    let site = tmp("fp");
    let out = site.join("out"); // a common layout: out INSIDE the site dir
    fs::create_dir_all(site.join("templates")).unwrap();
    fs::create_dir_all(&out).unwrap();
    fs::write(site.join("data.json"), "{}").unwrap();

    let base = molde::testing::site_fingerprint(&site, &out);

    fs::write(out.join("anything.html"), "built output").unwrap();
    fs::write(site.join(".DS_Store"), "junk").unwrap();
    fs::create_dir_all(site.join(".git")).unwrap();
    fs::write(site.join(".git/index"), "git state").unwrap();
    assert_eq!(
        molde::testing::site_fingerprint(&site, &out),
        base,
        "out dir / dotfile writes must not look like site changes"
    );

    fs::write(site.join("data.json"), r#"{"changed": true}"#).unwrap();
    assert_ne!(molde::testing::site_fingerprint(&site, &out), base);

    let _ = fs::remove_dir_all(&site);
}

fn body_str(body: &[u8]) -> String {
    String::from_utf8_lossy(body).into_owned()
}

#[test]
fn common_asset_types_get_their_content_type() {
    // Fonts, images and sitemap.xml served as octet-stream looked broken locally
    let root = tmp("ctypes");
    fs::create_dir_all(&root).unwrap();
    let cases = [
        ("sitemap.xml", "application/xml; charset=utf-8"),
        ("f.woff2", "font/woff2"),
        ("i.webp", "image/webp"),
        ("p.JPG", "image/jpeg"),
        ("a.js", "text/javascript; charset=utf-8"),
        ("s.css", "text/css; charset=utf-8"),
        ("site.webmanifest", "application/manifest+json"),
        ("x.unknown", "application/octet-stream"),
    ];
    for (file, ctype) in cases {
        fs::write(root.join(file), b"x").unwrap();
        let raw = molde::testing::serve_response(&root, &format!("/{file}"), false);
        let head = String::from_utf8_lossy(&raw);
        assert!(
            head.contains(&format!("\r\nContent-Type: {ctype}\r\n")),
            "{file}: {head}"
        );
    }
    let _ = fs::remove_dir_all(&root);
}

/// A free high port for a test server (the OS picks it; the listener is dropped).
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// One raw HTTP exchange: (status, body).
fn http(port: u16, request: &str) -> (u16, String) {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    s.write_all(request.as_bytes()).unwrap();
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    let status = out
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let body = out
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or_default();
    (status, body)
}

fn wait_listening(port: u16) {
    for _ in 0..100 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("server on {port} never listened");
}

#[test]
fn serve_survives_an_idle_connection_and_answers_only_localhost() {
    let root = tmp("http-serve");
    fs::create_dir_all(root.join("caf\u{e9}")).unwrap();
    fs::write(root.join("index.html"), "<html>home</html>").unwrap();
    fs::write(root.join("caf\u{e9}/index.html"), "<html>accent</html>").unwrap();
    let port = free_port();
    let r = root.clone();
    std::thread::spawn(move || molde::serve(&r, port, None));
    wait_listening(port);

    // a browser's speculative connection that never sends anything
    let _idle = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    // `http` gives up after its read timeout and reports status 0, so a server blocked
    // by the idle connection fails this assert
    let (status, body) = http(
        port,
        &format!("GET / HTTP/1.1\r\nHost: localhost:{port}\r\n\r\n"),
    );
    assert_eq!((status, body.as_str()), (200, "<html>home</html>"));

    let (status, body) = http(
        port,
        &format!("GET /caf%C3%A9/ HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
    );
    assert_eq!((status, body.as_str()), (200, "<html>accent</html>"));
    // DNS rebinding: the attacker's host name in Host
    let (status, _) = http(
        port,
        &format!("GET / HTTP/1.1\r\nHost: rebind.evil.example:{port}\r\n\r\n"),
    );
    assert_eq!(status, 421);
    let (status, _) = http(
        port,
        &format!("POST / HTTP/1.1\r\nHost: localhost:{port}\r\n\r\n"),
    );
    assert_eq!(status, 405);
    let (status, _) = http(
        port,
        &format!("GET /%2e%2e/%2e%2e/etc/passwd HTTP/1.1\r\nHost: localhost:{port}\r\n\r\n"),
    );
    assert_eq!(status, 400);
    // a request head that arrives in two packets
    {
        use std::io::{Read, Write};
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(b"GET /caf%C3%A9/ HT").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        s.write_all(format!("TP/1.1\r\nHost: localhost:{port}\r\n\r\n").as_bytes())
            .unwrap();
        let mut out = String::new();
        let _ = s.read_to_string(&mut out);
        assert!(out.ends_with("<html>accent</html>"), "{out}");
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn events_refuses_hints_from_web_pages_elsewhere() {
    let out = tmp("http-events");
    let port = free_port();
    let server = std::sync::Arc::new(molde::events::EventsServer::new(
        demo_site(),
        out.clone(),
        std::time::Duration::from_secs(3600),
    ));
    std::thread::spawn(move || server.run(port));
    wait_listening(port);
    let (status, _) = http(
        port,
        &format!("POST /hint HTTP/1.1\r\nHost: localhost:{port}\r\nOrigin: https://evil.example\r\nContent-Length: 0\r\n\r\n"),
    );
    assert_eq!(status, 403);
    // a sandboxed frame or a data: page sends `Origin: null`
    let (status, _) = http(
        port,
        &format!("POST /hint HTTP/1.1\r\nHost: localhost:{port}\r\nOrigin: null\r\nContent-Length: 0\r\n\r\n"),
    );
    assert_eq!(status, 403);
    let (status, _) = http(
        port,
        &format!("GET /status HTTP/1.1\r\nHost: rebind.evil.example:{port}\r\n\r\n"),
    );
    assert_eq!(status, 421);
    let (status, _) = http(
        port,
        &format!("POST /hint HTTP/1.1\r\nHost: localhost:{port}\r\nContent-Length: 0\r\n\r\n"),
    );
    assert_eq!(status, 200);
    let _ = fs::remove_dir_all(&out);
}

/// A miss gets the nearest 404.html up its path (issue #11): a prefixed locale's
/// /de/404.html answers under /de/, the root one everywhere else.
#[test]
fn a_miss_gets_the_nearest_404_page() {
    let root = tmp("nearest404");
    fs::create_dir_all(root.join("de/buecher")).unwrap();
    fs::write(root.join("404.html"), "root 404").unwrap();
    fs::write(root.join("de/404.html"), "de 404").unwrap();
    fs::write(root.join("de/buecher/index.html"), "books").unwrap();
    for (path, body) in [
        ("/missing", "root 404"),
        ("/en/missing/", "root 404"),
        ("/de/missing", "de 404"),
        ("/de/buecher/missing/", "de 404"),
        ("/de/no/such/dir/x.html", "de 404"),
        ("/dex/missing", "root 404"),
    ] {
        assert_eq!(
            get(&root, path, false),
            (404, body.as_bytes().to_vec()),
            "{path}"
        );
    }
    assert_eq!(get(&root, "/de/404.html", false).0, 200);
    fs::remove_file(root.join("404.html")).unwrap();
    assert_eq!(get(&root, "/missing", false), (404, b"not found".to_vec()));
    let _ = fs::remove_dir_all(&root);
}
