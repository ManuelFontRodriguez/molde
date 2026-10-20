use crate::build::par_map;
use crate::check::html::{has_noindex, html_files};
use crate::check::read_page;
use crate::site::Page;
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Conformance check 2: every page has a non-empty <title>, and indexable pages have one no
/// other indexable page has. A page carrying a robots noindex meta isn't a search result,
/// so it may share a title (Next.js writes its not-found page three times).
pub fn check_titles(build_dir: &Path) -> Result<Vec<serde_json::Value>> {
    check_titles_scoped(build_dir, &[])
}

/// "First page with this text", scoped by locale: under `locales:` two pages may share
/// a title or description across locales (a product code reads the same in every language),
/// never within one. Unscoped pages — static files, `locales: none` routes, every
/// page of a site without locales — are compared against every scope.
#[derive(Default)]
pub(crate) struct ScopedFirst(BTreeMap<String, Vec<(Option<String>, String)>>);

impl ScopedFirst {
    /// The earlier page with `text` in an overlapping scope; records `page` when none.
    pub(crate) fn first(&mut self, text: &str, scope: Option<&str>, page: &str) -> Option<String> {
        let seen = self.0.entry(text.to_string()).or_default();
        if let Some((_, other)) = seen
            .iter()
            .find(|(s, _)| s.is_none() || scope.is_none() || s.as_deref() == scope)
        {
            return Some(other.clone());
        }
        seen.push((scope.map(String::from), page.to_string()));
        None
    }
}

/// The document with the places a `<title>` is not the page title cut out: SVG
/// and MathML (an icon's accessible name), comments, scripts and templates.
fn without_foreign_titles(html: &str) -> std::borrow::Cow<'_, str> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(
            r"(?is)<!--.*?-->|<svg\b.*?</svg\s*>|<math\b.*?</math\s*>|<script\b.*?</script\s*>|<template\b.*?</template\s*>",
        )
        .unwrap()
    });
    re.replace_all(html, "")
}

/// The page's title as a browser shows it: the first `<title>` outside SVG, MathML,
/// comments, scripts and templates, entities decoded, trimmed. None when it is absent or
/// empty.
pub(crate) fn page_title(html: &str) -> Option<String> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"(?is)<title>(.*?)</title>").unwrap());
    let html = without_foreign_titles(html);
    let title = crate::check::html::unescape_entities(re.captures(&html)?[1].trim());
    (!title.is_empty()).then_some(title)
}

/// `check_titles` with titles.duplicate scoped per locale (see `ScopedFirst`).
pub(crate) fn check_titles_scoped(
    build_dir: &Path,
    pages: &[Page],
) -> Result<Vec<serde_json::Value>> {
    let title_re = regex::Regex::new(r"(?is)<title>(.*?)</title>").unwrap();
    let scope_of: BTreeMap<PathBuf, &str> = pages
        .iter()
        .filter_map(|p| {
            Some((
                PathBuf::from(p.path.trim_start_matches('/')),
                p.locale.as_deref()?,
            ))
        })
        .collect();
    // per page, on every core: how many titles, and the first one
    let files = html_files(build_dir)?;
    let found = par_map(files.len(), |i| {
        let raw = read_page(&build_dir.join(&files[i]))?;
        let html = without_foreign_titles(&raw);
        let count = title_re.find_iter(&html).count();
        let title = title_re
            .captures(&html)
            .map(|cap| cap[1].trim().to_string())
            .unwrap_or_default();
        Ok((count, title, has_noindex(&raw)))
    })?;
    // then across pages, in page order, so `duplicate_of` names the same page
    let mut errors = Vec::new();
    let mut first_page_with = ScopedFirst::default();
    for (page, (count, title, noindex)) in files.iter().zip(found) {
        let page_str = page.display().to_string();
        if count > 1 {
            errors.push(serde_json::json!({
                "layer": "output",
                "check": "titles.multiple",
                "page": page_str,
                "count": count,
            }));
        }
        if title.is_empty() {
            errors.push(serde_json::json!({
                "layer": "output",
                "check": "titles.missing",
                "page": page_str,
            }));
        } else if noindex {
            // neither reported nor remembered: an indexable page that shares a noindex
            // page's title is not a duplicate in search either
        } else if let Some(other) =
            first_page_with.first(&title, scope_of.get(page).copied(), &page_str)
        {
            errors.push(serde_json::json!({
                "layer": "output",
                "check": "titles.duplicate",
                "page": page_str,
                "title": title,
                "duplicate_of": other,
            }));
        }
    }
    Ok(errors)
}
