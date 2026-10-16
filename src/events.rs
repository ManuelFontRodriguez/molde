//! events module — webhooks are hints, never commands.
//! A hint means "run the verified pull path now instead of at the next poll".
//! The request body is deliberately never read: a forged hint can only trigger a
//! harmless rate-limited rebuild, and run_check never publishes a failing build.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct EventsServer {
    pub site: PathBuf,
    pub out: PathBuf,
    pub min_interval: Duration,
    last: Mutex<Option<(Instant, Value)>>,
}

impl EventsServer {
    pub fn new(site: PathBuf, out: PathBuf, min_interval: Duration) -> Self {
        Self {
            site,
            out,
            min_interval,
            last: Mutex::new(None),
        }
    }

    /// (status, body) for one request — the whole surface, testable without sockets.
    pub fn handle(&self, method: &str, path: &str) -> (u16, Value) {
        match (method, path) {
            ("POST", "/hint") => self.hint(),
            ("GET", "/status") => match &*self
                .last
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
            {
                Some((at, report)) => (
                    200,
                    json!({ "age_s": at.elapsed().as_secs(), "report": report }),
                ),
                None => (200, json!({ "status": "no build this session" })),
            },
            _ => (
                404,
                json!({ "error": "not found — POST /hint or GET /status" }),
            ),
        }
    }

    fn hint(&self) -> (u16, Value) {
        let mut last = self
            .last
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((at, _)) = &*last {
            if at.elapsed() < self.min_interval {
                let wait = self.min_interval - at.elapsed();
                return (
                    429,
                    json!({ "status": "throttled", "retry_after_s": wait.as_secs() + 1 }),
                );
            }
        }
        // 200 whenever the hint was processed — the report carries the verdict;
        // a deterministic conformance failure is not something the sender should retry.
        match crate::check::run_check(&self.site, &self.out) {
            Ok(report) => {
                *last = Some((Instant::now(), report.clone()));
                (200, report)
            }
            Err(e) => {
                let err = json!({ "error": format!("{e:#}") });
                *last = Some((Instant::now(), err.clone()));
                (500, err)
            }
        }
    }

    pub fn run(self: &Arc<Self>, port: u16) -> Result<()> {
        let listener = TcpListener::bind(("127.0.0.1", port))
            .with_context(|| format!("binding 127.0.0.1:{port}"))?;
        eprintln!(
            "molde events: POST http://127.0.0.1:{port}/hint (min interval {}s)",
            self.min_interval.as_secs()
        );
        let server = Arc::clone(self);
        crate::http::serve_forever(listener, port, move |req| {
            // a POST from a web page served elsewhere is refused: a hint is for this
            // machine's tools (curl, a data job), not for any site the browser visits
            if req.method == "POST" && !crate::http::origin_is_local(req.origin.as_deref(), port) {
                return crate::http::response(
                    403,
                    "application/json",
                    br#"{"error":"hints are accepted from this machine only"}"#,
                );
            }
            let (status, body) = server.handle(&req.method, &req.path);
            let body = serde_json::to_vec_pretty(&body).unwrap_or_default();
            crate::http::response(status, "application/json", &body)
        });
        Ok(())
    }
}
