use crate::build::{loc_to_file, par_map, sitemap_page_locs};
use crate::check::html::{has_noindex, html_files};
use crate::check::read_page;
use crate::routes::is_valid_lastmod;
use crate::site::Page;
use anyhow::Result;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Conformance check 3 (ships with base_url): the sitemap and the page set agree.
/// No sitemap.xml in the build means the capability is off — no errors.
///
/// "The page set" is the indexable HTML: a page is excused from the sitemap when
/// its route says `index: false` (robots.missing polices its meta) or when the
/// file itself carries a robots noindex meta — the one way a static/*.html page
/// (e.g. a hand-written 404) can pass. Listing a noindex page is a contradiction.
pub fn check_sitemap(build_dir: &Path, pages: &[Page]) -> Result<Vec<serde_json::Value>> {
    check_sitemap_at(build_dir, pages, None)
}

/// `check_sitemap` for a site whose `base_url` may carry a path
/// (`https://host/blog`): a `<loc>` maps to a file by dropping exactly that
/// prefix, the inverse of `page_url`. Without a base, everything after
/// the host is the path.
pub fn check_sitemap_at(
    build_dir: &Path,
    pages: &[Page],
    base_url: Option<&str>,
) -> Result<Vec<serde_json::Value>> {
    let base = base_url.map(|b| b.trim_end_matches('/'));
    let Some((locs, missing_children)) = sitemap_page_locs(build_dir, base)? else {
        return Ok(Vec::new());
    };
    let mut errors: Vec<serde_json::Value> = missing_children
        .into_iter()
        .map(|loc| {
            serde_json::json!({
                "layer": "output",
                "check": "sitemap.dangling",
                "loc": loc,
            })
        })
        .collect();
    let mut listed = BTreeSet::new();
    for loc in &locs {
        let rel = loc_to_file(loc, base);
        if let Some(rel) = rel.filter(|rel| build_dir.join(rel).is_file()) {
            listed.insert(PathBuf::from(rel));
        } else {
            errors.push(serde_json::json!({
                "layer": "output",
                "check": "sitemap.dangling",
                "loc": loc,
            }));
        }
    }
    for page in pages.iter().filter(|p| p.in_sitemap()) {
        if let Some(lastmod) = page.lastmod.as_deref().filter(|l| !is_valid_lastmod(l)) {
            errors.push(serde_json::json!({
                "layer": "output",
                "check": "sitemap.lastmod_invalid",
                "page": page.path.trim_start_matches('/'),
                "lastmod": lastmod,
            }));
        }
    }
    let routed_noindex: BTreeSet<PathBuf> = pages
        .iter()
        .filter(|p| !p.index)
        .map(|p| PathBuf::from(p.path.trim_start_matches('/')))
        .collect();
    // each page is judged on its own, so every core takes pages; results in page order
    let files = html_files(build_dir)?;
    let per_page = par_map(files.len(), |i| {
        let page = &files[i];
        let excused = routed_noindex.contains(page);
        let noindex =
            || -> Result<bool> { Ok(excused || has_noindex(&read_page(&build_dir.join(page))?)) };
        Ok(if listed.contains(page) {
            noindex()?.then(|| {
                serde_json::json!({
                    "layer": "output",
                    "check": "sitemap.noindex_listed",
                    "page": page.display().to_string(),
                })
            })
        } else if !noindex()? {
            Some(serde_json::json!({
                "layer": "output",
                "check": "sitemap.missing_page",
                "page": page.display().to_string(),
            }))
        } else {
            None
        })
    })?;
    errors.extend(per_page.into_iter().flatten());
    Ok(errors)
}
