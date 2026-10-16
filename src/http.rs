//! The little HTTP the dev servers (`serve`, `events`) need, made safe to leave running
//! on a laptop: every connection gets its own thread and a timeout, so an idle socket
//! (browsers open them speculatively) cannot freeze the server; the whole request head
//! is read, up to a size cap; and only requests addressed to this machine are answered,
//! so a web page cannot reach the server through DNS rebinding.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A request head: what the dev servers look at. The body, if any, is never read.
pub(crate) struct Request {
    pub method: String,
    /// The path as sent, query and fragment removed, not yet `%`-decoded.
    pub path: String,
    pub host: Option<String>,
    pub origin: Option<String>,
}

const HEAD_LIMIT: usize = 16 * 1024;
const TIMEOUT: Duration = Duration::from_secs(5);
const MAX_CONNECTIONS: usize = 64;

/// Accept connections forever, each on its own thread, answering with `handle`.
/// Requests whose `Host` is not this machine get 421 without reaching `handle`.
pub(crate) fn serve_forever(
    listener: TcpListener,
    port: u16,
    handle: impl Fn(&Request) -> Vec<u8> + Send + Sync + 'static,
) {
    let handle = Arc::new(handle);
    let open = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        if open.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
            continue; // dropped: a dev server never needs this many at once
        }
        let slot = Slot::take(&open);
        let handle = Arc::clone(&handle);
        let spawned = std::thread::Builder::new().spawn(move || {
            let _slot = slot; // given back when the thread ends, even by a panic
            let _ = stream.set_write_timeout(Some(TIMEOUT));
            let response = match read_request(&mut stream) {
                Some(req) if !host_is_local(req.host.as_deref(), port) => response(
                    421,
                    "text/plain; charset=utf-8",
                    b"this server only answers requests addressed to localhost",
                ),
                Some(req) => handle(&req),
                None => response(400, "text/plain; charset=utf-8", b"bad request"),
            };
            let _ = stream.write_all(&response);
        });
        // no thread to be had: that connection is dropped, the server keeps accepting
        drop(spawned);
    }
}

/// One of the `MAX_CONNECTIONS` slots, given back on drop.
struct Slot(Arc<AtomicUsize>);

impl Slot {
    fn take(open: &Arc<AtomicUsize>) -> Self {
        open.fetch_add(1, Ordering::Relaxed);
        Slot(Arc::clone(open))
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Read up to the blank line that ends the head (at most `HEAD_LIMIT` bytes, all of it
/// within `TIMEOUT` however slowly it trickles in) and parse what the servers use. None
/// for anything unreadable, too long or too slow.
pub(crate) fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let deadline = Instant::now() + TIMEOUT;
    let mut head: Vec<u8> = Vec::new();
    let mut buf = [0u8; 2048];
    loop {
        let left = deadline.checked_duration_since(Instant::now())?;
        stream
            .set_read_timeout(Some(left.max(Duration::from_millis(1))))
            .ok()?;
        let n = stream.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        // only the new bytes (and the three before them) can complete the blank line
        let from = head.len().saturating_sub(3);
        head.extend_from_slice(&buf[..n]);
        if head[from..].windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if head.len() > HEAD_LIMIT {
            return None;
        }
    }
    parse_head(&String::from_utf8_lossy(&head))
}

pub(crate) fn parse_head(head: &str) -> Option<Request> {
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split_whitespace();
    let method = first.next()?.to_string();
    let target = first.next()?;
    let path = target.split(['?', '#']).next().unwrap_or("/").to_string();
    let (mut host, mut origin) = (None, None);
    for line in lines.take_while(|l| !l.is_empty()) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "host" => host = Some(value.trim().to_string()),
            "origin" => origin = Some(value.trim().to_string()),
            _ => {}
        }
    }
    Some(Request {
        method,
        path,
        host,
        origin,
    })
}

/// The `Host` a browser sends to this server: `localhost`, `127.0.0.1` or `[::1]`, with
/// this port. A missing `Host` (a plain HTTP/1.0 client) is fine: DNS rebinding always
/// carries the attacker's host name.
pub(crate) fn host_is_local(host: Option<&str>, port: u16) -> bool {
    let Some(host) = host else { return true };
    ["localhost", "127.0.0.1", "[::1]"].iter().any(|name| {
        host.eq_ignore_ascii_case(&format!("{name}:{port}"))
            // browsers leave the default port out of `Host`
            || (port == 80 && host.eq_ignore_ascii_case(name))
    })
}

/// No `Origin` header at all (curl, scripts), or one from a page served by this machine
/// on this port. Anything else is a web page somewhere else asking, including
/// `Origin: null` (sandboxed frames, `data:` and `file:` pages).
pub(crate) fn origin_is_local(origin: Option<&str>, port: u16) -> bool {
    let Some(origin) = origin else { return true };
    origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .is_some_and(|host| host_is_local(Some(host), port))
}

/// `%XX` escapes decoded to the bytes they encode, read as UTF-8. None for an escape
/// that is malformed or decodes to invalid UTF-8.
pub(crate) fn percent_decode_strict(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = |b: u8| (b as char).to_digit(16);
            let (h, l) = (hex(*bytes.get(i + 1)?)?, hex(*bytes.get(i + 2)?)?);
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

pub(crate) fn response(status: u16, ctype: &str, body: &[u8]) -> Vec<u8> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        405 => "Method Not Allowed",
        421 => "Misdirected Request",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "Not Found",
    };
    let mut out = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    out.extend_from_slice(body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heads_and_hosts() {
        let r = parse_head("GET /caf%C3%A9/?x=1 HTTP/1.1\r\nHost: localhost:8080\r\nOrigin: http://evil.example\r\n\r\n").unwrap();
        assert_eq!((r.method.as_str(), r.path.as_str()), ("GET", "/caf%C3%A9/"));
        assert!(host_is_local(r.host.as_deref(), 8080));
        assert!(!origin_is_local(r.origin.as_deref(), 8080));
        assert!(host_is_local(Some("127.0.0.1:8080"), 8080));
        assert!(host_is_local(None, 8080));
        assert!(!host_is_local(Some("rebind.evil.example:8080"), 8080));
        assert!(!host_is_local(Some("localhost:9999"), 8080));
        assert!(origin_is_local(Some("http://localhost:8080"), 8080));
        assert!(origin_is_local(None, 8080));
        assert!(!origin_is_local(Some("null"), 8080));
        assert!(!origin_is_local(Some("file://"), 8080));
        assert!(host_is_local(Some("localhost"), 80));
        assert!(!host_is_local(Some("localhost"), 8080));
        assert_eq!(
            percent_decode_strict("/caf%C3%A9/").as_deref(),
            Some("/café/")
        );
        assert_eq!(percent_decode_strict("/%2e%2e/").as_deref(), Some("/../"));
        assert_eq!(percent_decode_strict("/bad%zz"), None);
        assert_eq!(percent_decode_strict("/%ff"), None);
    }
}
