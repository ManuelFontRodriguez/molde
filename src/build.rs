use crate::check::html::{unescape_entities, xml_escape};
use crate::check::paths::check_duplicate_paths;
use crate::locales;
use crate::render::{engine, page_url, render_page, PageFailed};
use crate::routes::{expand_routes, is_valid_lastmod};
use crate::site::{load_site, Page, Site};
use crate::store::{list_builds, lock_out, publish};
use anyhow::{bail, Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;
use minijinja::Environment;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

#[derive(Debug)]
pub struct BuildReport {
    pub pages: usize,
    pub assets: usize,
    pub ms: u128,
    pub build_dir: PathBuf,
    /// `t()` keys the default locale doesn't define: (template, key, locale).
    pub unknown_keys: Vec<(String, String, String)>,
}

/// Render + publish, unconditionally. `check`/the events module use the gated
/// path instead: render_site -> conformance -> publish only on pass.
pub fn build(site_dir: &Path, out: &Path) -> Result<BuildReport> {
    let _lock = lock_out(out)?;
    let report = render_site(site_dir, out)?;
    publish(out, &report.build_dir)?;
    Ok(report)
}

/// Render into a new immutable build dir. Does NOT touch `current`.
pub fn render_site(site_dir: &Path, out: &Path) -> Result<BuildReport> {
    let t0 = Instant::now();
    let site = load_site(site_dir)?;
    let pages = expand_routes(&site)?;
    render_loaded(&site, &pages, out, t0)
}

/// `render_site` for a site that is already loaded and planned: `check` loads and
/// expands once and renders from that, instead of parsing data.json twice.
pub(crate) fn render_loaded(
    site: &Site,
    pages: &[Page],
    out: &Path,
    t0: Instant,
) -> Result<BuildReport> {
    let env = engine(site)?;

    // duplicate/reserved output paths mean a later write silently replaces an
    // earlier page while the report claims both — refuse to render at all
    let dups = check_duplicate_paths(site, pages);
    if !dups.is_empty() {
        let detail: Vec<String> = dups
            .iter()
            .map(|e| {
                if e["check"] == "paths.file_and_folder" {
                    return format!(
                        "/{} is a file but /{} needs it as a folder",
                        e["page"].as_str().unwrap_or("?"),
                        e["inside"].as_str().unwrap_or("?")
                    );
                }
                let offenders = e["routes"]
                    .as_array()
                    .map(|rs| {
                        rs.iter()
                            .filter_map(|r| r["route"].as_str())
                            .collect::<Vec<_>>()
                            .join(" and ")
                    })
                    .unwrap_or_else(|| {
                        format!("{} (framework-emitted)", e["route"].as_str().unwrap_or("?"))
                    });
                format!("/{} <- {}", e["page"].as_str().unwrap_or("?"), offenders)
            })
            .collect();
        bail!(
            "duplicate output paths — a later write would silently overwrite an earlier page: {}",
            detail.join("; ")
        );
    }
    let path_errors = locales::routes_errors(site, pages);
    if !path_errors.is_empty() {
        let detail: Vec<String> = path_errors
            .iter()
            .map(|e| match e["page"].as_str() {
                Some(page) => format!("{} /{page}", e["check"].as_str().unwrap_or("?")),
                None => e["check"].as_str().unwrap_or("?").to_string(),
            })
            .collect();
        bail!(
            "locale path errors (see `molde check`): {}",
            detail.join("; ")
        );
    }

    // --- immutable build dir, written under a dot-prefixed name that
    // list_builds ignores and renamed only once complete: a failed render
    // leaves no half-written build to become the diff baseline ---
    let build_id = format!("{}", std::time::UNIX_EPOCH.elapsed()?.as_nanos());
    let build_dir = out.join("builds").join(&build_id);
    let tmp_dir = out.join("builds").join(format!(".tmp-{build_id}"));
    fs::create_dir_all(&tmp_dir)?;
    let assets = match write_build(site, &env, pages, &tmp_dir) {
        Ok(assets) => assets,
        Err(e) => {
            let _ = fs::remove_dir_all(&tmp_dir);
            return Err(e);
        }
    };

    // --- dedup: a byte-identical rebuild reuses an existing build, so cautious
    // re-verification never pollutes the diff baseline or accumulates dirs.
    // The PUBLISHED build is checked before the newest: reverting an edit whose
    // failing build never went live must resolve back to the live id — minting a
    // byte-identical new build would swap `current` for no visible change and
    // spuriously reload --reload browser tabs ---
    let published = fs::read_link(out.join("current"))
        .ok()
        .and_then(|t| t.file_name().map(|n| n.to_string_lossy().into_owned()));
    let unknown_keys: Vec<(String, String, String)> =
        site.unknown_keys.lock().unwrap().iter().cloned().collect();
    let newest = list_builds(out)
        .ok()
        .and_then(|ids| ids.into_iter().next_back());
    let mut candidates: Vec<String> = Vec::new();
    candidates.extend(published);
    candidates.extend(newest.filter(|id| !candidates.contains(id)));
    for prev in candidates {
        let prev_dir = out.join("builds").join(&prev);
        let identical = prev_dir.is_dir() && dirs_identical(&tmp_dir, &prev_dir).unwrap_or(false);
        if identical {
            fs::remove_dir_all(&tmp_dir)?;
            return Ok(BuildReport {
                pages: pages.len(),
                assets,
                ms: t0.elapsed().as_millis(),
                build_dir: prev_dir,
                unknown_keys,
            });
        }
    }
    if let Err(e) = fs::rename(&tmp_dir, &build_dir) {
        let _ = fs::remove_dir_all(&tmp_dir);
        return Err(e).context("could not move the finished build into place");
    }

    Ok(BuildReport {
        pages: pages.len(),
        assets,
        ms: t0.elapsed().as_millis(),
        build_dir,
        unknown_keys,
    })
}

/// Static assets, pages, then the sitemap and robots.txt into `build_dir`;
/// returns the number of static assets copied.
fn write_build(
    site: &Site,
    env: &Environment<'static>,
    pages: &[Page],
    build_dir: &Path,
) -> Result<usize> {
    // --- static assets first: routes win over static/ on collision ---
    let assets = copy_static(site, build_dir)?;

    // --- render: pure function of (templates, snapshot) ---
    // every core renders, writes and compresses pages; the error reported is the
    // first failing page in page order, exactly as a sequential loop would give
    par_for_each(pages.len(), |i| {
        let page = &pages[i];
        let html = render_page(env, site, page).with_context(|| PageFailed {
            page: page.path.trim_start_matches('/').to_string(),
            route: page.route,
        })?;
        write_page(build_dir, &page.path, &html)
    })?;

    // --- web-standards: sitemap is a query over the route manifest ---
    if let Some(base) = &site.config.base_url {
        let base = base.trim_end_matches('/');
        write_sitemaps(build_dir, base, pages, SITEMAP_MAX_URLS)?;

        // robots.txt ships with the capability, unless the author provided one in static/
        if !build_dir.join("robots.txt").is_file() {
            write_page(
                build_dir,
                "/robots.txt",
                &format!("User-agent: *\nAllow: /\n\nSitemap: {base}/sitemap.xml\n"),
            )?;
        }
    }
    Ok(assets)
}

/// The sitemap protocol's per-file limits: 50,000 URLs and 50 MB uncompressed.
/// The byte cap keeps a margin for the closing tag.
const SITEMAP_MAX_URLS: usize = 50_000;
const SITEMAP_MAX_BYTES: usize = 50 * 1024 * 1024 - 1024;

const URLSET_OPEN: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
     <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n";

/// sitemap.xml lists every indexable page. Above `max_urls` URLs (or 50 MB) it
/// becomes a sitemap index pointing at `sitemap-1.xml`, `sitemap-2.xml`, … each
/// within the limits. robots.txt keeps pointing at sitemap.xml either way.
fn write_sitemaps(build_dir: &Path, base: &str, pages: &[Page], max_urls: usize) -> Result<()> {
    let entries: Vec<String> = pages
        .iter()
        .filter(|p| p.in_sitemap())
        .map(|page| {
            let loc = xml_escape(&page_url(base, &page.path));
            // an invalid lastmod is left out here and reported by sitemap.lastmod_invalid
            match page.lastmod.as_deref().filter(|l| is_valid_lastmod(l)) {
                Some(lastmod) => format!(
                    "  <url><loc>{loc}</loc><lastmod>{}</lastmod></url>\n",
                    xml_escape(lastmod)
                ),
                None => format!("  <url><loc>{loc}</loc></url>\n"),
            }
        })
        .collect();
    let mut files: Vec<String> = vec![String::from(URLSET_OPEN)];
    let mut in_file = 0usize;
    for entry in &entries {
        let current = files.last_mut().unwrap();
        // a new file only once this one holds an entry: never an empty child sitemap
        if in_file == max_urls || (in_file > 0 && current.len() + entry.len() > SITEMAP_MAX_BYTES) {
            files.push(String::from(URLSET_OPEN));
            in_file = 0;
        }
        files.last_mut().unwrap().push_str(entry);
        in_file += 1;
    }
    for xml in files.iter_mut() {
        xml.push_str("</urlset>\n");
    }
    if files.len() == 1 {
        return write_page(build_dir, "/sitemap.xml", &files[0]);
    }
    let mut index = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <sitemapindex xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for (n, xml) in files.iter().enumerate() {
        let name = format!("/sitemap-{}.xml", n + 1);
        write_page(build_dir, &name, xml)?;
        index.push_str(&format!(
            "  <sitemap><loc>{}</loc></sitemap>\n",
            xml_escape(&page_url(base, &name))
        ));
    }
    index.push_str("</sitemapindex>\n");
    write_page(build_dir, "/sitemap.xml", &index)
}

/// Is `path` (no leading slash, lowercase) a file the sitemap writer may emit?
pub(crate) fn is_sitemap_file(path: &str) -> bool {
    path == "sitemap.xml"
        || path
            .strip_prefix("sitemap-")
            .and_then(|rest| rest.strip_suffix(".xml"))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// A sitemap `<loc>` -> the file it names in the build ("https://host/a/" ->
/// "a/index.html"), by removing exactly `base`; None when it is outside the base.
pub(crate) fn loc_to_file(loc: &str, base: Option<&str>) -> Option<String> {
    let path = match base {
        Some(base) => loc
            .strip_prefix(base)
            .filter(|rest| rest.is_empty() || rest.starts_with('/'))
            .map(|rest| rest.trim_start_matches('/')),
        None => Some(loc.splitn(4, '/').nth(3).unwrap_or_default()),
    }?;
    let mut rel = path
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_string();
    if rel.is_empty() || rel.ends_with('/') {
        rel.push_str("index.html");
    }
    Some(rel)
}

/// The page `<loc>`s the build's sitemap lists, following a sitemap index into
/// its child files. The second list holds child sitemap locs that name no file.
/// None when the build has no sitemap.xml.
pub(crate) fn sitemap_page_locs(
    build_dir: &Path,
    base: Option<&str>,
) -> Result<Option<(Vec<String>, Vec<String>)>> {
    static LOC_RE: OnceLock<regex::Regex> = OnceLock::new();
    let loc_re = LOC_RE.get_or_init(|| regex::Regex::new(r"<loc>([^<]+)</loc>").unwrap());
    let Ok(xml) = fs::read_to_string(build_dir.join("sitemap.xml")) else {
        return Ok(None);
    };
    let locs = |xml: &str| -> Vec<String> {
        loc_re
            .captures_iter(xml)
            .map(|c| unescape_entities(&c[1]))
            .collect()
    };
    if !xml.contains("<sitemapindex") {
        return Ok(Some((locs(&xml), Vec::new())));
    }
    let mut pages = Vec::new();
    let mut missing = Vec::new();
    for child in locs(&xml) {
        match loc_to_file(&child, base)
            .filter(|rel| is_sitemap_file(rel))
            .and_then(|rel| fs::read_to_string(build_dir.join(rel)).ok())
        {
            Some(child_xml) => pages.extend(locs(&child_xml)),
            None => missing.push(child),
        }
    }
    Ok(Some((pages, missing)))
}

fn all_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(dir) {
        let entry = entry?;
        if entry.file_type().is_file() {
            files.push(entry.path().strip_prefix(dir)?.to_path_buf());
        }
    }
    files.sort();
    Ok(files)
}

fn dirs_identical(a: &Path, b: &Path) -> Result<bool> {
    let files_a = all_files(a)?;
    if files_a != all_files(b)? {
        return Ok(false);
    }
    // compared on every core; a size mismatch settles a file without reading it,
    // and the first difference found stops the rest
    let differ = std::sync::atomic::AtomicBool::new(false);
    par_for_each(files_a.len(), |i| {
        if differ.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(());
        }
        let (fa, fb) = (a.join(&files_a[i]), b.join(&files_a[i]));
        if fs::metadata(&fa)?.len() != fs::metadata(&fb)?.len() || fs::read(&fa)? != fs::read(&fb)?
        {
            differ.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(())
    })?;
    Ok(!differ.into_inner())
}

/// Runs `f(i)` for every `i` in `0..n` on all cores. Indices are handed out in
/// increasing order and stop being handed out after a failure, so every index
/// below a failing one has run, and the error returned is the one of the lowest
/// failing index: the same error a sequential loop would stop at.
fn par_for_each(n: usize, f: impl Fn(usize) -> Result<()> + Sync) -> Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let threads = std::thread::available_parallelism()
        .map_or(1, |t| t.get())
        .min(n.max(1));
    let next = AtomicUsize::new(0);
    let stop_at = AtomicUsize::new(usize::MAX);
    let failed: std::sync::Mutex<Option<(usize, anyhow::Error)>> = std::sync::Mutex::new(None);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= n || i > stop_at.load(Ordering::Relaxed) {
                    break;
                }
                if let Err(e) = f(i) {
                    stop_at.fetch_min(i, Ordering::Relaxed);
                    let mut slot = failed.lock().unwrap();
                    if slot.as_ref().is_none_or(|(j, _)| i < *j) {
                        *slot = Some((i, e));
                    }
                    break;
                }
            });
        }
    });
    match failed.into_inner().unwrap() {
        Some((_, e)) => Err(e),
        None => Ok(()),
    }
}

fn write_page(build_dir: &Path, url_path: &str, html: &str) -> Result<()> {
    let rel = url_path.trim_start_matches('/');
    if rel.is_empty() {
        bail!("route path must not be empty");
    }
    // traversal guard: nothing may be written outside the build dir — same
    // rule the dev server applies on the way out (fn respond)
    if rel
        .split('/')
        .any(|seg| seg.is_empty() || seg == "." || seg == "..")
    {
        bail!("unsafe output path: {url_path:?}");
    }
    let file = build_dir.join(rel);
    // compress first, on the caller's core; only the file system work is rationed
    let gz_bytes = gzip(html.as_bytes())?;
    let _permit = FILE_WRITERS.acquire();
    // the path was validated when the site loaded: a failure here is the disk's (full,
    // read-only, permissions), reported as the tool failing, naming the file
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(&file, html).with_context(|| format!("writing {}", file.display()))?;
    let gz = gz_sibling(&file);
    fs::write(&gz, gz_bytes).with_context(|| format!("writing {}", gz.display()))?;
    Ok(())
}

/// `f(i)` for every `i` in `0..n` on all cores, results in index order. The same
/// error rule as `par_for_each`.
pub(crate) fn par_map<T: Send + Sync>(
    n: usize,
    f: impl Fn(usize) -> Result<T> + Sync,
) -> Result<Vec<T>> {
    let slots: Vec<OnceLock<T>> = (0..n).map(|_| OnceLock::new()).collect();
    par_for_each(n, |i| {
        let _ = slots[i].set(f(i)?);
        Ok(())
    })?;
    Ok(slots
        .into_iter()
        .map(|slot| slot.into_inner().expect("every index ran"))
        .collect())
}

/// Precompressed sibling for nginx gzip_static — compression happens once, at cycle time.
fn write_gz_sibling(file: &Path, bytes: &[u8]) -> Result<()> {
    fs::write(gz_sibling(file), gzip(bytes)?)?;
    Ok(())
}

fn gz_sibling(file: &Path) -> PathBuf {
    let mut gz_name = file.file_name().unwrap().to_os_string();
    gz_name.push(".gz");
    file.with_file_name(gz_name)
}

/// The smallest output gzip can make (level 9): the bytes of every `.gz` stay the
/// same as they always were, and the work runs on every core.
fn gzip(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut enc = GzEncoder::new(Vec::with_capacity(bytes.len() / 4), Compression::best());
    enc.write_all(bytes)?;
    Ok(enc.finish()?)
}

/// How many threads may create files at once. Rendering and compressing use every
/// core, but creating files and folders does not scale the same way: on APFS,
/// building 1,901 pages took 168 ms with 4 concurrent writers and 320 ms with 12.
const MAX_FILE_WRITERS: usize = 4;

static FILE_WRITERS: Permits = Permits::new(MAX_FILE_WRITERS);

/// A counting semaphore: `acquire` waits for one of `n` permits, which the guard
/// returns when dropped.
struct Permits {
    free: std::sync::Mutex<usize>,
    freed: std::sync::Condvar,
}

impl Permits {
    const fn new(n: usize) -> Self {
        Permits {
            free: std::sync::Mutex::new(n),
            freed: std::sync::Condvar::new(),
        }
    }

    fn acquire(&self) -> Permit<'_> {
        let mut free = self.free.lock().unwrap();
        while *free == 0 {
            free = self.freed.wait(free).unwrap();
        }
        *free -= 1;
        Permit(self)
    }
}

struct Permit<'a>(&'a Permits);

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        *self.0.free.lock().unwrap() += 1;
        self.0.freed.notify_one();
    }
}

/// Text-like assets get a .gz sibling; binaries (images, fonts) do not.
fn is_compressible(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some(
            "css" | "js" | "svg" | "txt" | "xml" | "json" | "html" | "htm" | "map" | "webmanifest"
        )
    )
}

/// Copy site/static/** verbatim into the build. Runs BEFORE rendering, so a route
/// that collides with a static file wins (routes are the site's source of truth).
fn copy_static(site: &Site, build_dir: &Path) -> Result<usize> {
    let static_dir = site.root.join("static");
    if !static_dir.is_dir() {
        return Ok(0);
    }
    let mut count = 0;
    for entry in walkdir::WalkDir::new(&static_dir) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        // file-manager litter is not part of the site, and would ship to the web root
        if matches!(entry.file_name().to_str(), Some(".DS_Store" | "Thumbs.db")) {
            continue;
        }
        let rel = entry.path().strip_prefix(&static_dir)?;
        let dst = build_dir.join(rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(entry.path(), &dst)?;
        if is_compressible(rel) {
            write_gz_sibling(&dst, &fs::read(entry.path())?)?;
        }
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod sitemap_split {
    //! The sitemap split with a small URL limit: a real 50,001-page site is too slow for the suite.
    use super::*;
    use crate::check::head::check_head;
    use crate::check::sitemap::check_sitemap_at;

    fn site(tag: &str, pages: usize) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("molde-split-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("templates")).unwrap();
        fs::write(
            dir.join("routes.yaml"),
            "routes:\n  - path: /p/{slug}/index.html\n    template: p.j2\n    \
             foreach: items\n    title: \"{{ item.slug }}\"\n",
        )
        .unwrap();
        let items: Vec<_> = (0..pages)
            .map(|n| serde_json::json!({ "slug": format!("p{n}") }))
            .collect();
        fs::write(
            dir.join("data.json"),
            serde_json::json!({ "items": items }).to_string(),
        )
        .unwrap();
        fs::write(dir.join("site.yaml"), "base_url: https://x.example/blog\n").unwrap();
        fs::write(
            dir.join("templates/p.j2"),
            "<html><head>\n{{ std_head }}\n</head><body>x</body></html>",
        )
        .unwrap();
        dir
    }

    #[test]
    fn above_the_limit_the_sitemap_becomes_an_index_the_checks_follow() {
        let dir = site("index", 5);
        let s = load_site(&dir).unwrap();
        let pages = expand_routes(&s).unwrap();
        let build = render_site(&dir, &dir.join("out")).unwrap().build_dir;
        // one file at the default limit
        assert!(!fs::read_to_string(build.join("sitemap.xml"))
            .unwrap()
            .contains("<sitemapindex"));

        write_sitemaps(&build, "https://x.example/blog", &pages, 2).unwrap();
        let index = fs::read_to_string(build.join("sitemap.xml")).unwrap();
        assert!(index.contains("<sitemapindex"), "{index}");
        for n in 1..=3 {
            assert!(
                index.contains(&format!(
                    "<loc>https://x.example/blog/sitemap-{n}.xml</loc>"
                )),
                "{index}"
            );
        }
        assert!(!build.join("sitemap-4.xml").exists());
        let urls = |n: usize| {
            fs::read_to_string(build.join(format!("sitemap-{n}.xml")))
                .unwrap()
                .matches("<url>")
                .count()
        };
        assert_eq!((urls(1), urls(2), urls(3)), (2, 2, 1));

        // the sitemap and canonical checks read through the index
        let base = Some("https://x.example/blog");
        assert_eq!(
            check_sitemap_at(&build, &pages, base).unwrap(),
            Vec::<serde_json::Value>::new()
        );
        let head = check_head(&build, &s, &pages).unwrap();
        assert!(
            !head
                .iter()
                .any(|e| e["check"] == "canonical.not_in_sitemap"),
            "{head:?}"
        );

        // a missing child file is dangling, and its pages are missing
        fs::remove_file(build.join("sitemap-2.xml")).unwrap();
        let errors = check_sitemap_at(&build, &pages, base).unwrap();
        assert!(errors.iter().any(|e| e["check"] == "sitemap.dangling"
            && e["loc"] == "https://x.example/blog/sitemap-2.xml"));
        assert_eq!(
            errors
                .iter()
                .filter(|e| e["check"] == "sitemap.missing_page")
                .count(),
            2,
            "{errors:?}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sitemap_file_names_are_reserved() {
        assert!(is_sitemap_file("sitemap.xml"));
        assert!(is_sitemap_file("sitemap-12.xml"));
        assert!(!is_sitemap_file("sitemap-.xml"));
        assert!(!is_sitemap_file("sitemap-a.xml"));
        assert!(!is_sitemap_file("blog/sitemap-1.xml"));

        let dir = site("reserved", 1);
        fs::write(
            dir.join("routes.yaml"),
            "routes:\n  - path: /sitemap-1.xml\n    template: p.j2\n",
        )
        .unwrap();
        let s = load_site(&dir).unwrap();
        let pages = expand_routes(&s).unwrap();
        let errors = check_duplicate_paths(&s, &pages);
        assert_eq!(errors[0]["check"], "paths.reserved", "{errors:?}");
        assert_eq!(errors[0]["page"], "sitemap-1.xml");
        let _ = fs::remove_dir_all(&dir);
    }
}
