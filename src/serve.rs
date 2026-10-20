use crate::check::run_check;
use crate::http;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// `serve --watch` options: rerun the verified check pipeline when the site
/// changes; optionally auto-refresh open browser tabs when a new build publishes.
pub struct WatchOpts {
    pub site: PathBuf,
    pub out: PathBuf,
    pub reload: bool,
}

/// Dev server: serves a directory (typically out/current) on localhost.
/// Deliberately minimal — std-only, GET only, no caching. The human loop.
///
/// With `watch`, site changes rerun `run_check` — a failing check never
/// publishes, so the browser keeps the last good build. With `watch.reload`,
/// served HTML (never build artifacts — those stay byte-deterministic) gets a
/// snippet that polls `/__molde/build` and reloads when the published build id
/// changes; dedup means an unchanged rebuild never causes a spurious refresh.
pub fn serve(root: &Path, port: u16, watch: Option<WatchOpts>) -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .with_context(|| format!("binding 127.0.0.1:{port}"))?;
    let reload = watch.as_ref().is_some_and(|w| w.reload);
    // Some(running) under --watch: whether the startup check is still running
    let first_check = watch.as_ref().map(|_| Arc::new(AtomicBool::new(true)));
    if let Some(w) = watch {
        // baseline BEFORE the initial check: an edit saved while that check runs
        // must register as a change on the first poll, not vanish into the baseline
        let baseline = site_fingerprint(&w.site, &w.out);
        eprintln!(
            "molde serve: watching {} (verified check on change{})",
            w.site.display(),
            if w.reload {
                ", browser auto-reload"
            } else {
                ""
            }
        );
        // one verified build up front, in the background: the last good build is
        // served meanwhile, and a fresh checkout gets a "first build" page, not 404s
        let running = first_check.clone();
        std::thread::spawn(move || {
            log_check(&w);
            if let Some(r) = running {
                r.store(false, Ordering::SeqCst);
            }
            watch_loop(&w, baseline)
        });
    }
    eprintln!("molde serve: {} -> http://127.0.0.1:{port}", root.display());
    let root = root.to_path_buf();
    http::serve_forever(listener, port, move |req| {
        if req.method != "GET" && req.method != "HEAD" {
            return http::response(405, "text/plain; charset=utf-8", b"GET only");
        }
        // the browser sends /caf%C3%A9/ for /café/; decode before any path check
        let Some(path) =
            http::percent_decode_strict(&req.path).filter(|p| !p.contains(['\0', '\\']))
        else {
            return http::response(400, "text/plain; charset=utf-8", b"bad request");
        };
        let mut resp = match &first_check {
            Some(running) => serve_watching(&root, &path, reload, running.load(Ordering::SeqCst)),
            None => serve_response(&root, &path, reload),
        };
        if req.method == "HEAD" {
            if let Some(end) = resp.windows(4).position(|w| w == b"\r\n\r\n") {
                resp.truncate(end + 4);
            }
        }
        resp
    });
    Ok(())
}

/// Full HTTP bytes for one request — the whole serve surface, testable without sockets.
pub fn serve_response(root: &Path, url_path: &str, reload: bool) -> Vec<u8> {
    if reload && url_path == "/__molde/build" {
        return http_response(
            200,
            "text/plain; charset=utf-8",
            current_build_id(root).as_bytes(),
        );
    }
    let (status, ctype, mut body) = respond(root, url_path);
    if reload && status == 200 && ctype.starts_with("text/html") {
        inject_reload_snippet(&mut body, &current_build_id(root));
    }
    http_response(status, ctype, &body)
}

/// Under `serve --watch`: the published build as `serve_response` serves it, or, while
/// there is none yet, a page that says why and reloads itself every second (status 503):
/// the startup check is running, or it failed and the terminal says what to fix.
pub fn serve_watching(
    root: &Path,
    url_path: &str,
    reload: bool,
    first_check_running: bool,
) -> Vec<u8> {
    if root.exists() {
        return serve_response(root, url_path, reload);
    }
    let text = if first_check_running {
        "First build in progress: molde is running the check. This page reloads when it publishes."
    } else {
        "No build yet: the first check did not pass. The terminal running molde serve says \
         what to fix; this page reloads when a build publishes."
    };
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"refresh\" content=\"1\"><title>molde serve</title></head>\
         <body><p>{text}</p></body></html>"
    );
    http_response(503, "text/html; charset=utf-8", body.as_bytes())
}

fn respond(root: &Path, url_path: &str) -> (u16, &'static str, Vec<u8>) {
    let rel = url_path.trim_start_matches('/');
    // path traversal guard
    if rel.split('/').any(|seg| seg == "..") {
        return (400, "text/plain", b"bad request".to_vec());
    }
    let mut file = root.join(rel);
    if file.is_dir() || rel.is_empty() {
        file = file.join("index.html");
    }
    match fs::read(&file) {
        Ok(bytes) => (200, content_type(&file), bytes),
        // the host convention (nginx error_page, S3, Netlify): a site-provided
        // 404.html is the body of every miss, served with the real 404 status. The
        // nearest one up the path wins, so a prefixed locale's /de/404.html answers
        // under /de/ (Cloudflare Pages does the same)
        Err(_) => match not_found_page(root, rel) {
            Some(bytes) => (404, "text/html; charset=utf-8", bytes),
            None => (404, "text/plain", b"not found".to_vec()),
        },
    }
}

/// The `404.html` nearest to `rel`: in its directory, then each parent up to the root.
fn not_found_page(root: &Path, rel: &str) -> Option<Vec<u8>> {
    let mut dir = rel.rsplit_once('/').map_or("", |(d, _)| d);
    loop {
        if let Ok(bytes) = fs::read(root.join(dir).join("404.html")) {
            return Some(bytes);
        }
        if dir.is_empty() {
            return None;
        }
        dir = dir.rsplit_once('/').map_or("", |(d, _)| d);
    }
}

/// The published build's identity: where the `current` symlink points.
/// Publish swaps the symlink; dedup reuses the previous target — so this id
/// changes exactly when the served content does.
fn current_build_id(root: &Path) -> String {
    fs::read_link(root)
        .ok()
        .and_then(|t| t.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "unknown".into())
}

/// Serve-time only: the snippet never lands in build artifacts. The baseline is
/// the id of the build THIS response was served from — seeding from the first
/// poll instead would let a publish landing before that poll pass unnoticed and
/// leave the tab permanently stale.
fn inject_reload_snippet(body: &mut Vec<u8>, build_id: &str) {
    // build ids are nanosecond digits; keep anything else out of the script literal
    let id: String = build_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    let snippet = format!(
        "<script>(()=>{{let b='{id}';setInterval(()=>fetch('/__molde/build',\
{{cache:'no-store'}}).then(r=>r.text()).then(t=>{{if(t!==b)location.reload()}})\
.catch(()=>{{}}),1000)}})()</script>"
    );
    let needle = b"</body>";
    match body
        .windows(needle.len())
        .rposition(|w| w.eq_ignore_ascii_case(needle))
    {
        Some(p) => {
            let tail = body.split_off(p);
            body.extend_from_slice(snippet.as_bytes());
            body.extend_from_slice(&tail);
        }
        None => body.extend_from_slice(snippet.as_bytes()),
    }
}

fn watch_loop(w: &WatchOpts, mut last: BTreeMap<PathBuf, (u64, u128)>) {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let mut fp = site_fingerprint(&w.site, &w.out);
        if fp == last {
            continue;
        }
        // settle until two consecutive polls agree: a multi-file save (editor,
        // git checkout, data refresh) must not be built torn. Bounded: a file
        // that never stops changing (app log, db journal in the site dir) must
        // not silently starve rebuilds forever
        for attempt in 1.. {
            std::thread::sleep(std::time::Duration::from_millis(300));
            let next = site_fingerprint(&w.site, &w.out);
            if next == fp {
                break;
            }
            fp = next;
            if attempt >= 10 {
                eprintln!(
                    "molde serve --watch: site still changing after {attempt} settle polls — \
                     building anyway (a failing check keeps the previous build live)"
                );
                break;
            }
        }
        last = fp;
        log_check(w);
    }
}

fn log_check(w: &WatchOpts) {
    match run_check(&w.site, &w.out) {
        Ok(report) => eprint!("{}", watch_summary(&report)),
        Err(e) => eprintln!("molde serve --watch: check error: {e:#}"),
    }
}

/// How many errors `serve --watch` prints after a failing check.
const WATCH_ERRORS_SHOWN: usize = 5;

/// What `serve --watch` prints after a check: one line for the verdict, then on a fail
/// the first errors with where they are and what is wrong, so the reason a save did not
/// publish is on screen (the browser keeps the last good build).
pub(crate) fn watch_summary(report: &serde_json::Value) -> String {
    let verdict = report["verdict"].as_str().unwrap_or("?");
    let errors = report["errors"].as_array().map_or(&[][..], Vec::as_slice);
    let mut out = if report["build"].is_null() {
        format!(
            "molde serve --watch: {verdict} — build refused, {} error(s)\n",
            errors.len()
        )
    } else {
        format!(
            "molde serve --watch: {verdict} — {} pages in {} ms, {} error(s)\n",
            report["build"]["pages"],
            report["build"]["ms"],
            errors.len()
        )
    };
    if errors.is_empty() {
        return out;
    }
    out.push_str("  not published: the browser keeps the last good build\n");
    for e in errors.iter().take(WATCH_ERRORS_SHOWN) {
        let at = match (e["page"].as_str(), e["file"].as_str(), e["line"].as_u64()) {
            (Some(page), _, _) => format!("/{page}"),
            (None, Some(file), Some(line)) => format!("{file}:{line}"),
            (None, Some(file), None) => file.to_string(),
            (None, None, _) => String::new(),
        };
        let detail = [
            "broken_href",
            "title",
            "description",
            "message",
            "loc",
            "missing",
        ]
        .iter()
        .find_map(|k| {
            let v = &e[*k];
            (!v.is_null()).then(|| match v.as_str() {
                Some(s) => s.to_string(),
                None => v.to_string(),
            })
        })
        .unwrap_or_default();
        // page content goes to a terminal: no control characters (escape sequences)
        let mut detail: String = detail
            .chars()
            .take(160)
            .map(|c| if c.is_control() { '?' } else { c })
            .collect();
        if !detail.is_empty() {
            detail.insert_str(0, "  ");
        }
        out.push_str(&format!(
            "  {} {at}{detail}\n",
            e["check"].as_str().unwrap_or("?")
        ));
    }
    if errors.len() > WATCH_ERRORS_SHOWN {
        out.push_str(&format!(
            "  … and {} more: `molde check` prints the full report\n",
            errors.len() - WATCH_ERRORS_SHOWN
        ));
    }
    out
}

/// What "the site changed" means to the watcher: (len, mtime) per file, skipping
/// dotfiles (.git, editor swap files) and the out dir — in a common layout the
/// out dir lives INSIDE the site dir, and watching our own output would loop.
/// Known limit: a same-length edit within the filesystem's mtime granularity is
/// invisible — the mtime-watcher tradeoff (APFS/ext4 are nanosecond; FAT is not).
pub fn site_fingerprint(site: &Path, out: &Path) -> BTreeMap<PathBuf, (u64, u128)> {
    // resolved once per call, not per entry — this runs twice a second forever
    let site_abs = fs::canonicalize(site).ok();
    let out_abs = fs::canonicalize(out).ok();
    let is_out_dir = |e: &walkdir::DirEntry| {
        let (Some(site_abs), Some(out_abs)) = (&site_abs, &out_abs) else {
            return false;
        };
        e.depth() > 0
            && e.file_type().is_dir()
            && e.path()
                .strip_prefix(site)
                .is_ok_and(|rel| &site_abs.join(rel) == out_abs)
    };
    let mut fp = BTreeMap::new();
    let walker = walkdir::WalkDir::new(site).into_iter().filter_entry(|e| {
        if e.depth() > 0 && e.file_name().to_string_lossy().starts_with('.') {
            return false;
        }
        !is_out_dir(e)
    });
    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        if let (Ok(rel), Ok(md)) = (entry.path().strip_prefix(site), entry.metadata()) {
            let mtime = md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            fp.insert(rel.to_path_buf(), (md.len(), mtime));
        }
    }
    fp
}

fn content_type(path: &Path) -> &'static str {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext.to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "webmanifest" => "application/manifest+json",
        "xml" => "application/xml; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "pdf" => "application/pdf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

fn http_response(status: u16, ctype: &str, body: &[u8]) -> Vec<u8> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        503 => "Service Unavailable",
        _ => "Not Found",
    };
    let mut resp = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    resp.extend_from_slice(body);
    resp
}

#[cfg(test)]
mod watch_summary_text {
    use super::*;

    #[test]
    fn a_failing_check_names_its_errors() {
        let report = serde_json::json!({
            "verdict": "fail", "published": false,
            "build": { "pages": 6, "ms": 4 },
            "errors": [
                { "check": "links.internal", "page": "index.html", "broken_href": "/books/dnue/" },
                { "check": "titles.duplicate", "page": "b.html", "title": "Home" },
                { "check": "a", "page": "1" }, { "check": "b", "page": "2" },
                { "check": "c", "page": "3" }, { "check": "d", "page": "4" }
            ]
        });
        let text = watch_summary(&report);
        assert!(
            text.starts_with("molde serve --watch: fail — 6 pages in 4 ms, 6 error(s)\n"),
            "{text}"
        );
        assert!(
            text.contains("  links.internal /index.html  /books/dnue/\n"),
            "{text}"
        );
        assert!(
            text.contains("  titles.duplicate /b.html  Home\n"),
            "{text}"
        );
        assert!(text.contains("… and 1 more"), "{text}");
        let source = serde_json::json!({
            "verdict": "fail", "build": null,
            "errors": [{ "check": "config.parse", "file": "routes.yaml", "line": 3, "message": "bad" }]
        });
        assert!(watch_summary(&source).contains("  config.parse routes.yaml:3  bad\n"));
        let pass = serde_json::json!({ "verdict": "pass", "build": { "pages": 6, "ms": 4 }, "errors": [] });
        assert_eq!(
            watch_summary(&pass),
            "molde serve --watch: pass — 6 pages in 4 ms, 0 error(s)\n"
        );
    }
}
