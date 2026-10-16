use crate::build::is_sitemap_file;
use crate::site::{Page, Site};
use std::collections::{BTreeMap, BTreeSet};

/// Two routes (or two items of one foreach) expanding to the same output path:
/// the later write wins and a page silently vanishes, while the build still
/// claims the full page count. The checks that run over surviving files can't
/// see a page that was never written, so this must fire at expansion time.
///
/// The collision key is what actually lands on disk: write_page trims the
/// leading slash, and macOS/Windows dev filesystems are case-insensitive — a
/// pair differing only in slash or case still overwrites silently. One error
/// per colliding path, every offending route named in `routes`; `route` +
/// `template` carry the first offender for generic report consumers. With
/// web-standards on, `/sitemap.xml` is framework-emitted and reserved.
pub fn check_duplicate_paths(site: &Site, pages: &[Page]) -> Vec<serde_json::Value> {
    let key_of = |p: &Page| p.path.trim_start_matches('/').to_lowercase();
    let mut by_key: BTreeMap<String, Vec<&Page>> = BTreeMap::new();
    for page in pages {
        by_key.entry(key_of(page)).or_default().push(page);
    }
    let mut errors = Vec::new();
    for colliding in by_key.values().filter(|c| c.len() > 1) {
        // unique offending routes: a 500-item same-slug collection is one entry
        let mut seen = BTreeSet::new();
        let routes: Vec<serde_json::Value> = colliding
            .iter()
            .filter(|p| seen.insert(p.route))
            .map(|p| {
                serde_json::json!({
                    "route": site.routes.routes[p.route].path.to_string(),
                    "template": p.template,
                    "foreach": site.routes.routes[p.route].foreach,
                    "paginate": site.routes.routes[p.route].paginate,
                })
            })
            .collect();
        errors.push(serde_json::json!({
            "layer": "routes",
            "check": "paths.duplicate",
            "page": colliding[0].path.trim_start_matches('/'),
            "pages_colliding": colliding.len(),
            "route": site.routes.routes[colliding[0].route].path.to_string(),
            "template": colliding[0].template,
            "routes": routes,
        }));
    }
    // one page written as a file where another needs a folder of the same name
    // (`/a` and `/a/b.html`): the disk can hold only one of them, and which write
    // failed would depend on timing, so it is refused before rendering
    let file_keys: BTreeMap<String, &Page> = pages.iter().map(|p| (key_of(p), p)).collect();
    for page in pages {
        let key = key_of(page);
        let mut ancestor = key.as_str();
        while let Some((parent, _)) = ancestor.rsplit_once('/') {
            ancestor = parent;
            if let Some(file) = file_keys.get(ancestor) {
                errors.push(serde_json::json!({
                    "layer": "routes",
                    "check": "paths.file_and_folder",
                    "page": file.path.trim_start_matches('/'),
                    "inside": page.path.trim_start_matches('/'),
                    "route": site.routes.routes[file.route].path.to_string(),
                    "template": file.template,
                }));
                break;
            }
        }
    }
    if site.config.base_url.is_some() {
        // sitemap.xml, and the sitemap-N.xml files a large site splits it into
        for p in pages.iter().filter(|p| is_sitemap_file(&key_of(p))) {
            errors.push(serde_json::json!({
                "layer": "routes",
                "check": "paths.reserved",
                "page": p.path.trim_start_matches('/'),
                "route": site.routes.routes[p.route].path.to_string(),
                "template": p.template,
            }));
        }
    }
    errors
}
