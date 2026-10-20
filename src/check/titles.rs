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
/// and MathML (an icon's accessible name), comments, scripts and templates. SVG and
/// MathML nest (a sprite's `<symbol>` holds an `<svg>`, as Sphinx's Furo theme writes
/// it), so they are cut to their matching end tag, not the first one.
pub(crate) fn without_foreign_titles(html: &str) -> std::borrow::Cow<'_, str> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"(?is)<!--.*?-->|<script\b.*?</script\s*>|<template\b.*?</template\s*>")
            .unwrap()
    });
    let html = re.replace_all(html, "");
    let lower = html.to_ascii_lowercase();
    if !lower.contains("<svg") && !lower.contains("<math") {
        return html;
    }
    std::borrow::Cow::Owned(cut_nested(&cut_nested(&html, "svg"), "math"))
}

/// `html` without each outermost `<tag …>…</tag>` element, counting nested ones of the
/// same name. Tags are read whole, quoted values included, so an `<svg>` written inside
/// an attribute (`content="inline an <svg> icon"`) is not one, and a `>` in a value
/// doesn't end a tag. `<svg/>` closes itself; an element left open runs to the end of
/// the document, as browsers read it.
fn cut_nested(html: &str, tag: &str) -> String {
    let lower = html.to_ascii_lowercase();
    // `<svg` / `</svg` followed by a character that ends a tag name (not `<svgfoo>`)
    let named = |t: &str, prefix: &str| {
        t.strip_prefix(prefix).is_some_and(|rest| {
            rest.chars()
                .next()
                .is_some_and(|c| c.is_ascii_whitespace() || c == '>' || c == '/')
        })
    };
    let (open, close) = (format!("<{tag}"), format!("</{tag}"));
    let mut out = String::with_capacity(html.len());
    let (mut copied, mut at, mut depth, mut cut_from) = (0, 0, 0usize, 0);
    while let Some(k) = lower[at..].find('<').map(|k| at + k) {
        let Some(end) = tag_end(&lower, k) else {
            at = k + 1;
            continue;
        };
        let t = &lower[k..end];
        if named(t, &open) {
            let self_closed = t[..t.len() - 1].trim_end().ends_with('/');
            if depth == 0 {
                cut_from = k;
            }
            if !self_closed {
                depth += 1;
            } else if depth == 0 {
                out.push_str(&html[copied..k]);
                copied = end;
            }
        } else if depth > 0 && named(t, &close) {
            depth -= 1;
            if depth == 0 {
                out.push_str(&html[copied..cut_from]);
                copied = end;
            }
        }
        at = end;
    }
    if depth > 0 {
        out.push_str(&html[copied..cut_from]);
    } else {
        out.push_str(&html[copied..]);
    }
    out
}

/// The end (after `>`) of the tag starting at `at` (`<` and a letter, or `</` and a
/// letter), quoted values taken whole once an `=` opens them; None when `at` starts no
/// tag, or one that never closes.
fn tag_end(lower: &str, at: usize) -> Option<usize> {
    let b = lower.as_bytes();
    let name = if b.get(at + 1) == Some(&b'/') {
        at + 2
    } else {
        at + 1
    };
    if !b.get(name).is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }
    let (mut i, mut after_eq) = (name, false);
    while i < b.len() {
        match b[i] {
            b'>' => return Some(i + 1),
            q @ (b'"' | b'\'') if after_eq => {
                i += lower[i + 1..].find(q as char)? + 2;
                after_eq = false;
                continue;
            }
            b'=' => after_eq = true,
            c if !c.is_ascii_whitespace() => after_eq = false,
            _ => {}
        }
        i += 1;
    }
    None
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

#[cfg(test)]
mod foreign_title_tests {
    use super::*;

    /// Furo's sprite: a `<symbol>` holds an `<svg>`; the icons' titles after the inner
    /// `</svg>` are still the sprite's, not the page's.
    #[test]
    fn nested_svg_titles_are_not_page_titles() {
        let html = "<head><title>Page</title></head><body><svg style=\"display:none\">\
            <symbol id=\"a\"><title>A</title><svg viewBox=\"0 0 1 1\"></svg></symbol>\
            <symbol id=\"b\"><title>B</title></symbol></svg><p>after</p>\
            <math><mrow><math><title>M</title></math><title>N</title></mrow></math></body>";
        let cut = without_foreign_titles(html);
        assert_eq!(cut.matches("<title>").count(), 1, "{cut}");
        assert!(cut.contains("<p>after</p>"), "{cut}");
        assert_eq!(page_title(html).as_deref(), Some("Page"));
        // a self-closing svg and a look-alike tag name
        let html = "<svg/><svgx><title>T</title></svgx>";
        assert_eq!(
            without_foreign_titles(html),
            "<svgx><title>T</title></svgx>"
        );
        // left open: to the end
        assert_eq!(without_foreign_titles("a<svg><title>x</title>"), "a");
        // an <svg> or <math> written inside an attribute value is not an element (the
        // review of the branch: it used to cut the rest of the page)
        let html = r#"<meta name="description" content="Learn to inline an <svg> icon, x <math> y"><title>A</title>"#;
        assert_eq!(page_title(html).as_deref(), Some("A"));
        let html = r#"<a title="<svg>">x</a><title>one</title><title>two</title>"#;
        assert_eq!(without_foreign_titles(html).matches("<title>").count(), 2);
        // a `>` inside a value doesn't end the tag: this <svg/> closes itself
        let html = r#"<svg data-x="a>b" /><title>one</title><title>two</title>"#;
        assert_eq!(without_foreign_titles(html).matches("<title>").count(), 2);
    }
}
