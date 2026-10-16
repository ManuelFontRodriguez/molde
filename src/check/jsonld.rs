use crate::build::par_map;
use crate::check::html::html_files;
use crate::check::read_page;
use crate::site::{Page, Site};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Conformance check: every ld+json block on an HTML page parses (`jsonld.invalid`,
/// always on), and every page of a route that declares `jsonld:` carries each of
/// those @types somewhere in its blocks (`jsonld.type_missing`).
pub(crate) fn check_jsonld(
    build_dir: &Path,
    site: &Site,
    pages: &[Page],
) -> Result<Vec<serde_json::Value>> {
    // `type` must be the attribute itself (not `data-type`) and the MIME type must end
    // at `json` (a `; charset=…` parameter is fine, `ld+json-patch` is not)
    let block_re = regex::Regex::new(
        r#"(?is)<script\s(?:[^>]*\s)?type\s*=\s*["']?application/ld\+json(?:\s*;[^"'>]*)?(?:["'\s][^>]*)?>(.*?)</script"#,
    )
    .unwrap();
    // commented-out blocks are invisible to browsers and crawlers, so to the check too
    let comment_re = regex::Regex::new(r"(?s)<!--.*?-->").unwrap();
    let declared: BTreeMap<String, Vec<String>> = pages
        .iter()
        .filter(|p| p.is_html())
        .filter_map(|p| {
            let types = site.routes.routes[p.route].jsonld.as_ref()?.to_vec();
            Some((p.path.trim_start_matches('/').to_string(), types))
        })
        .collect();
    // each page is judged on its own: every core takes pages, results in page order
    let files = html_files(build_dir)?;
    let per_page = par_map(files.len(), |i| {
        let page = &files[i];
        let mut errors = Vec::new();
        let rel = page.display().to_string();
        let html = read_page(&build_dir.join(page))?;
        let html = comment_re.replace_all(&html, "");
        let mut found = BTreeSet::new();
        for (i, cap) in block_re.captures_iter(&html).enumerate() {
            match serde_json::from_str::<serde_json::Value>(&cap[1]) {
                Ok(v) => collect_types(&v, &mut found),
                Err(e) => errors.push(serde_json::json!({
                    "layer": "output",
                    "check": "jsonld.invalid",
                    "page": rel,
                    "block": i + 1,
                    "error": e.to_string(),
                })),
            }
        }
        if let Some(want) = declared.get(&rel) {
            let missing: Vec<&String> = want.iter().filter(|t| !found.contains(*t)).collect();
            if !missing.is_empty() {
                errors.push(serde_json::json!({
                    "layer": "output",
                    "check": "jsonld.type_missing",
                    "page": rel,
                    "missing": missing,
                    "found": found,
                }));
            }
        }
        Ok(errors)
    })?;
    Ok(per_page.into_iter().flatten().collect())
}

/// Every `@type` in a JSON-LD value, at any depth (a string or a list of strings).
/// schema.org IRIs count as their bare term: `https://schema.org/Book` and
/// `schema:Book` are both `Book`.
fn collect_types(v: &serde_json::Value, out: &mut BTreeSet<String>) {
    let term = |t: &str| {
        ["https://schema.org/", "http://schema.org/", "schema:"]
            .iter()
            .find_map(|p| t.strip_prefix(p))
            .unwrap_or(t)
            .to_string()
    };
    match v {
        serde_json::Value::Object(map) => {
            match map.get("@type") {
                Some(serde_json::Value::String(t)) => {
                    out.insert(term(t));
                }
                Some(serde_json::Value::Array(ts)) => {
                    out.extend(ts.iter().filter_map(|t| t.as_str().map(term)));
                }
                _ => {}
            }
            map.values().for_each(|x| collect_types(x, out));
        }
        serde_json::Value::Array(xs) => xs.iter().for_each(|x| collect_types(x, out)),
        _ => {}
    }
}
