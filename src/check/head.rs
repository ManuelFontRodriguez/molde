use crate::build::{par_map, sitemap_page_locs};
use crate::check::html::{has_noindex, link_tag_re, meta_tag_re, unescape_entities};
use crate::check::read_page;
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
    let sitemap_locs: BTreeSet<String> =
        sitemap_page_locs(build_dir, Some(base.trim_end_matches('/')))?
            .map(|(locs, _)| locs.into_iter().collect())
            .unwrap_or_default();

    // per page, on every core: everything but description.duplicate, which needs the
    // pages before it; errors keep their order within a page and across pages
    let html_pages: Vec<&Page> = pages.iter().filter(|p| p.is_html()).collect();
    let per_page = par_map(html_pages.len(), |i| {
        let page = html_pages[i];
        let rel = page.path.trim_start_matches('/');
        let Ok(html) = read_page(&build_dir.join(rel)) else {
            return Ok(None); // a missing page is another check's problem
        };
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
        after: og_url_mismatch(html, rel, &expected).into_iter().collect(),
    }
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
        Some(href) if href != expected => Some(serde_json::json!({
            "layer": "output",
            "check": "canonical.mismatch",
            "page": rel,
            "found": href,
            "expected": expected,
        })),
        Some(href) if page.index && !sitemap_locs.contains(&href) => Some(serde_json::json!({
            "layer": "output",
            "check": "canonical.not_in_sitemap",
            "page": rel,
            "url": href,
        })),
        Some(_) => None,
    }
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
    (og_url != expected).then(|| {
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
/// None: no such tag; Some(None): the tag has no `want`. Only double-quoted values are
/// read here, unlike `has_noindex`: widening it would change which pages pass.
fn tag_attr(
    html: &str,
    tag_re: &regex::Regex,
    key: &str,
    val: &str,
    want: &str,
) -> Option<Option<String>> {
    static ATTR_RE: OnceLock<regex::Regex> = OnceLock::new();
    let attr_re = ATTR_RE.get_or_init(|| regex::Regex::new(r#"([\w:-]+)="([^"]*)""#).unwrap());
    tag_re.find_iter(html).find_map(|m| {
        let mut matched = false;
        let mut found = None;
        for cap in attr_re.captures_iter(m.as_str()) {
            if &cap[1] == key && &cap[2] == val {
                matched = true;
            }
            if &cap[1] == want {
                found = Some(unescape_entities(&cap[2]));
            }
        }
        matched.then_some(found)
    })
}
