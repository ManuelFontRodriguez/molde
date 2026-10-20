//! `molde verify <dir>`: molde's output checks on any generator's build
//! (docs/design/verify.md, v2). The build is read once (`scan`), every file classified
//! (`Kind`), and every check works from those facts, so the same page is never judged
//! two ways.
mod scan;
mod url;

use crate::check::head::shown_text;
use crate::check::robots;
use anyhow::Result;
use scan::{Broken, LinkCtx, PageFacts};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use url::{Address, Files, Target};

/// What verify was asked to do (docs/design/verify.md §3).
#[derive(Debug, Default, Clone)]
pub struct VerifyOptions {
    pub base_url: Option<String>,
    pub clean_urls: bool,
    pub ignore: Vec<String>,
    pub spa_fallback: Option<String>,
    pub require_canonical: bool,
    pub og: bool,
    pub sitemap_complete: bool,
    /// Duplicate titles and descriptions among indexable pages are errors (except on
    /// list pages), not warnings.
    pub unique: bool,
}

/// What a file of the build is (§4). Every check asks this first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    /// No `<html`, `<head` or doctype: a verification file, a fragment.
    NotDocument,
    /// `404.html` and the like: hosts serve it with status 404.
    NotFound,
    /// A meta refresh, or a canonical to another page of the site (an alias, a pager).
    Redirect,
    Noindex,
    Indexable,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::NotDocument => "not_document",
            Kind::NotFound => "not_found",
            Kind::Redirect => "redirect",
            Kind::Noindex => "noindex",
            Kind::Indexable => "indexable",
        }
    }
    fn is_document(self) -> bool {
        self != Kind::NotDocument
    }
}

/// Files that mean "this folder is a project, not its build output", and the output
/// folder each generator writes. `package.json` alone is the fallback (`dist/`).
const PROJECT_MARKERS: [(&str, &str); 24] = [
    ("hugo.toml", "public"),
    ("hugo.yaml", "public"),
    ("hugo.json", "public"),
    ("config.toml", "public"),
    ("zola.toml", "public"),
    ("astro.config.mjs", "dist"),
    ("astro.config.ts", "dist"),
    ("astro.config.js", "dist"),
    ("next.config.ts", "out"),
    ("next.config.js", "out"),
    ("next.config.mjs", "out"),
    ("eleventy.config.js", "_site"),
    ("eleventy.config.mjs", "_site"),
    ("eleventy.config.cjs", "_site"),
    (".eleventy.js", "_site"),
    ("docusaurus.config.js", "build"),
    ("docusaurus.config.ts", "build"),
    ("svelte.config.js", "build"),
    ("gatsby-config.js", "public"),
    ("gatsby-config.ts", "public"),
    ("mkdocs.yml", "site"),
    ("mkdocs.yaml", "site"),
    ("_config.yml", "_site"),
    (".vitepress", ".vitepress/dist"),
];

/// `--base-url` as the command line takes it: an http(s) address, a host and an
/// optional path, no query or fragment. The error says what was wrong.
pub fn parse_base_url(s: &str) -> std::result::Result<String, String> {
    match Address::parse(s) {
        Some(_) => Ok(s.to_string()),
        None => Err(format!(
            "`{s}` is not a site address: give http(s), a host and an optional base path, \
             no query or fragment (https://example.com/ or https://user.github.io/repo/)"
        )),
    }
}

/// `--spa-fallback` must name a file of the build: the host serves it for unknown paths,
/// and a fallback that isn't there would excuse every broken page link.
pub fn spa_fallback_error(dir: &Path, fallback: &str) -> Option<String> {
    let rel = fallback.trim_start_matches('/');
    (dir.is_dir() && (rel.is_empty() || !dir.join(rel).is_file())).then(|| {
        format!(
            "--spa-fallback {fallback}: no such file in {}",
            dir.display()
        )
    })
}

pub fn run_verify(dir: &Path, opts: &VerifyOptions) -> Result<Value> {
    let started = std::time::Instant::now();
    if let Some(stop) = refusal(dir) {
        // stopped before reading anything, as check's stops: `build` is null
        let mut r = finish(dir, started, json!({}), 0, vec![stop], vec![], vec![]);
        r["build"] = Value::Null;
        return Ok(r);
    }
    let all = scan::all_files(dir)?;
    let html = scan::html_files(dir)?;
    let files = Files::new(all.iter().cloned(), opts.clean_urls);
    let given = opts.base_url.as_deref().and_then(Address::parse);
    // the same glob rules as size budgets: `*` within a segment, `**` across them
    let ignore: Vec<regex::Regex> = opts
        .ignore
        .iter()
        .map(|p| crate::check::budgets::glob_regex(p))
        .collect();
    let ctx = |site| LinkCtx {
        site,
        files: &files,
        ignore: &ignore,
        spa_fallback: opts.spa_fallback.is_some(),
    };
    let facts = scan::scan(dir, &html, &ctx(given.as_ref()))?;
    let css = scan::css_broken(dir, &all, &ctx(given.as_ref()))?;
    let claimed = claimed_address(&facts, &files);
    let broken: Vec<&[Broken]> = facts.iter().map(|f| f.broken.as_slice()).collect();
    let mut run = judge(
        dir,
        &facts,
        &broken,
        &css,
        &files,
        given.as_ref(),
        opts,
        &ignore,
    )?;
    if run.documents == 0 {
        let e = json!({"layer": "output", "check": "verify.no_pages", "dir": dir.display().to_string()});
        return Ok(finish(
            dir,
            started,
            run.counts,
            all.len(),
            vec![e],
            vec![],
            vec![],
        ));
    }
    if given.is_none() {
        let mut s = json!({"checks": "canonical, sitemap, robots, own-address links", "reason": "no --base-url"});
        if let Some(c) = &claimed {
            let c = c.url_of("/");
            s["suggestion"] = json!(format!(
                "the pages' canonicals use {c}: run with --base-url {c}"
            ));
        }
        run.skipped.push(s);
    }
    // one cause, one error (R2): when the pages themselves claim another address than
    // --base-url, and that address explains most of the errors away, say so once. Errors
    // that remain under both addresses are real whichever is right, and stay.
    // Without --base-url the build is read as served from `/`: a base-path build
    // (`https://user.github.io/repo/`) is the same single cause.
    if let Some(claimed) = &claimed {
        let differs = match &given {
            Some(given) => !url::same_url(&given.url_of("/"), &claimed.url_of("/")),
            None => claimed.base != "/",
        };
        if differs && run.errors.len() >= 3 {
            let html_b = scan::html_broken(dir, &html, &ctx(Some(claimed)))?;
            let css_b = scan::css_broken(dir, &all, &ctx(Some(claimed)))?;
            let broken_b: Vec<&[Broken]> = html_b.iter().map(Vec::as_slice).collect();
            let other = judge(
                dir,
                &facts,
                &broken_b,
                &css_b,
                &files,
                Some(claimed),
                opts,
                &ignore,
            )?;
            if other.errors.len() * 2 < run.errors.len() {
                let key = |e: &Value| format!("{}|{}|{}", e["check"], e["page"], e["broken_href"]);
                let in_both: BTreeSet<String> = other.errors.iter().map(key).collect();
                let mut errors = vec![
                    json!({"layer": "output", "check": "verify.base_url_mismatch",
                    "given": given.as_ref().map(|g| g.url_of("/")), "found": claimed.url_of("/")}),
                ];
                errors.extend(run.errors.into_iter().filter(|e| in_both.contains(&key(e))));
                // the wrong address's warnings (pages "missing" from a sitemap it can't
                // read) are the same cause: only what holds under both stays
                let w_key = |w: &Value| format!("{}|{}", w["check"], w["page"]);
                let w_both: BTreeSet<String> = other.warnings.iter().map(w_key).collect();
                let warnings = run
                    .warnings
                    .into_iter()
                    .filter(|w| w_both.contains(&w_key(w)))
                    .collect();
                return Ok(finish(
                    dir,
                    started,
                    run.counts,
                    all.len(),
                    errors,
                    warnings,
                    run.skipped,
                ));
            }
        }
    }
    Ok(finish(
        dir,
        started,
        run.counts,
        all.len(),
        run.errors,
        run.warnings,
        run.skipped,
    ))
}

/// One verification of the scanned build against one address.
struct Run {
    documents: usize,
    counts: Value,
    errors: Vec<Value>,
    warnings: Vec<Value>,
    skipped: Vec<Value>,
}

#[allow(clippy::too_many_arguments)]
fn judge(
    dir: &Path,
    facts: &[PageFacts],
    broken: &[&[Broken]],
    css: &[(String, Vec<Broken>)],
    files: &Files,
    site: Option<&Address>,
    opts: &VerifyOptions,
    ignore: &[regex::Regex],
) -> Result<Run> {
    let ignored = |site_path: &str| ignore.iter().any(|re| re.is_match(site_path));
    let kinds = classify(facts, site, files, opts.spa_fallback.as_deref());
    let kind_of: BTreeMap<&str, Kind> = facts
        .iter()
        .zip(&kinds)
        .map(|(f, k)| (f.rel.as_str(), *k))
        .collect();
    let mut run = Run {
        documents: kinds.iter().filter(|k| k.is_document()).count(),
        counts: counts(&kinds),
        errors: Vec::new(),
        warnings: Vec::new(),
        skipped: Vec::new(),
    };
    run.errors.extend(links(facts, broken, css));
    let (e, w) = uniqueness(facts, &kinds, opts.unique);
    run.errors.extend(e);
    run.warnings.extend(w);
    run.errors.extend(page_content(facts, &kinds));
    run.errors
        .extend(redirects(facts, &kinds, site, files, &kind_of, &ignored));
    if let Some(site) = site {
        run.errors.extend(canonicals(
            facts, &kinds, site, files, &kind_of, opts, &ignored,
        ));
        let (e, w, s) = sitemap(dir, facts, &kinds, site, files, &kind_of, opts)?;
        run.errors.extend(e);
        run.warnings.extend(w);
        run.skipped.extend(s);
    }
    if opts.og {
        run.errors.extend(open_graph(facts, &kinds));
    } else {
        run.skipped
            .push(json!({"checks": "og", "reason": "Open Graph is opt-in: --og"}));
    }
    run.errors = one_cause(std::mem::take(&mut run.errors), files, opts);
    Ok(run)
}

/// The report, hints attached.
fn finish(
    dir: &Path,
    started: std::time::Instant,
    counts: Value,
    files: usize,
    mut errors: Vec<Value>,
    mut warnings: Vec<Value>,
    skipped: Vec<Value>,
) -> Value {
    for e in errors.iter_mut().chain(warnings.iter_mut()) {
        if let Some(h) = hint(e["check"].as_str().unwrap_or("")) {
            e["hint"] = json!(h);
        }
    }
    report(dir, started, counts, files, errors, warnings, skipped)
}

/// A folder verify must not judge: missing, or a project root rather than its output.
fn refusal(dir: &Path) -> Option<Value> {
    if !dir.is_dir() {
        return Some(
            json!({"layer": "output", "check": "verify.no_pages", "dir": dir.display().to_string(),
                           "message": "not a folder"}),
        );
    }
    let marker = PROJECT_MARKERS
        .iter()
        .find(|(f, _)| dir.join(f).exists())
        .map(|(f, out)| (*f, *out))
        .or_else(|| {
            dir.join("package.json")
                .is_file()
                .then_some(("package.json", "dist"))
        });
    marker.map(|(file, out)| {
        json!({"layer": "output", "check": "verify.project_root", "dir": dir.display().to_string(),
               "found": file, "message": format!("this looks like the project, not its build: verify the output folder (`{out}/` for this generator)")})
    })
}

fn classify(
    facts: &[PageFacts],
    site: Option<&Address>,
    files: &Files,
    spa_fallback: Option<&str>,
) -> Vec<Kind> {
    // the --spa-fallback file is what the host serves for unknown paths: a not-found
    // page by role, often a bare shell with no title. Not the root index.html, the
    // usual fallback, which is the home page too
    let fallback = spa_fallback
        .map(|f| f.trim_start_matches('/').to_ascii_lowercase())
        .filter(|f| f != "index.html");
    facts
        .iter()
        .map(|f| {
            let rel = f.rel.to_ascii_lowercase();
            if !f.is_document {
                Kind::NotDocument
            } else if is_not_found_page(&rel) || fallback.as_deref() == Some(rel.as_str()) {
                Kind::NotFound
            } else if f.refresh.is_some() || canonical_elsewhere(f, site, files) {
                Kind::Redirect
            } else if f.noindex {
                Kind::Noindex
            } else {
                Kind::Indexable
            }
        })
        .collect()
}

/// A page hosts serve for a missing URL: `404.html` at any depth (a locale's own,
/// `fr/404.html`, as Docusaurus and Hugo write them), Next.js's `_not-found`, and the
/// root `200.html` single-page apps fall back to. `rel` is lower-cased.
pub(crate) fn is_not_found_page(rel: &str) -> bool {
    let name = |n: &str| rel == n || rel.ends_with(&format!("/{n}"));
    name("404.html")
        || name("404/index.html")
        || name("_not-found.html")
        || name("_not-found/index.html")
        || rel == "200.html"
        || rel == "__spa-fallback.html"
}

/// Does the page's canonical name another file of this build (an alias, a pager)?
fn canonical_elsewhere(f: &PageFacts, site: Option<&Address>, files: &Files) -> bool {
    let Some(canonical) = f.canonicals.first() else {
        return false;
    };
    match url::resolve(canonical, &url::page_dir(&f.rel, site), None, site) {
        Target::Site(path) => files.file_for(&path).is_some_and(|file| file != f.rel),
        _ => false,
    }
}

/// The address the pages claim: the scheme and host most absolute canonicals use, and
/// the base path they all share when the build has no folder by that name
/// (`https://user.github.io/repo/`). A suggestion and a comparison, never an inference.
fn claimed_address(facts: &[PageFacts], files: &Files) -> Option<Address> {
    let mut votes: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for f in facts {
        if let Some(c) = f.canonicals.first() {
            let Some(at) = c.find("://") else { continue };
            let rest = &c[at + 3..];
            let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
            let origin = format!(
                "{}://{}",
                c[..at].to_ascii_lowercase(),
                rest[..host_end].to_ascii_lowercase()
            );
            votes.entry(origin).or_default().push(&rest[host_end..]);
        }
    }
    let total: usize = votes.values().map(Vec::len).sum();
    let (origin, paths) = votes.into_iter().max_by_key(|(_, p)| p.len())?;
    if paths.len() * 2 <= total || paths.len() < 2 {
        return None;
    }
    // the first path segment every canonical shares, if the build has no such folder
    let first_seg = |p: &str| {
        p.trim_start_matches('/')
            .split(['/', '?', '#'])
            .next()
            .unwrap_or("")
            .to_string()
    };
    let seg = first_seg(paths[0]);
    let shared = !seg.is_empty()
        && paths.iter().all(|p| {
            first_seg(p) == seg && p.trim_start_matches('/').len() > seg.len()
                || p.trim_matches('/') == seg
        })
        && files.file_for(&format!("/{seg}/")).is_none();
    let base = if shared {
        format!("{origin}/{seg}/")
    } else {
        format!("{origin}/")
    };
    Address::parse(&base)
}

fn counts(kinds: &[Kind]) -> Value {
    let mut c: BTreeMap<&str, usize> = BTreeMap::new();
    for k in kinds {
        *c.entry(k.name()).or_default() += 1;
    }
    json!(c)
}

/// links.internal and links.outside_base, from the broken links the scan found, each
/// with the site path it names in `_path` (for `one_cause`). `html` is each page's
/// broken links, in `facts` order.
fn links(facts: &[PageFacts], html: &[&[Broken]], css: &[(String, Vec<Broken>)]) -> Vec<Value> {
    let pages = facts
        .iter()
        .map(|f| f.rel.as_str())
        .zip(html.iter().copied());
    let sheets = css.iter().map(|(rel, b)| (rel.as_str(), b.as_slice()));
    let mut errors = Vec::new();
    for (rel, broken) in pages.chain(sheets) {
        for b in broken {
            errors.push(
                json!({"layer": "output", "check": b.check, "page": rel, "attribute": b.attribute,
                               "broken_href": b.written, "_path": b.path}),
            );
        }
    }
    errors
}

/// The checks whose errors name a missing page of the site (in `_path`).
const MISSING_TARGET: [&str; 4] = [
    "links.internal",
    "canonical.target_missing",
    "redirect.target_missing",
    "sitemap.dangling",
];

/// One cause, one error (§6), over every check that names a missing page: when most of
/// them would resolve with clean URLs, that is one `links.clean_urls` error (`count`,
/// `by_check`, `examples`) instead of each. Then links to the same missing target from
/// three or more pages are one error each. `_path` is dropped.
fn one_cause(errors: Vec<Value>, files: &Files, opts: &VerifyOptions) -> Vec<Value> {
    let path = |e: &Value| e["_path"].as_str().unwrap_or("").to_string();
    let missing = |e: &Value| MISSING_TARGET.contains(&e["check"].as_str().unwrap_or(""));
    let clean = |e: &Value| !opts.clean_urls && missing(e) && files.would_resolve_clean(&path(e));
    let n_missing = errors.iter().filter(|e| missing(e)).count();
    let n_clean = errors.iter().filter(|e| clean(e)).count();
    let mut out = Vec::new();
    let errors = if n_clean >= 3 && n_clean * 2 > n_missing {
        let mut by_check: BTreeMap<String, usize> = BTreeMap::new();
        let mut examples = Vec::new();
        let mut kept = Vec::new();
        for e in errors {
            if clean(&e) {
                *by_check
                    .entry(e["check"].as_str().unwrap_or("").into())
                    .or_default() += 1;
                if examples.len() < 5 {
                    examples.push(json!(path(&e)));
                }
            } else {
                kept.push(e);
            }
        }
        out.push(
            json!({"layer": "output", "check": "links.clean_urls", "count": n_clean,
                        "by_check": by_check, "examples": examples}),
        );
        kept
    } else {
        errors
    };
    let mut internal = Vec::new();
    let mut outside = Vec::new();
    let mut rest = Vec::new();
    for mut e in errors {
        let p = path(&e);
        if let Some(o) = e.as_object_mut() {
            o.remove("_path");
        }
        match e["check"].as_str() {
            Some("links.internal") => internal.push((e, p)),
            Some("links.outside_base") => outside.push((e, p)),
            _ => rest.push(e),
        }
    }
    out.extend(group_by_target(internal));
    out.extend(group_by_target(outside));
    out.extend(rest);
    out
}

/// One cause, one error (§6): links to the same missing page, however each was written,
/// from three or more pages, are reported once: `target` names the page, `count` and
/// `pages` the pages that link to it, and `page` and `broken_href` the first, as in `check`.
fn group_by_target(errors: Vec<(Value, String)>) -> Vec<Value> {
    let mut by_target: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut order = Vec::new();
    for (e, target) in errors {
        if !by_target.contains_key(&target) {
            order.push(target.clone());
        }
        by_target.entry(target).or_default().push(e);
    }
    let mut out = Vec::new();
    for target in order {
        let group = by_target.remove(&target).unwrap_or_default();
        let pages: BTreeSet<String> = group
            .iter()
            .map(|e| e["page"].as_str().unwrap_or("").to_string())
            .collect();
        if pages.len() < 3 {
            out.extend(group);
            continue;
        }
        let mut first = group[0].clone();
        if !target.is_empty() {
            first["target"] = json!(target);
        }
        first["count"] = json!(pages.len());
        first["pages"] = json!(pages);
        out.push(first);
    }
    out
}

/// A page that lists other pages (a pager, a taxonomy page): it may share a title with
/// its siblings without being a duplicate anyone searches for.
fn is_list_page(rel: &str) -> bool {
    let segs: Vec<&str> = rel.split('/').collect();
    let number = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    // `page/2/` (Hugo, Eleventy, Astro) and Jekyll's `page2/`
    let pager = segs.windows(2).any(|w| w[0] == "page" && number(w[1]))
        || segs
            .iter()
            .any(|s| s.strip_prefix("page").is_some_and(number));
    // a taxonomy folder at any depth (`tags/x/`, `fr/tags/x/`, `blog/category/x/`)
    let folders = &segs[..segs.len().saturating_sub(1)];
    let taxonomy = folders.iter().any(|s| {
        matches!(
            *s,
            "tags"
                | "tag"
                | "categories"
                | "category"
                | "topics"
                | "topic"
                | "archive"
                | "archives"
        )
    });
    pager || taxonomy
}

/// titles.duplicate and description.duplicate among indexable documents (§5). By default
/// they are warnings: SEO hygiene, not something broken, and often the theme's choice.
/// With --unique they are errors, except where a list page (a pager, a taxonomy page) is
/// involved. A duplicate names the first holder of the same value, among non-list pages
/// first, so a list page can't hide a real pair.
fn uniqueness(facts: &[PageFacts], kinds: &[Kind], unique: bool) -> (Vec<Value>, Vec<Value>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for (field, check) in [
        ("title", "titles.duplicate"),
        ("description", "description.duplicate"),
    ] {
        let mut first_page: BTreeMap<String, &str> = BTreeMap::new();
        let mut first_list: BTreeMap<String, &str> = BTreeMap::new();
        for (f, k) in facts.iter().zip(kinds) {
            if *k != Kind::Indexable {
                continue;
            }
            let value = if field == "title" {
                Some(f.title.clone()).filter(|t| !t.is_empty())
            } else {
                f.description.clone()
            };
            let Some(value) = value else { continue };
            let list = is_list_page(&f.rel);
            let other = first_page
                .get(&value)
                .copied()
                .map(|o| (o, false))
                .or_else(|| first_list.get(&value).copied().map(|o| (o, true)));
            if let Some((other, other_list)) = other {
                let e = json!({"layer": "output", "check": check, "page": f.rel, field: value, "duplicate_of": other});
                if unique && !list && !other_list {
                    errors.push(e);
                } else {
                    warnings.push(e);
                }
            }
            let firsts = if list {
                &mut first_list
            } else {
                &mut first_page
            };
            firsts.entry(value).or_insert(&f.rel);
        }
    }
    (group_duplicates(errors), group_duplicates(warnings))
}

/// Pages sharing one value with the same first holder, past this many, are one entry.
const DUPLICATES_LISTED: usize = 10;

/// One cause, one entry: a value many pages share (a theme's default description on
/// every post) is reported once, where its first duplicate was, with `count` and the
/// first `examples`, instead of once per page.
fn group_duplicates(found: Vec<Value>) -> Vec<Value> {
    let key = |e: &Value| {
        let value = e.get("title").or(e.get("description")).cloned();
        (
            e["check"].to_string(),
            value.map(|v| v.to_string()),
            e["duplicate_of"].to_string(),
        )
    };
    let mut sizes: BTreeMap<_, usize> = BTreeMap::new();
    for e in &found {
        *sizes.entry(key(e)).or_default() += 1;
    }
    let mut groups: BTreeMap<_, usize> = BTreeMap::new();
    let mut out: Vec<Value> = Vec::new();
    for e in found {
        let k = key(&e);
        let n = sizes[&k];
        if n <= DUPLICATES_LISTED {
            out.push(e);
        } else if let Some(&at) = groups.get(&k) {
            let examples = out[at]["examples"].as_array_mut().unwrap();
            if examples.len() < 5 {
                examples.push(e["page"].clone());
            }
        } else {
            let mut g = e.clone();
            g["count"] = json!(n);
            g["examples"] = json!([e["page"]]);
            groups.insert(k, out.len());
            out.push(g);
        }
    }
    out
}

/// What each document must carry whatever the site's address: one title, text escaped
/// once, JSON-LD that parses, no second canonical or og tag.
fn page_content(facts: &[PageFacts], kinds: &[Kind]) -> Vec<Value> {
    let mut errors = Vec::new();
    for (f, k) in facts.iter().zip(kinds) {
        // a not-found page is held to its links only (§4): hosts serve it as an error,
        // and generators write it in ways that are no one's to fix (Next.js gives it two
        // <title>s)
        if !k.is_document() || *k == Kind::NotFound {
            continue;
        }
        // a noindex page is no one's search result: a frame's content (mdBook's toc.html)
        // or a print view may go without a title
        if f.title.is_empty() && *k != Kind::Noindex {
            errors.push(json!({"layer": "output", "check": "titles.missing", "page": f.rel}));
        }
        if f.title_count > 1 {
            errors.push(json!({"layer": "output", "check": "titles.multiple", "page": f.rel, "count": f.title_count}));
        }
        if let Some((found, context, count)) = &f.double_escaped {
            errors.push(
                json!({"layer": "output", "check": "html.double_escaped", "page": f.rel,
                               "found": found, "context": context, "count": count}),
            );
        }
        for (block, error) in &f.jsonld_errors {
            errors.push(json!({"layer": "output", "check": "jsonld.invalid", "page": f.rel, "block": block, "error": error}));
        }
        if f.canonicals.len() > 1 {
            errors.push(json!({"layer": "output", "check": "canonical.duplicate", "page": f.rel, "found": f.canonicals}));
        }
        for (prop, found) in [
            ("og:url", &f.og_url),
            ("og:title", &f.og_title),
            ("og:type", &f.og_type),
        ] {
            if found.len() > 1 {
                errors.push(json!({"layer": "output", "check": "og.duplicate", "page": f.rel, "property": prop, "found": found}));
            }
        }
    }
    errors
}

/// A redirect's target (§5), with or without --base-url: a meta refresh, or a canonical
/// to another page, must lead to a page of the build, and a canonical must not hand the
/// page's ranking to one that asks not to be indexed. Absolute targets need the address.
fn redirects(
    facts: &[PageFacts],
    kinds: &[Kind],
    site: Option<&Address>,
    files: &Files,
    kind_of: &BTreeMap<&str, Kind>,
    ignored: &dyn Fn(&str) -> bool,
) -> Vec<Value> {
    let mut errors = Vec::new();
    for (f, k) in facts.iter().zip(kinds) {
        if *k != Kind::Redirect {
            continue;
        }
        let by_canonical = f.refresh.is_none();
        let Some(t) = f.refresh.as_ref().or(f.canonicals.first()) else {
            continue;
        };
        let page_dir = url::page_dir(&f.rel, site);
        if let Target::Site(p) = url::resolve(t, &page_dir, None, site) {
            match files.file_for(&p) {
                None if !ignored(&p) => errors.push(
                    json!({"layer": "output", "check": "redirect.target_missing", "page": f.rel, "target": t, "_path": p}),
                ),
                Some(file)
                    if by_canonical
                        && matches!(
                            kind_of.get(file.as_str()),
                            Some(Kind::Noindex | Kind::NotFound)
                        ) =>
                {
                    errors.push(json!({"layer": "output", "check": "canonical.target_noindex", "page": f.rel, "found": t, "target": file}))
                }
                _ => {}
            }
        }
    }
    errors
}

/// The canonicals of pages that aren't redirects, judged (§5): off the site, to a missing
/// page, to a noindex page; and with --require-canonical, a missing one.
fn canonicals(
    facts: &[PageFacts],
    kinds: &[Kind],
    site: &Address,
    files: &Files,
    kind_of: &BTreeMap<&str, Kind>,
    opts: &VerifyOptions,
    ignored: &dyn Fn(&str) -> bool,
) -> Vec<Value> {
    let mut errors = Vec::new();
    for (f, k) in facts.iter().zip(kinds) {
        if !k.is_document() || *k == Kind::NotFound {
            continue;
        }
        if *k == Kind::Redirect {
            continue;
        }
        let page_dir = url::page_dir(&f.rel, Some(site));
        let Some(canonical) = f.canonicals.first() else {
            if opts.require_canonical && *k == Kind::Indexable {
                errors.push(json!({"layer": "output", "check": "canonical.missing", "page": f.rel,
                                   "expected": site.url_of(&url::site_path_of(&f.rel, opts.clean_urls))}));
            }
            continue;
        };
        match url::resolve(canonical, &page_dir, None, Some(site)) {
            Target::Site(p) => match files.file_for(&p) {
                None if !ignored(&p) => errors.push(json!({"layer": "output", "check": "canonical.target_missing", "page": f.rel, "found": canonical, "_path": p})),
                // a page naming itself is not pointing anywhere, noindex or not
                Some(file) if file != f.rel && matches!(kind_of.get(file.as_str()), Some(Kind::Noindex | Kind::NotFound)) => {
                    errors.push(json!({"layer": "output", "check": "canonical.target_noindex", "page": f.rel, "found": canonical, "target": file}))
                }
                _ => {}
            },
            Target::OutsideBase(_) | Target::AboveRoot => {
                errors.push(json!({"layer": "output", "check": "canonical.off_host", "page": f.rel, "found": canonical}))
            }
            Target::Elsewhere if canonical.contains("://") || canonical.starts_with("//") => {
                errors.push(json!({"layer": "output", "check": "canonical.off_host", "page": f.rel, "found": canonical}))
            }
            Target::Elsewhere => {}
        }
    }
    errors
}

/// The sitemap, found as §7 says, against the pages; and robots.txt.
#[allow(clippy::type_complexity)]
fn sitemap(
    dir: &Path,
    facts: &[PageFacts],
    kinds: &[Kind],
    site: &Address,
    files: &Files,
    kind_of: &BTreeMap<&str, Kind>,
    opts: &VerifyOptions,
) -> Result<(Vec<Value>, Vec<Value>, Vec<Value>)> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut skipped = Vec::new();
    let robots_text = std::fs::read(dir.join("robots.txt"))
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned());
    let robots_lines = robots_text
        .as_deref()
        .map(robots::parse)
        .unwrap_or_default();
    let named: Vec<String> = robots_lines
        .iter()
        .filter(|(k, _)| k == "sitemap")
        .map(|(_, v)| v.clone())
        .collect();
    let file_of_url = |u: &str| match url::resolve(u, &site.base, None, Some(site)) {
        Target::Site(p) => files.file_for(&p),
        _ => None,
    };
    let entry = named.iter().find_map(|u| file_of_url(u)).or_else(|| {
        ["sitemap.xml", "sitemap-index.xml", "sitemap_index.xml"]
            .iter()
            .map(|s| s.to_string())
            .find(|s| dir.join(s).is_file())
    });
    // robots.txt: a Sitemap line must name a sitemap that is in the build, and with a
    // sitemap in the build, one must name it (a robots.txt without Sitemap lines is valid
    // when there is no sitemap: that is sitemap.not_found's warning); and let crawlers in
    let indexable = kinds.contains(&Kind::Indexable);
    if robots_text.is_some() {
        let names_one = named.iter().any(|u| file_of_url(u).is_some());
        // a Sitemap line naming nothing in the build is an error; no Sitemap line at all,
        // next to a sitemap, is valid (crawlers also take it from a search console, and
        // Hugo's and Docsy's default robots.txt has none): a warning
        if !names_one && (!named.is_empty() || entry.is_some()) {
            let e = json!({"layer": "output", "check": "robots.sitemap_missing", "file": "robots.txt",
                           "expected": entry.as_ref().map(|e| site.url_of(&format!("/{e}"))), "found": named});
            if named.is_empty() {
                warnings.push(e);
            } else {
                errors.push(e);
            }
        }
        if indexable && robots::blocks_everyone(&robots_lines) {
            errors.push(
                json!({"layer": "output", "check": "robots.blocks_all", "file": "robots.txt"}),
            );
        }
    } else {
        skipped.push(json!({"checks": "robots", "reason": "no robots.txt in the build"}));
    }
    let Some(entry) = entry else {
        warnings.push(json!({"layer": "output", "check": "sitemap.not_found"}));
        skipped.push(json!({"checks": "sitemap", "reason": "no sitemap in the build"}));
        return Ok((errors, warnings, skipped));
    };
    let locs = sitemap_locs(dir, &entry, &|u: &str| file_of_url(u));
    let mut listed: BTreeSet<String> = BTreeSet::new();
    for loc in &locs {
        match url::resolve(loc, &site.base, None, Some(site)) {
            Target::Site(p) => match files.file_for(&p) {
                None => {
                    // a loc that resolves only with clean URLs still names its page: the
                    // cause is links.clean_urls's, not a page missing from the sitemap
                    if !opts.clean_urls && files.would_resolve_clean(&p) {
                        listed.insert(format!("{}.html", p.trim_matches('/')));
                    }
                    errors.push(json!({"layer": "output", "check": "sitemap.dangling", "loc": loc, "_path": p}))
                }
                Some(file) => {
                    match kind_of.get(file.as_str()) {
                        Some(Kind::Noindex | Kind::NotFound) => {
                            errors.push(json!({"layer": "output", "check": "sitemap.noindex_listed", "page": file, "loc": loc}))
                        }
                        Some(Kind::Redirect) => {
                            errors.push(json!({"layer": "output", "check": "sitemap.non_canonical_listed", "page": file, "loc": loc}))
                        }
                        _ => {}
                    }
                    listed.insert(file);
                }
            },
            _ => errors.push(json!({"layer": "output", "check": "sitemap.dangling", "loc": loc, "reason": "not on this site"})),
        }
    }
    for (f, k) in facts.iter().zip(kinds) {
        if *k == Kind::Indexable && !listed.contains(&f.rel) {
            let e = json!({"layer": "output", "check": "sitemap.missing_page", "page": f.rel});
            if opts.sitemap_complete {
                errors.push(e);
            } else {
                warnings.push(e);
            }
        }
    }
    Ok((errors, warnings, skipped))
}

/// The page `<loc>`s of a sitemap file, following a sitemap index into its children
/// (namespaced tags and CDATA allowed).
fn sitemap_locs(
    dir: &Path,
    entry: &str,
    file_of_url: &dyn Fn(&str) -> Option<String>,
) -> Vec<String> {
    // a page's <loc>, namespaced or not, but not an extension's (<image:loc> names an
    // image, often on a CDN, not a page of the site)
    let loc_re =
        regex::Regex::new(r"(?s)<(\w+:)?loc>\s*(?:<!\[CDATA\[)?(.*?)(?:\]\]>)?\s*</(?:\w+:)?loc>")
            .unwrap();
    let read = |rel: &str| {
        std::fs::read(dir.join(rel))
            .ok()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    };
    let locs = |xml: &str| -> Vec<String> {
        loc_re
            .captures_iter(xml)
            .filter(|c| {
                !c.get(1).is_some_and(|p| {
                    ["image:", "video:", "news:", "xhtml:"]
                        .iter()
                        .any(|x| p.as_str().eq_ignore_ascii_case(x))
                })
            })
            .map(|c| crate::check::html::unescape_entities(c[2].trim()))
            .collect()
    };
    let Some(xml) = read(entry) else {
        return Vec::new();
    };
    if !xml.contains("sitemapindex") {
        return locs(&xml);
    }
    let mut out = Vec::new();
    for child in locs(&xml) {
        if let Some(child_xml) = file_of_url(&child).and_then(|f| read(&f)) {
            out.extend(locs(&child_xml));
        }
    }
    out
}

/// With --og: the full block on indexable documents, and an og:title that agrees with the title.
fn open_graph(facts: &[PageFacts], kinds: &[Kind]) -> Vec<Value> {
    let mut errors = Vec::new();
    for (f, k) in facts.iter().zip(kinds) {
        if *k != Kind::Indexable {
            continue;
        }
        for (prop, found) in [
            ("og:title", &f.og_title),
            ("og:url", &f.og_url),
            ("og:type", &f.og_type),
        ] {
            if found.is_empty() {
                errors.push(json!({"layer": "output", "check": "og.missing", "page": f.rel, "property": prop}));
            }
        }
        if let (Some(og), false) = (f.og_title.first(), f.title.is_empty()) {
            let (o, t) = (
                shown_text(og).to_lowercase(),
                shown_text(&f.title).to_lowercase(),
            );
            if !o.is_empty() && !o.contains(&t) && !t.contains(&o) {
                errors.push(
                    json!({"layer": "output", "check": "og.title_mismatch", "page": f.rel,
                                   "found": shown_text(og), "expected": shown_text(&f.title)}),
                );
            }
        }
    }
    errors
}

fn hint(check: &str) -> Option<&'static str> {
    Some(match check {
        "links.internal" => "the page links to a file the build doesn't have: fix the link, or the page it should reach is missing",
        "links.outside_base" => "a root-relative link that skips the site's base path: it works on disk, but in production it leaves the site; write it relative, or with the base path",
        "links.clean_urls" => "most missing pages (links, canonicals, redirects, sitemap entries: `by_check`) would resolve if the host serves /a from a.html: if yours does (Netlify, Vercel, Cloudflare Pages, GitHub Pages), run with --clean-urls",
        "titles.missing" => "the page has no <title>, or an empty one",
        "titles.multiple" => "the page has more than one <title>",
        "titles.duplicate" => "two indexable pages share a title, so search results can't tell them apart",
        "description.duplicate" => "two indexable pages share a meta description",
        "html.double_escaped" => "text escaped twice shows entities to readers (`&amp;` on screen): the text was escaped before the template escaped it again",
        "jsonld.invalid" => "a JSON-LD block doesn't parse; build it from data with the generator's JSON serializer, not by hand",
        "canonical.duplicate" | "og.duplicate" => "the page has two of a tag that must be single (`found` lists them): remove one",
        "canonical.off_host" => "the canonical points to another site (or outside the base path): search engines will index that address instead",
        "canonical.target_missing" => "the canonical names a page this build doesn't have",
        "canonical.target_noindex" => "the canonical names a page that asks not to be indexed: neither page will be",
        "canonical.missing" => "an indexable page without a canonical (required by --require-canonical)",
        "redirect.target_missing" => "a redirect (meta refresh or alias) leads to a page this build doesn't have",
        "sitemap.dangling" => "the sitemap lists a URL with no file behind it",
        "sitemap.noindex_listed" => "the sitemap lists a page that asks not to be indexed",
        "sitemap.non_canonical_listed" => "the sitemap lists a page whose canonical is another page: list the canonical one",
        "sitemap.missing_page" => "an indexable page the sitemap doesn't list (a warning unless --sitemap-complete)",
        "sitemap.not_found" => "no sitemap in the build (robots.txt names none, and there is no sitemap.xml or sitemap-index.xml)",
        "robots.sitemap_missing" => "robots.txt names a sitemap the build doesn't have (an error), or names none while the build has one (a warning: add a `Sitemap:` line so every crawler finds it)",
        "robots.blocks_all" => "robots.txt shuts every crawler out while the site has pages to index",
        "og.missing" => "the page lacks part of the Open Graph block (required by --og)",
        "og.title_mismatch" => "og:title and <title> say different things",
        "verify.no_pages" => "no HTML documents in this folder: is the build empty, or is this the wrong folder?",
        "verify.base_url_mismatch" => "the site's own canonicals use another address than --base-url (or a base path, and no --base-url was given): pass the address in `found` (or rebuild the site for the one you gave)",
        "verify.project_root" => "this is the project's folder: run verify on its build output",
        _ => return None,
    })
}

fn report(
    dir: &Path,
    started: std::time::Instant,
    pages: Value,
    files: usize,
    errors: Vec<Value>,
    warnings: Vec<Value>,
    skipped: Vec<Value>,
) -> Value {
    json!({
        "command": "verify",
        "verdict": if errors.is_empty() { "pass" } else { "fail" },
        "published": false,
        "build": {"dir": dir.display().to_string(), "files": files, "ms": started.elapsed().as_millis() as u64},
        "pages": pages,
        "skipped": skipped,
        "errors": errors,
        "warnings": warnings,
    })
}
