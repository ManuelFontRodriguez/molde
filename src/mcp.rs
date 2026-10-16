//! context module — the site exposed to agents as an MCP server (stdio transport).
//! Hand-rolled JSON-RPC loop over stdin/stdout: newline-delimited messages, no SDK,
//! no async runtime. The design metric is turn parsimony — one tool call returns the
//! complete truth about the site.

use crate::render::{engine, render_page};
use crate::routes::expand_routes;
use crate::site::load_site;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};

/// The longest message the server reads (16 MB); longer lines get a parse error.
const MAX_MESSAGE: usize = 16 * 1024 * 1024;

pub struct McpServer {
    pub site: PathBuf,
    pub out: PathBuf,
}

impl McpServer {
    pub fn run_stdio(&self) -> Result<()> {
        self.serve(std::io::stdin().lock(), std::io::stdout())
    }

    /// One JSON-RPC message per line in, one response per line out, until `input` ends.
    /// A line that is not UTF-8 or not JSON, or longer than `MAX_MESSAGE`, is answered
    /// with a parse error; the server keeps going.
    pub fn serve(&self, mut input: impl BufRead, mut output: impl Write) -> Result<()> {
        let mut line = Vec::new();
        loop {
            line.clear();
            let n = (&mut input)
                .take(MAX_MESSAGE as u64 + 1)
                .read_until(b'\n', &mut line)?;
            if n == 0 {
                return Ok(());
            }
            let parsed = if line.len() > MAX_MESSAGE && !line.ends_with(b"\n") {
                // skip the rest of the oversized line before answering
                let mut rest = Vec::new();
                while (&mut input).take(1 << 20).read_until(b'\n', &mut rest)? > 0 {
                    if rest.ends_with(b"\n") {
                        break;
                    }
                    rest.clear();
                }
                Err(format!("message longer than {MAX_MESSAGE} bytes"))
            } else if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            } else {
                serde_json::from_slice::<Value>(&line).map_err(|e| e.to_string())
            };
            let resp = match parsed {
                Ok(msg) => self.handle(&msg),
                Err(e) => Some(json!({
                    "jsonrpc": "2.0", "id": null,
                    "error": { "code": -32700, "message": format!("parse error: {e}") },
                })),
            };
            if let Some(resp) = resp {
                writeln!(output, "{resp}")?;
                output.flush()?;
            }
        }
    }

    /// Handle one JSON-RPC message. Returns None for notifications (no id).
    pub fn handle(&self, msg: &Value) -> Option<Value> {
        let id = msg.get("id").filter(|v| !v.is_null())?.clone();
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let result = match method {
            "initialize" => Ok(json!({
                "protocolVersion": msg
                    .pointer("/params/protocolVersion")
                    .and_then(|v| v.as_str())
                    .unwrap_or("2025-06-18"),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "molde", "version": env!("CARGO_PKG_VERSION") },
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tool_definitions() })),
            "tools/call" => self.call_tool(msg),
            _ => Err((-32601, format!("method not found: {method}"))),
        };
        Some(match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": code, "message": message },
            }),
        })
    }

    fn call_tool(&self, msg: &Value) -> Result<Value, (i64, String)> {
        let name = msg
            .pointer("/params/name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let args = msg
            .pointer("/params/arguments")
            .cloned()
            .unwrap_or(json!({}));
        const TOOLS: [&str; 5] = [
            "check",
            "list_routes",
            "explain_route",
            "render_page",
            "diff_build",
        ];
        if !TOOLS.contains(&name) {
            return Err((-32602, format!("unknown tool: {name}")));
        }
        // a bug that panics inside one tool call answers that call with an error; it
        // must not take the agent's whole server down
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match name {
            "check" => crate::check::run_check(&self.site, &self.out),
            "list_routes" => self.tool_list_routes(),
            "explain_route" => self.tool_explain_route(&args),
            "render_page" => self.tool_render_page(&args),
            "diff_build" => crate::diff::diff_builds(
                &self.out,
                args.get("from").and_then(|v| v.as_str()),
                args.get("to").and_then(|v| v.as_str()),
                args.get("full").and_then(|v| v.as_bool()).unwrap_or(false),
            ),
            _ => unreachable!("tool names are checked above"),
        }));
        let outcome = run.unwrap_or_else(|panic| {
            let what = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            Err(anyhow::anyhow!(
                "molde hit an internal error running `{name}` (please report it): {what}"
            ))
        });
        // Tool runtime failures go INSIDE the result (isError) so the agent sees them
        // as feedback; only protocol misuse (above) is a JSON-RPC error.
        Ok(match outcome {
            Ok(v) => json!({
                "content": [{ "type": "text", "text": v.to_string() }],
                "isError": false,
            }),
            Err(e) => json!({
                "content": [{ "type": "text", "text": json!({ "error": format!("{e:#}") }).to_string() }],
                "isError": true,
            }),
        })
    }

    fn tool_list_routes(&self) -> Result<Value> {
        list_routes(&self.site)
    }

    fn tool_explain_route(&self, args: &Value) -> Result<Value> {
        let (site, page, normalized) = self.lookup(args)?;
        Ok(match page {
            Some(p) => {
                let mut out = json!({
                "matched": true,
                "path": p.path,
                "route": site.routes.routes[p.route].path,
                "template": p.template,
                "index": p.index,
                "lastmod": p.lastmod,
                "item": p.item.as_deref(),
                // the page's place in its paginated list; `items` (the list slice) and
                // `links` (one per page) are left out — n/pages/prev/next locate it, and
                // render_page shows the full context
                "pagination": p.pagination.as_ref().map(|pg| {
                    let mut pg = serde_json::to_value(pg).unwrap_or(Value::Null);
                    if let Some(o) = pg.as_object_mut() {
                        o.remove("items");
                        o.remove("links");
                    }
                    pg
                }),
                });
                if site.config.locales.is_some() {
                    out["locale"] = json!(p.locale);
                    out["alternates"] = json!(p
                        .alternates
                        .iter()
                        .map(|a| json!({"locale": a.locale, "path": a.path, "index": a.index}))
                        .collect::<Vec<_>>());
                    out["localized_fields"] = json!(site.routes.routes[p.route].localized_fields());
                }
                out
            }
            None => json!({
                "matched": false,
                "normalized": normalized,
                "hint": "no route renders this path — call list_routes for the full surface",
            }),
        })
    }

    fn tool_render_page(&self, args: &Value) -> Result<Value> {
        let (site, page, normalized) = self.lookup(args)?;
        let page = page.with_context(|| format!("no route renders '{normalized}'"))?;
        let env = engine(&site)?;
        let html = render_page(&env, &site, &page)?;
        Ok(json!({ "path": page.path, "template": page.template, "html": html }))
    }

    fn lookup(
        &self,
        args: &Value,
    ) -> Result<(crate::site::Site, Option<crate::site::Page>, String)> {
        let Some(path) = args.get("path").and_then(|v| v.as_str()) else {
            bail!("missing required argument 'path'");
        };
        let normalized = crate::render::normalize_url_path(path);
        let site = load_site(&self.site)?;
        let pages = expand_routes(&site)?;
        let page = pages.into_iter().find(|p| p.path == normalized);
        Ok((site, page, normalized))
    }
}

/// Every route with its expanded pages: the JSON `molde routes` prints and the
/// `list_routes` tool returns.
pub fn list_routes(site_dir: &Path) -> Result<Value> {
    let site = load_site(site_dir)?;
    let pages = expand_routes(&site)?;
    let routes: Vec<Value> = site
        .routes
        .routes
        .iter()
        .enumerate()
        .map(|(idx, r)| {
            let expanded: Vec<&crate::site::Page> =
                pages.iter().filter(|p| p.route == idx).collect();
            let mut route = json!({
                "path": r.path,
                "template": r.template,
                "foreach": r.foreach,
                "group_by": r.group_by,
                "title": r.title,
                "index": r.index.clone().unwrap_or(crate::site::IndexRule::Fixed(true)),
                "description": r.description,
                "lastmod": r.lastmod,
                "jsonld": r.jsonld.as_ref().map(|j| j.to_vec()),
                "paginate": r.paginate,
                "pages": expanded.len(),
                "noindex_pages": expanded.iter().filter(|p| !p.index).count(),
                "examples": expanded.iter().take(3).map(|p| p.path.as_str()).collect::<Vec<_>>(),
            });
            // the locale dimension, only under site.yaml `locales:`
            if site.config.locales.is_some() {
                route["locales"] = match r.locales {
                    Some(crate::site::RouteLocales::None) => json!("none"),
                    _ => json!(crate::locales::route_tags(&site.config, r)),
                };
                route["localized"] = json!(r.localized_fields());
                route["pages_by_locale"] =
                    json!(crate::locales::counts_by_locale(&site, idx, &pages));
            }
            route
        })
        .collect();
    Ok(json!({
        "total_pages": pages.len(),
        "routes": routes,
    }))
}

fn tool_definitions() -> Value {
    let path_schema = json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "URL path, e.g. \"/books/dune/\"" }
        },
        "required": ["path"],
    });
    json!([
        {
            "name": "check",
            "description": "Build the site and run every conformance layer. Returns ONE structured \
                            JSON report: build stats, all errors across all layers, final verdict. \
                            The complete truth about the current state in a single call.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "list_routes",
            "description": "The router is data: every route with its template, foreach collection, \
                            title, description, index and lastmod patterns, required JSON-LD @types, paginate block, expanded page count (and how many are \
                            noindex), and example output paths.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "explain_route",
            "description": "Which route renders a URL path, from which template, with which \
                            snapshot item (or place in a paginated list), whether the page is indexable, and its sitemap lastmod.",
            "inputSchema": path_schema,
        },
        {
            "name": "render_page",
            "description": "Render one page and return its HTML — grep the output instead of \
                            opening a browser.",
            "inputSchema": path_schema,
        },
        {
            "name": "diff_build",
            "description": "Diff two immutable builds (default: the two most recent) — the \
                            review unit for a change. Added/removed/changed pages, with the \
                            first differing line per changed page.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "older build id (default: previous)" },
                    "to": { "type": "string", "description": "newer build id (default: latest)" },
                    "full": { "type": "boolean", "description": "changed lines per page instead of first difference only" }
                },
            },
        },
    ])
}
