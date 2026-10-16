pub(crate) mod budgets;
pub(crate) mod escaping;
pub(crate) mod head;
pub(crate) mod html;
pub(crate) mod jsonld;
pub(crate) mod links;
pub(crate) mod paths;
pub(crate) mod sitemap;
pub(crate) mod titles;

use crate::build::render_loaded;
use crate::check::budgets::check_budgets;
use crate::check::head::check_head;
use crate::check::jsonld::check_jsonld;
use crate::check::links::check_links;
use crate::check::paths::check_duplicate_paths;
use crate::check::sitemap::check_sitemap_at;
use crate::check::titles::check_titles_scoped;
use crate::locales;
use crate::render::PageFailed;
use crate::routes::expand_routes;
use crate::site::{load_site, Page, Site, SiteFile};
use crate::store::{lock_out, publish};
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::Instant;

/// A built page as text for the checks. Bytes that are not UTF-8 (a hand-written static
/// page in Latin-1, say) become U+FFFD instead of failing the whole check pass: browsers
/// display such pages, and the checks only look for ASCII markup. A read error names the
/// file.
pub(crate) fn read_page(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
    })
}

/// Render + every conformance layer -> the single structured report.
/// All layers always run — the full report is the agent's todo list, not a fail-fast probe.
/// Publishes (atomic swap of `current`) ONLY on pass: a failed check leaves the
/// previous build live — stale-but-consistent beats fresh-but-broken.
/// Exception: routes-layer failures (duplicate output paths) abort BEFORE
/// rendering — the report carries them with `build: null`, nothing publishes.
pub fn run_check(site_dir: &Path, out: &Path) -> Result<serde_json::Value> {
    let _lock = lock_out(out)?;
    // a site-authoring error is a failing report like any other, never bare text:
    // the agent's most frequent mistakes must reach it as structured fields
    let site = match load_site(site_dir) {
        Ok(site) => site,
        Err(e) => return Ok(source_failure(&e, None).unwrap_or_else(|| config_invalid(&e))),
    };
    let pages = match expand_routes(&site) {
        Ok(pages) => pages,
        Err(e) => return Ok(source_failure(&e, Some(&site)).unwrap_or_else(|| config_invalid(&e))),
    };
    let locale_extras = LocaleExtras::new(&site, &pages);
    let dup_errors = routes_layer_errors(&site, &pages);
    if !dup_errors.is_empty() {
        return Ok(locale_extras.add_to(serde_json::json!({
            "verdict": "fail",
            "published": false,
            "build": serde_json::Value::Null,
            "errors": dup_errors,
        })));
    }
    // render from the site and pages loaded above, not a second load of data.json
    let r = match render_loaded(&site, &pages, out, Instant::now()) {
        Ok(r) => r,
        // anything else (disk full, permissions) is the tool failing, not the site
        Err(e) => {
            return source_failure(&e, Some(&site))
                .map(|report| locale_extras.add_to(report))
                .ok_or(e)
        }
    };
    let mut errors = output_layer_errors(&r.build_dir, &site, &pages, &r.unknown_keys)?;
    attach_sources(&mut errors, &site, &pages);
    let pass = errors.is_empty();
    if pass {
        publish(out, &r.build_dir)?;
    }
    Ok(locale_extras.add_to(serde_json::json!({
        "verdict": if pass { "pass" } else { "fail" },
        "published": pass,
        "build": { "pages": r.pages, "assets": r.assets, "ms": r.ms, "dir": r.build_dir.display().to_string() },
        "errors": errors,
    })))
}

/// The routes layer, which runs before anything renders: duplicate output paths and the
/// locale layout of the manifest. Each error carries its hint.
fn routes_layer_errors(site: &Site, pages: &[Page]) -> Vec<serde_json::Value> {
    let mut errors = check_duplicate_paths(site, pages);
    errors.extend(locales::routes_errors(site, pages));
    add_hints(&mut errors);
    errors
}

/// Every check that reads the rendered build, in report order.
fn output_layer_errors(
    build_dir: &Path,
    site: &Site,
    pages: &[Page],
    unknown_keys: &[(String, String, String)],
) -> Result<Vec<serde_json::Value>> {
    let mut errors = check_links(build_dir)?;
    errors.extend(check_titles_scoped(build_dir, pages)?);
    errors.extend(check_sitemap_at(
        build_dir,
        pages,
        site.config.base_url.as_deref(),
    )?);
    errors.extend(check_head(build_dir, site, pages)?);
    errors.extend(check_jsonld(build_dir, site, pages)?);
    errors.extend(check_budgets(build_dir, site)?);
    errors.extend(escaping::check_escaping(build_dir)?);
    errors.extend(locales::output_errors(
        build_dir,
        site,
        pages,
        unknown_keys,
    )?);
    Ok(errors)
}

/// Enrich every page-anchored error with its fixable SOURCE (route + template)
/// and a hint — the report is a todo list, not a symptom log.
fn attach_sources(errors: &mut [serde_json::Value], site: &Site, pages: &[Page]) {
    let by_output: BTreeMap<String, &Page> = pages
        .iter()
        .map(|p| (p.path.trim_start_matches('/').to_string(), p))
        .collect();
    for e in errors.iter_mut() {
        if let Some(hint) = e.get("check").and_then(|c| c.as_str()).and_then(hint_for) {
            e["hint"] = serde_json::json!(hint);
        }
        let source = e
            .get("page")
            .and_then(|v| v.as_str())
            .and_then(|pg| by_output.get(pg))
            .map(|p| {
                (
                    site.routes.routes[p.route].path.to_string(),
                    p.template.clone(),
                )
            });
        if let Some((route, template)) = source {
            e["route"] = serde_json::json!(route);
            e["template"] = serde_json::json!(template);
        }
    }
}

/// Sets each error's `hint` from its check id.
fn add_hints(errors: &mut [serde_json::Value]) {
    for e in errors.iter_mut() {
        if let Some(hint) = e["check"].as_str().and_then(hint_for) {
            e["hint"] = serde_json::json!(hint);
        }
    }
}

/// `pages_by_locale` and `warnings`, which exist only under `locales:` — reports of other
/// sites are unchanged. The counts make a locale the data builder forgot visible.
struct LocaleExtras {
    pages_by_locale: Option<Vec<serde_json::Value>>,
    warnings: Option<Vec<serde_json::Value>>,
}

impl LocaleExtras {
    fn new(site: &Site, pages: &[Page]) -> Self {
        let pages_by_locale = site.config.locales.is_some().then(|| {
            site.routes
                .routes
                .iter()
                .enumerate()
                .filter(|(_, r)| !locales::route_tags(&site.config, r).is_empty())
                .map(|(idx, r)| {
                    serde_json::json!({
                        "route": r.path.to_string(),
                        "pages": locales::counts_by_locale(site, idx, pages),
                    })
                })
                .collect::<Vec<_>>()
        });
        let warnings = site.config.locales.is_some().then(|| {
            let mut w = locales::warnings(site, pages);
            add_hints(&mut w);
            w
        });
        LocaleExtras {
            pages_by_locale,
            warnings,
        }
    }

    fn add_to(&self, mut report: serde_json::Value) -> serde_json::Value {
        if let Some(c) = &self.pages_by_locale {
            report["pages_by_locale"] = serde_json::json!(c);
        }
        if let Some(w) = &self.warnings {
            report["warnings"] = serde_json::json!(w);
        }
        report
    }
}

/// The failing report for one source error: `template.syntax`, `template.render`
/// or `config.parse`, with the file, line, page and route the error carries.
/// None when the error is not a source error.
fn source_failure(e: &anyhow::Error, site: Option<&Site>) -> Option<serde_json::Value> {
    let mut entry = serde_json::json!({ "layer": "source" });
    let mj = e.chain().find_map(|c| c.downcast_ref::<minijinja::Error>());
    let page = e.downcast_ref::<PageFailed>();
    if mj.is_some() || page.is_some() {
        let syntax = mj.is_some_and(|mj| mj.kind() == minijinja::ErrorKind::SyntaxError);
        entry["check"] = serde_json::json!(if syntax {
            "template.syntax"
        } else {
            "template.render"
        });
        // the whole chain: molde's own render errors (a localized field printed
        // without `| loc`, a locale without data) live in the context, not in minijinja
        entry["message"] = serde_json::json!(format!("{e:#}"));
        if let Some(name) = mj.and_then(|mj| mj.name()) {
            entry["template"] = serde_json::json!(name);
        }
        if let Some(line) = mj.and_then(|mj| mj.line()) {
            entry["line"] = serde_json::json!(line);
        }
        if let Some(pf) = page {
            entry["page"] = serde_json::json!(pf.page);
            if let Some(route) = site.and_then(|s| s.routes.routes.get(pf.route)) {
                entry["route"] = serde_json::json!(route.path.to_string());
                entry
                    .as_object_mut()
                    .unwrap()
                    .entry("template")
                    .or_insert_with(|| serde_json::json!(route.template));
            }
        }
    } else {
        let file = e.downcast_ref::<SiteFile>()?;
        entry["check"] = serde_json::json!("config.parse");
        entry["file"] = serde_json::json!(file.0);
        entry["message"] = serde_json::json!(e.root_cause().to_string());
        let line = e.chain().find_map(|c| {
            c.downcast_ref::<serde_yaml::Error>()
                .and_then(|y| y.location())
                .map(|l| l.line())
                .or_else(|| c.downcast_ref::<serde_json::Error>().map(|j| j.line()))
        });
        if let Some(line) = line {
            entry["line"] = serde_json::json!(line);
        }
    }
    Some(failed_report(entry))
}

/// Any other error while loading the site or expanding its routes: a value the
/// router rejects (a missing collection, a bad locale tag). The message names it.
fn config_invalid(e: &anyhow::Error) -> serde_json::Value {
    failed_report(serde_json::json!({
        "layer": "source",
        "check": "config.invalid",
        "message": format!("{e:#}"),
    }))
}

fn failed_report(mut entry: serde_json::Value) -> serde_json::Value {
    if let Some(hint) = entry["check"].as_str().and_then(hint_for) {
        entry["hint"] = serde_json::json!(hint);
    }
    serde_json::json!({
        "verdict": "fail",
        "published": false,
        "build": serde_json::Value::Null,
        "errors": [entry],
    })
}

/// One fix suggestion per check, shipped inside the error itself.
fn hint_for(check: &str) -> Option<&'static str> {
    Some(match check {
        "template.syntax" => {
            "the template does not parse — fix it at `template`:`line` (molde templates are \
             minijinja, Jinja2 syntax; see docs/reference.md#template-contract)"
        }
        "template.render" => {
            "the template parsed but failed while rendering `page` — usually an attribute of \
             an undefined value (`{{ nope.x }}`), a filter given the wrong type, or a missing \
             template; `molde render <page path>` reproduces it in one page"
        }
        "config.parse" => "the file does not parse — fix the syntax at `file`:`line`",
        "config.invalid" => {
            "the site's files parse but describe something the router rejects — `message` \
             names the route or value"
        }
        "paths.duplicate" => {
            "two routes (or two items of one foreach, or a paginate route's pages) expand to the \
             same output path and the later write would silently replace the earlier page — \
             change one route's path (or paginate.path) pattern, or de-overlap the colliding \
             collections/slugs. Paths are compared with \
             the leading slash trimmed and case-insensitively (case-only distinctions collide \
             on macOS/Windows dev filesystems)"
        }
        "paths.file_and_folder" => {
            "this page is written as a file, and another page (`inside`) needs a folder of \
             the same name — the disk can hold only one: give the file page an extension \
             (`/a.html`) or move it inside the folder (`/a/index.html`)"
        }
        "paths.reserved" => {
            "with base_url set, /sitemap.xml is emitted by the framework from the route \
             manifest and would overwrite this page — remove the route (or drop base_url) \
             instead of hand-rendering a sitemap"
        }
        "links.internal" => {
            "no file in the build matches this href — add a route that emits it, or fix the href"
        }
        "titles.missing" => {
            "add a `title` pattern to the route in routes.yaml AND render {{ std_head }} in the \
             template's <head> — std_head is what emits <title>. (A hand-written <title> also \
             works, but only on routes WITHOUT a `title` pattern, or titles.multiple fires.) \
             Note: an undefined field in a title pattern renders as empty"
        }
        "titles.duplicate" => {
            "if this is a foreach route, make its title pattern item-dependent, \
             e.g. \"{{ item.name }} by {{ item.author }}\" (a `localized:` field: \
             `{{ item.name | loc }}`)"
        }
        "html.double_escaped" => {
            "text is escaped twice and shows as `&amp;` (or `&lt;`, …) in the browser — \
             usually data that already holds HTML entities (decode them where data.json is \
             made), or a hand-written `replace(\"&\", \"&amp;\")` or `| e` that autoescape \
             escapes again (remove it)"
        }
        "titles.multiple" => {
            "the route's title pattern and a hand-written <title> are both present — \
             remove the one in the template"
        }
        "canonical.missing" => "render {{ std_head }} inside <head>",
        "canonical.mismatch" | "og.mismatch" => {
            "a hand-written URL drifted — prefer {{ std_head }}, which derives it from the route"
        }
        "canonical.not_in_sitemap" => {
            "canonical and sitemap disagree — regenerate both from the route (render {{ std_head }})"
        }
        "og.missing" => {
            "the route declares a title, so a full OG block is required — \
             render {{ std_head }} instead of hand-writing head tags"
        }
        "sitemap.dangling" => "the sitemap lists a URL no file backs — rebuild, or fix the route",
        "sitemap.missing_page" => {
            "an HTML page is absent from the sitemap — it was written outside the router \
             (static/*.html is never in the sitemap). Route it; or, if it must not be indexed \
             (a 404 page), give it <meta name=\"robots\" content=\"noindex\">"
        }
        "sitemap.noindex_listed" => {
            "the sitemap lists a page that says noindex — a contradictory signal. Remove the \
             hand-written robots meta, or set `index: false` on the route: std_head then emits \
             the meta and the sitemap drops the page"
        }
        "description.missing" => {
            "the route declares a `description` pattern but this page's meta description is \
             empty — a field in the pattern is missing or misspelled (an undefined field \
             renders empty), or the template doesn't render {{ std_head }}"
        }
        "description.duplicate" => {
            "two indexable pages share a meta description — if this is a foreach route, make \
             its description pattern item-dependent, e.g. \"{{ item.name }}: …\""
        }
        "sitemap.lastmod_invalid" => {
            "the route's `lastmod` pattern must render empty (no <lastmod>) or a date: \
             YYYY-MM-DD or an RFC 3339 datetime such as 2026-09-23T09:00:00Z. Derive it from \
             the data (when the page's data last changed), never the build time"
        }
        "robots.missing" => {
            "the route sets `index: false`, so the page must carry \
             <meta name=\"robots\" content=\"noindex\"> — render {{ std_head }} inside <head>, \
             which emits it"
        }
        "jsonld.invalid" => {
            "an application/ld+json block doesn't parse as JSON (`block` counts from 1 on the \
             page) — build the object in the template and emit it with `| tojson` instead of \
             hand-writing JSON, so quoting and escaping are always right"
        }
        "jsonld.type_missing" => {
            "the route declares `jsonld:` types this page's JSON-LD doesn't carry — add an \
             object with that \"@type\" (top level, inside @graph, or nested), or fix a \
             misspelled type in routes.yaml; `found` lists what the page does carry"
        }
        "budget.exceeded" => {
            "the file is over a site.yaml budget — shrink what the template emits (fewer rows, \
             shorter markup, move data to a paginated route) or, if the growth is intended, \
             raise the limit in site.yaml"
        }
        "budget.unmatched" => {
            "no file in the build matches this budget's glob, so it guards nothing — fix the \
             pattern (`*` stays within one path segment, `**` crosses them; no leading `/` \
             matches at any depth) or remove the rule"
        }
        "paths.locale_prefix" => {
            "a root-namespace page's first path segment is a locale prefix, so it would sit \
             inside (and shadow part of) that locale's tree — rename the segment, or move the \
             route into that locale"
        }
        "paths.invalid_chars" => {
            "under locales every path must be plain ASCII URL characters (letters, digits, \
             - . _ ~ /) so every locale's URL is the same string everywhere — transliterate \
             the slug in the data (\"acción\" -> \"accion\")"
        }
        "paths.root_missing" => {
            "with prefix_default_locale every locale lives under a prefix and nothing renders \
             /index.html — add a `locales: none` route at /index.html (a language chooser)"
        }
        "html.lang" => {
            "the page's <html lang> must be its locale — write <html lang=\"{{ locale }}\"> in \
             the base template"
        }
        "hreflang.mismatch" => {
            "the page's hreflang links disagree with the route manifest — render {{ std_head }} \
             (it emits them) and remove hand-written <link rel=\"alternate\" hreflang> tags"
        }
        "translations.missing" => {
            "the default locale's file defines this key and this locale's doesn't (or leaves \
             it empty) — it renders empty until it is added to translations/<locale>.yaml"
        }
        "translations.extra" => {
            "this locale defines a key the default locale doesn't — remove it, or add it to the \
             default locale's file (which defines the key set)"
        }
        "translations.args" => {
            "the translation's {placeholders} differ from the default locale's — use the same \
             names, so `t(key, name=…)` fills every locale"
        }
        "translations.unknown_key" => {
            "a template calls t() with a key the default locale's translations file doesn't \
             define (it rendered empty) — fix the key or add it"
        }
        "i18n.sparse_locale" => {
            "this locale has fewer than half the default locale's pages for the route — a \
             missing locale value means no page, so check that the data builder emits this \
             locale (a warning: the build still publishes)"
        }
        _ => return None,
    })
}
