use crate::build::{par_map, sitemap_page_locs};
use crate::check::html::{
    attr_re, attr_value, code_re, has_noindex, link_tag_re, meta_tag_re, unescape_entities,
};
use crate::check::read_page;
use crate::check::titles::page_title;
use crate::check::titles::ScopedFirst;
use crate::render::page_url;
use crate::site::{Page, Site};
use anyhow::Result;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::OnceLock;

/// What one page's head checks found: the errors that come before the description's, the
/// meta description itself (for the cross-page duplicate check), and the errors after.
struct PageHead {
    before: Vec<serde_json::Value>,
    description: Option<String>,
    after: Vec<serde_json::Value>,
}

/// Conformance check 4 (ships with base_url): every page's canonical + OG agree
/// with the router. `canonical.missing` + `og.missing` together are the std_head
/// slot check — a page that never rendered it cannot pass both by accident.
/// A page whose route says `index: false` owes the robots noindex meta instead
/// (`robots.missing`); it has no canonical to claim. Non-HTML outputs are skipped.
pub fn check_head(build_dir: &Path, site: &Site, pages: &[Page]) -> Result<Vec<serde_json::Value>> {
    let Some(base) = &site.config.base_url else {
        return Ok(Vec::new());
    };
    // normalised once, so each page's canonical is one set lookup (43k pages × 43k locs
    // compared one by one is minutes)
    let sitemap_locs: BTreeSet<String> =
        sitemap_page_locs(build_dir, Some(base.trim_end_matches('/')))?
            .map(|(locs, _)| locs.iter().map(|l| normal_url(l)).collect())
            .unwrap_or_default();

    // per page, on every core: everything but description.duplicate, which needs the
    // pages before it; errors keep their order within a page and across pages
    let html_pages: Vec<&Page> = pages.iter().filter(|p| p.is_html()).collect();
    let per_page = par_map(html_pages.len(), |i| {
        let page = html_pages[i];
        let rel = page.path.trim_start_matches('/');
        let Ok(raw) = read_page(&build_dir.join(rel)) else {
            return Ok(None); // a missing page is another check's problem
        };
        // head tags are read from the markup only: a tag in a comment, a script, a style
        // or a <template> is not one the browser or a crawler sees
        let html = markup_only(&raw);
        Ok(Some(page_head(&html, base, &sitemap_locs, site, page)))
    })?;

    let mut errors = Vec::new();
    let mut first_page_with = ScopedFirst::default();
    for (page, head) in html_pages.iter().zip(per_page) {
        let Some(head) = head else {
            continue;
        };
        errors.extend(head.before);
        errors.extend(description_errors(
            site,
            page,
            head.description,
            &mut first_page_with,
        ));
        errors.extend(head.after);
    }
    Ok(errors)
}

/// Everything about one page's head that needs only that page: robots, canonical, the
/// Open Graph block, and its meta description.
fn page_head(
    html: &str,
    base: &str,
    sitemap_locs: &BTreeSet<String>,
    site: &Site,
    page: &Page,
) -> PageHead {
    let rel = page.path.trim_start_matches('/');
    let expected = page_url(base, &page.path);
    let mut before = Vec::new();
    if !page.index && !has_noindex(html) {
        before.push(serde_json::json!({
            "layer": "output",
            "check": "robots.missing",
            "page": rel,
        }));
    }
    before.extend(canonical_error(html, rel, page, &expected, sitemap_locs));
    before.extend(duplicate_errors(html, rel));
    if page.index && site.routes.routes[page.route].title.is_some() {
        before.extend(og_missing_errors(html, rel));
    }
    let description = tag_attr(html, meta_tag_re(), "name", "description", "content")
        .flatten()
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty());
    PageHead {
        before,
        description,
        after: og_url_mismatch(html, rel, &expected)
            .into_iter()
            .chain(og_title_mismatch(html, rel))
            .collect(),
    }
}

/// `canonical.duplicate` / `og.duplicate`: a page carries a second canonical link or a
/// second `og:url`, `og:title` or `og:type`. Crawlers then pick one, and nothing says
/// which; the error lists every value `found`, in page order.
fn duplicate_errors(html: &str, rel: &str) -> Vec<serde_json::Value> {
    let mut errors = Vec::new();
    let canonicals = tag_attrs(html, link_tag_re(), "rel", "canonical", "href");
    if canonicals.len() > 1 {
        errors.push(serde_json::json!({
            "layer": "output",
            "check": "canonical.duplicate",
            "page": rel,
            "found": canonicals,
        }));
    }
    for prop in ["og:url", "og:title", "og:type"] {
        let found = tag_attrs(html, meta_tag_re(), "property", prop, "content");
        if found.len() > 1 {
            errors.push(serde_json::json!({
                "layer": "output",
                "check": "og.duplicate",
                "page": rel,
                "property": prop,
                "found": found,
            }));
        }
    }
    errors
}

/// The page without its comments, scripts, styles and `<template>` contents: what a
/// crawler reads as the document's markup.
fn markup_only(html: &str) -> String {
    static TEMPLATE: OnceLock<regex::Regex> = OnceLock::new();
    let template =
        TEMPLATE.get_or_init(|| regex::Regex::new(r"(?is)<template\b.*?</template\s*>").unwrap());
    template
        .replace_all(&code_re().replace_all(html, ""), "")
        .into_owned()
}

/// Text as a browser shows it, for comparing a title with an og:title. Both come in with
/// the basic entities already decoded (`tag_attr`, `page_title`); this adds the common
/// named ones and collapses runs of whitespace.
fn shown_text(s: &str) -> String {
    let mut t = s.to_string();
    for (entity, ch) in [
        ("&nbsp;", " "),
        ("&mdash;", "—"),
        ("&ndash;", "–"),
        ("&hellip;", "…"),
        ("&lsquo;", "‘"),
        ("&rsquo;", "’"),
        ("&ldquo;", "“"),
        ("&rdquo;", "”"),
        ("&laquo;", "«"),
        ("&raquo;", "»"),
        ("&middot;", "·"),
        ("&bull;", "•"),
        ("&copy;", "©"),
        ("&reg;", "®"),
        ("&trade;", "™"),
    ] {
        t = t.replace(entity, ch);
    }
    t.replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `og.title_mismatch`: the page's `og:title` and its `<title>` say different things, so a
/// shared link shows a different name from the search result. Compared as a browser
/// shows them; one containing the other passes (an og:title without the title's
/// "| Brand" suffix is a common, deliberate choice).
fn og_title_mismatch(html: &str, rel: &str) -> Option<serde_json::Value> {
    let og =
        shown_text(&tag_attr(html, meta_tag_re(), "property", "og:title", "content").flatten()?);
    let title = shown_text(&page_title(html)?);
    let (o, t) = (og.to_lowercase(), title.to_lowercase());
    (!o.is_empty() && !o.contains(&t) && !t.contains(&o)).then(|| {
        serde_json::json!({
            "layer": "output",
            "check": "og.title_mismatch",
            "page": rel,
            "found": og,
            "expected": title,
        })
    })
}

/// The canonical link's problem on this page, if it has one.
fn canonical_error(
    html: &str,
    rel: &str,
    page: &Page,
    expected: &str,
    sitemap_locs: &BTreeSet<String>,
) -> Option<serde_json::Value> {
    match tag_attr(html, link_tag_re(), "rel", "canonical", "href").flatten() {
        None if !page.index => None,
        None => Some(serde_json::json!({
            "layer": "output",
            "check": "canonical.missing",
            "page": rel,
            "expected": expected,
        })),
        Some(href) if !same_url(&href, expected) => Some(serde_json::json!({
            "layer": "output",
            "check": "canonical.mismatch",
            "page": rel,
            "found": href,
            "expected": expected,
        })),
        Some(href) if page.index && !sitemap_locs.contains(&normal_url(&href)) => {
            Some(serde_json::json!({
                "layer": "output",
                "check": "canonical.not_in_sitemap",
                "page": rel,
                "url": href,
            }))
        }
        Some(_) => None,
    }
}

/// The same page's URL, as a browser compares them: the scheme and host in any case, a
/// default port (`:443` for https, `:80` for http) the same as none, an empty path the
/// same as `/`; the path itself exactly (servers tell `/A` from `/a`).
fn same_url(a: &str, b: &str) -> bool {
    normal_url(a) == normal_url(b)
}

/// The form `same_url` compares: `HTTPS://Example.com:443` is `https://example.com/`.
pub(crate) fn normal_url(u: &str) -> String {
    let u = u.trim();
    let host_at = u.find("://").map_or(0, |i| i + 3);
    let path_at = u[host_at..].find('/').map_or(u.len(), |i| host_at + i);
    let mut origin = u[..path_at].to_ascii_lowercase();
    for (scheme, port) in [("https://", ":443"), ("http://", ":80")] {
        if origin.starts_with(scheme) && origin.ends_with(port) {
            origin.truncate(origin.len() - port.len());
        }
    }
    let path = &u[path_at..];
    format!("{origin}{}", if path.is_empty() { "/" } else { path })
}

/// One `og.missing` per required Open Graph property the page lacks.
fn og_missing_errors(html: &str, rel: &str) -> Vec<serde_json::Value> {
    ["og:title", "og:url", "og:type"]
        .into_iter()
        .filter(|prop| {
            tag_attr(html, meta_tag_re(), "property", prop, "content")
                .flatten()
                .is_none()
        })
        .map(|prop| {
            serde_json::json!({
                "layer": "output",
                "check": "og.missing",
                "page": rel,
                "property": prop,
            })
        })
        .collect()
}

/// `og.mismatch`: the page has an og:url, and it is not the page's URL.
fn og_url_mismatch(html: &str, rel: &str, expected: &str) -> Option<serde_json::Value> {
    let Some(Some(og_url)) = tag_attr(html, meta_tag_re(), "property", "og:url", "content") else {
        return None;
    };
    (!same_url(&og_url, expected)).then(|| {
        serde_json::json!({
            "layer": "output",
            "check": "og.mismatch",
            "page": rel,
            "found": og_url,
            "expected": expected,
        })
    })
}

/// `description.missing` (the route declares a pattern, the page has none) and
/// `description.duplicate` (an indexable page repeats an earlier page's description).
/// Pages must come in order: the first page with a description is the one others name.
fn description_errors(
    site: &Site,
    page: &Page,
    description: Option<String>,
    first_page_with: &mut ScopedFirst,
) -> Option<serde_json::Value> {
    let rel = page.path.trim_start_matches('/');
    match description {
        None if site.routes.routes[page.route].description.is_some() => Some(serde_json::json!({
            "layer": "output",
            "check": "description.missing",
            "page": rel,
        })),
        Some(d) if page.index => {
            let other = first_page_with.first(&d, page.locale.as_deref(), rel)?;
            Some(serde_json::json!({
                "layer": "output",
                "check": "description.duplicate",
                "page": rel,
                "description": d,
                "duplicate_of": other,
            }))
        }
        _ => None,
    }
}

/// The value of `want` on the first tag matching `tag_re` whose `key` attribute equals
/// `val` (attribute-order-independent: `<link href=".." rel="canonical">` is valid HTML).
/// None: no such tag; Some(None): the tag has no `want`. Attribute names and the `key`
/// value are matched case-insensitively, and values may be double-quoted, single-quoted
/// or unquoted, as browsers read them.
fn tag_attr(
    html: &str,
    tag_re: &regex::Regex,
    key: &str,
    val: &str,
    want: &str,
) -> Option<Option<String>> {
    tag_re
        .find_iter(html)
        .find_map(|m| tag_match(m.as_str(), key, val, want))
}

/// The `want` value of every tag `tag_attr` would consider, in page order (tags without
/// `want` are left out).
fn tag_attrs(html: &str, tag_re: &regex::Regex, key: &str, val: &str, want: &str) -> Vec<String> {
    tag_re
        .find_iter(html)
        .filter_map(|m| tag_match(m.as_str(), key, val, want).flatten())
        .collect()
}

/// One tag: None when its `key` isn't `val`; Some(the `want` value, if it has one).
fn tag_match(tag: &str, key: &str, val: &str, want: &str) -> Option<Option<String>> {
    let mut matched = false;
    let mut found = None;
    for cap in attr_re().captures_iter(tag) {
        let name = &cap[1];
        let value = attr_value(&cap);
        if name.eq_ignore_ascii_case(key) && value.eq_ignore_ascii_case(val) {
            matched = true;
        }
        if name.eq_ignore_ascii_case(want) && found.is_none() {
            found = Some(unescape_entities(value));
        }
    }
    matched.then_some(found)
}
