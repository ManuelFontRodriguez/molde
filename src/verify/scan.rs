//! One pass over the build: every HTML file is read once, and only the facts the checks
//! need are kept (docs/design/verify.md §4). Checks then work from these facts.
use super::url::{self, Address, Files, Target};
use crate::build::par_map;
use crate::check::head::{markup_only, tag_attr, tag_attrs};
use crate::check::html::{
    attr_re, attr_value, has_noindex, link_tag_re, meta_tag_re, unescape_entities,
};
use crate::check::links::{css_refs, html_refs, Ref};
use crate::check::titles::without_foreign_titles;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// What one HTML file says about itself.
pub(crate) struct PageFacts {
    /// Path relative to the build folder, `/`-separated.
    pub(crate) rel: String,
    /// A document: it has `<html`, `<head` or a doctype in its first 2 KB.
    pub(crate) is_document: bool,
    pub(crate) noindex: bool,
    /// The target of a `<meta http-equiv="refresh" content="0; url=…">`, as written.
    pub(crate) refresh: Option<String>,
    /// The first `<base href>`, entities decoded: redirects resolve from it too.
    pub(crate) base_href: Option<String>,
    pub(crate) canonicals: Vec<String>,
    /// The `<link rel="alternate">` hrefs (hreflang versions, feeds), as written.
    pub(crate) alternates: Vec<String>,
    pub(crate) og_url: Vec<String>,
    pub(crate) og_title: Vec<String>,
    pub(crate) og_type: Vec<String>,
    pub(crate) title_count: usize,
    /// The first `<title>`, entities decoded, trimmed; empty when there is none.
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    /// The links and assets that resolve to no file, judged during the scan so that a
    /// page's hundreds of good links are never kept (43k pages would hold gigabytes).
    pub(crate) broken: Vec<Broken>,
    /// Text escaped twice: the first occurrence, its context and the count.
    pub(crate) double_escaped: Option<(String, String, usize)>,
    /// JSON-LD blocks that don't parse: (block number from 1, parser message).
    pub(crate) jsonld_errors: Vec<(usize, String)>,
}

/// The `.html` / `.htm` files (any case) among `all`, in its order.
pub(crate) fn html_files(all: &[String]) -> Vec<String> {
    all.iter()
        .filter(|f| {
            let lower = f.to_ascii_lowercase();
            lower.ends_with(".html") || lower.ends_with(".htm")
        })
        .cloned()
        .collect()
}

/// Every file under `dir`, sorted, `/`-separated. `.git` is never part of a build; with
/// `project_root` (a repository published as is) neither is `node_modules`.
///
/// A symlink is followed when it stays inside the build: hosts serve the file it names
/// (a deploy uploads it). One that leaves the build is not descended into or listed.
/// Each real folder is walked once, so links that reach one folder by many paths (a
/// pnpm `node_modules`, a diamond of links) can't multiply the walk. Only symlinks are
/// resolved.
pub(crate) fn all_files(dir: &Path, project_root: bool) -> Result<Vec<String>> {
    let root = dir.canonicalize()?;
    let mut seen = std::collections::HashSet::new();
    let mut keep = |e: &walkdir::DirEntry| {
        if e.depth() > 0 && e.file_type().is_dir() {
            let name = e.file_name().to_string_lossy();
            if name == ".git" || project_root && name == "node_modules" {
                return false;
            }
        }
        if !e.path_is_symlink() {
            return true;
        }
        match e.path().canonicalize() {
            Ok(real) if real.starts_with(&root) => !e.file_type().is_dir() || seen.insert(real),
            _ => false,
        }
    };
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| keep(e))
    {
        let Ok(entry) = entry else { continue };
        if entry.file_type().is_file() {
            let rel: PathBuf = entry.path().strip_prefix(dir)?.to_path_buf();
            files.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    files.sort();
    Ok(files)
}

/// A link or asset that names no file of the build.
pub(crate) struct Broken {
    pub(crate) attribute: String,
    pub(crate) written: String,
    /// `links.internal` or `links.outside_base`.
    pub(crate) check: &'static str,
    /// The site path it names (empty when it climbs above the root).
    pub(crate) path: String,
}

/// What a link is judged against: the site's address, the build's files, what to ignore.
pub(crate) struct LinkCtx<'a> {
    pub(crate) site: Option<&'a Address>,
    pub(crate) files: &'a Files,
    pub(crate) ignore: &'a [regex::Regex],
    pub(crate) spa_fallback: bool,
}

impl LinkCtx<'_> {
    /// The broken ones among a file's references, each written link once.
    fn judge(&self, rel: &str, base_href: Option<&str>, refs: Vec<Ref>) -> Vec<Broken> {
        let page_dir = url::page_dir(rel, self.site);
        let base_dir = base_dir(&page_dir, base_href, self.site);
        let mut seen = std::collections::BTreeSet::new();
        let mut broken = Vec::new();
        for r in refs {
            let dir = match &base_dir {
                Some(None) if url::is_relative(&r.written) => continue,
                Some(Some(d)) => Some(d.as_str()),
                _ => None,
            };
            let (check, path) = match url::resolve(&r.written, &page_dir, dir, self.site) {
                Target::Elsewhere => continue,
                Target::AboveRoot => ("links.internal", String::new()),
                // the base-path mistake is a root-relative `/x`; an absolute URL to the
                // host or a `../` out of the base names another site on the same host
                // (doc.rust-lang.org/book/ links ../std/), which may well exist
                Target::OutsideBase(_) if !root_relative(&r.written) => continue,
                Target::OutsideBase(p) => ("links.outside_base", p),
                Target::Site(p) => {
                    if self.ignore.iter().any(|re| re.is_match(&p))
                        || self.files.file_for(&p).is_some()
                        || self.files.redirected(&p)
                    {
                        continue;
                    }
                    // the SPA fallback serves unknown pages, not missing files
                    if self.spa_fallback && !p.rsplit('/').next().unwrap_or("").contains('.') {
                        continue;
                    }
                    ("links.internal", p)
                }
            };
            if seen.insert((r.attribute.clone(), r.written.clone())) {
                broken.push(Broken {
                    attribute: r.attribute,
                    written: r.written,
                    check,
                    path,
                });
            }
        }
        broken
    }
}

/// Where `<base href>` moves a page's relative links (`page_dir` is the page's public
/// folder): None for no move; Some(Some(folder)) to a folder of this host; Some(None)
/// somewhere verify can't follow (another host, or any absolute URL without
/// --base-url), where relative links are not this build's to judge.
pub(crate) fn base_dir(
    page_dir: &str,
    base_href: Option<&str>,
    site: Option<&Address>,
) -> Option<Option<String>> {
    let b = base_href?;
    let folder = |d: String| {
        if d.ends_with('/') {
            d
        } else {
            d.rsplit_once('/')
                .map_or("/".into(), |(a, _)| format!("{a}/"))
        }
    };
    match url::resolve(b, page_dir, None, site) {
        Target::Site(p) => {
            let base = site.map_or("/", |s| s.base.as_str()).trim_end_matches('/');
            Some(Some(folder(format!("{base}{p}"))))
        }
        Target::OutsideBase(p) => Some(Some(folder(p))),
        // `#top` or `?q` only: the page's own address, so no move at all
        Target::Elsewhere if !b.trim().is_empty() && !b.trim().starts_with(['#', '?']) => {
            Some(None)
        }
        _ => None,
    }
}

/// Resolve `written` from a page as a browser does, `<base href>` included. None when
/// the base takes a relative link out of verify's reach.
pub(crate) fn resolve_from(
    written: &str,
    rel: &str,
    base_href: Option<&str>,
    site: Option<&Address>,
) -> Option<Target> {
    let page_dir = url::page_dir(rel, site);
    match base_dir(&page_dir, base_href, site) {
        Some(None) if url::is_relative(written) => None,
        Some(Some(d)) => Some(url::resolve(written, &page_dir, Some(&d), site)),
        _ => Some(url::resolve(written, &page_dir, None, site)),
    }
}

/// `/x`, not `//host/x` or a relative or absolute URL.
fn root_relative(written: &str) -> bool {
    let w = unescape_entities(written.trim());
    w.starts_with('/') && !w.starts_with("//")
}

/// The broken references of every stylesheet under `dir`.
pub(crate) fn css_broken(
    dir: &Path,
    files: &[String],
    ctx: &LinkCtx,
) -> Result<Vec<(String, Vec<Broken>)>> {
    let sheets: Vec<&String> = files
        .iter()
        .filter(|f| f.to_ascii_lowercase().ends_with(".css"))
        .collect();
    par_map(sheets.len(), |i| {
        let css = String::from_utf8_lossy(&std::fs::read(dir.join(sheets[i]))?).into_owned();
        Ok((
            sheets[i].clone(),
            ctx.judge(sheets[i], None, css_refs(&css)),
        ))
    })
}

/// The broken links of every HTML file judged against another address: the files are
/// read again, which happens only when verify compares --base-url with the address the
/// pages claim.
pub(crate) fn html_broken(dir: &Path, files: &[String], ctx: &LinkCtx) -> Result<Vec<Vec<Broken>>> {
    par_map(files.len(), |i| {
        let raw = String::from_utf8_lossy(&std::fs::read(dir.join(&files[i]))?).into_owned();
        Ok(facts(&files[i], &raw, ctx).broken)
    })
}

pub(crate) fn scan(dir: &Path, files: &[String], ctx: &LinkCtx) -> Result<Vec<PageFacts>> {
    par_map(files.len(), |i| {
        let raw = String::from_utf8_lossy(&std::fs::read(dir.join(&files[i]))?).into_owned();
        Ok(facts(&files[i], &raw, ctx))
    })
}

fn facts(rel: &str, raw: &str, ctx: &LinkCtx) -> PageFacts {
    static RES: OnceLock<(
        regex::Regex,
        regex::Regex,
        regex::Regex,
        regex::Regex,
        regex::Regex,
    )> = OnceLock::new();
    let (doc_re, title_re, base_re, jsonld_re, comment_re) = RES.get_or_init(|| {
        (
            regex::Regex::new(r"(?i)<!doctype|<html[\s>]|<head[\s>]").unwrap(),
            regex::Regex::new(r"(?is)<title\b[^>]*>(.*?)</title\s*>").unwrap(),
            // `<base>`, not a custom element named `base-…` (`<base-button>`)
            regex::Regex::new(r"(?is)<base(?:[\s/][^>]*)?>").unwrap(),
            regex::Regex::new(
                r#"(?is)<script\s(?:[^>]*\s)?type\s*=\s*["']?application/ld\+json(?:\s*;[^"'>]*)?(?:["'\s][^>]*)?>(.*?)</script"#,
            )
            .unwrap(),
            // commented-out blocks are invisible to browsers and crawlers, as in `check`
            regex::Regex::new(r"(?s)<!--.*?-->").unwrap(),
        )
    });
    // the first 2 KB after a byte-order mark, whitespace and leading comments
    let mut start = raw.trim_start_matches('\u{feff}').trim_start();
    while let Some(rest) = start.strip_prefix("<!--") {
        start = rest.split_once("-->").map_or("", |(_, r)| r).trim_start();
    }
    let head_cut = start
        .char_indices()
        .nth(2048)
        .map_or(start.len(), |(i, _)| i);
    let is_document = doc_re.is_match(&start[..head_cut]);
    let markup = markup_only(raw);
    let titled = without_foreign_titles(raw);
    let mut titles = title_re.captures_iter(&titled);
    let first_title = titles.next().map(|c| unescape_entities(c[1].trim()));
    let title_count = usize::from(first_title.is_some()) + titles.count();
    // a refresh inside <noscript> is a fallback for readers without JavaScript, not
    // what the page is
    static NOSCRIPT: OnceLock<regex::Regex> = OnceLock::new();
    let noscript =
        NOSCRIPT.get_or_init(|| regex::Regex::new(r"(?is)<noscript\b.*?</noscript\s*>").unwrap());
    let scripted = noscript.replace_all(&markup, "");
    let refresh_in = |html: &str| {
        meta_tag_re().find_iter(html).find_map(|m| {
            let tag = m.as_str();
            let mut is_refresh = false;
            let mut content = None;
            for cap in attr_re().captures_iter(tag) {
                match cap[1].to_ascii_lowercase().as_str() {
                    "http-equiv" => is_refresh = attr_value(&cap).eq_ignore_ascii_case("refresh"),
                    "content" => content = Some(unescape_entities(attr_value(&cap))),
                    _ => {}
                }
            }
            refresh_url(&content.filter(|_| is_refresh)?)
        })
    };
    // ...unless the page is nothing but that fallback (a JavaScript redirect with a
    // <noscript> refresh, no title, no text): then it is a redirect stub
    let refresh = refresh_in(&scripted).or_else(|| {
        if first_title.is_some() || scripted.len() == markup.len() {
            return None;
        }
        static TAG: OnceLock<regex::Regex> = OnceLock::new();
        let tag = TAG.get_or_init(|| regex::Regex::new(r"(?s)<[^>]*>").unwrap());
        tag.replace_all(&scripted, "")
            .trim()
            .is_empty()
            .then(|| refresh_in(&markup))
            .flatten()
    });
    // the first <base> that has an href is the one browsers use
    let base_href = base_re.find_iter(&markup).find_map(|m| {
        attr_re()
            .captures_iter(m.as_str())
            .find(|c| c[1].eq_ignore_ascii_case("href"))
            .map(|c| unescape_entities(attr_value(&c)))
    });
    let mut jsonld_errors = Vec::new();
    for (n, cap) in jsonld_re
        .captures_iter(&comment_re.replace_all(raw, ""))
        .enumerate()
    {
        if let Err(e) = serde_json::from_str::<serde_json::Value>(cap[1].trim()) {
            jsonld_errors.push((n + 1, e.to_string()));
        }
    }
    let canonicals = tag_attrs(&markup, link_tag_re(), "rel", "canonical", "href");
    // a <base> or canonical href is not a link: <base> moves the others, and the
    // canonical is the canonical checks' to judge, precisely. On a not-found page,
    // served for any missing URL, `rel=alternate` links are for no one either: crawlers
    // don't read a 404's head, and readers never follow it
    let not_found = super::is_not_found_page(&rel.to_ascii_lowercase());
    let not_links: Vec<(usize, usize)> = base_re
        .find_iter(raw)
        .chain(link_tag_re().find_iter(raw).filter(|m| {
            attr_re().captures_iter(m.as_str()).any(|c| {
                c[1].eq_ignore_ascii_case("rel")
                    && attr_value(&c).split_ascii_whitespace().any(|r| {
                        r.eq_ignore_ascii_case("canonical")
                            || not_found && r.eq_ignore_ascii_case("alternate")
                    })
            })
        }))
        .map(|m| (m.start(), m.end()))
        .collect();
    let refs = if not_links.is_empty() {
        html_refs(raw)
    } else {
        let mut cut = raw.to_string();
        for (a, b) in not_links {
            cut.replace_range(a..b, &" ".repeat(b - a));
        }
        html_refs(&cut)
    };
    let broken = ctx.judge(rel, base_href.as_deref(), refs);
    PageFacts {
        rel: rel.to_string(),
        is_document,
        noindex: has_noindex(&markup),
        refresh,
        base_href,
        alternates: tag_attrs(&markup, link_tag_re(), "rel", "alternate", "href"),
        canonicals,
        og_url: tag_attrs(&markup, meta_tag_re(), "property", "og:url", "content"),
        og_title: tag_attrs(&markup, meta_tag_re(), "property", "og:title", "content"),
        og_type: tag_attrs(&markup, meta_tag_re(), "property", "og:type", "content"),
        title_count,
        title: first_title.unwrap_or_default(),
        description: tag_attr(&markup, meta_tag_re(), "name", "description", "content")
            .flatten()
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty()),
        broken,
        double_escaped: double_escaped(raw),
        jsonld_errors,
    }
}

/// `&amp;amp;` and the like outside code, scripts, styles and comments, as `check` reads them.
fn double_escaped(html: &str) -> Option<(String, String, usize)> {
    static RES: OnceLock<(regex::Regex, regex::Regex)> = OnceLock::new();
    let (skip_re, twice_re) = RES.get_or_init(|| {
        (
            regex::Regex::new(
                r"(?is)<!--.*?-->|<script\b.*?</script\s*>|<style\b.*?</style\s*>|<pre\b.*?</pre\s*>|<code\b.*?</code\s*>|<textarea\b.*?</textarea\s*>",
            )
            .unwrap(),
            regex::Regex::new(r"&amp;(?:amp|lt|gt|quot|apos|nbsp|#[0-9]+|#[xX][0-9a-fA-F]+);").unwrap(),
        )
    });
    let text = skip_re.replace_all(html, "");
    let mut found = twice_re.find_iter(&text);
    let first = found.next()?;
    let start = text[..first.start()]
        .char_indices()
        .rev()
        .nth(30)
        .map_or(0, |(i, _)| i);
    let end = text[first.end()..]
        .char_indices()
        .nth(30)
        .map_or(text.len(), |(i, _)| first.end() + i);
    let context = text[start..end]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    Some((first.as_str().to_string(), context, 1 + found.count()))
}

/// The URL of a meta refresh's `content`, parsed as the HTML spec does: a number, then
/// `;` or `,`, then an optional `url=` (any case, spaces around `=`), then the URL,
/// quoted or not. None without a URL: that refresh reloads the page itself.
pub(crate) fn refresh_url(content: &str) -> Option<String> {
    let s = content.trim_start();
    let time = s
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(s.len());
    if time == 0 {
        return None;
    }
    let mut rest = s[time..].trim_start();
    if let Some(r) = rest.strip_prefix([';', ',']) {
        rest = r.trim_start();
    }
    if rest.get(..3).is_some_and(|u| u.eq_ignore_ascii_case("url")) {
        if let Some(r) = rest[3..].trim_start().strip_prefix('=') {
            rest = r.trim_start();
        }
    }
    let url = match rest.chars().next() {
        Some(q @ ('"' | '\'')) => rest[1..].split(q).next().unwrap_or(""),
        _ => rest,
    };
    let url = url.trim();
    (!url.is_empty()).then(|| url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_refresh_is_read_as_the_spec_reads_it() {
        for (content, want) in [
            ("0; url=/target/", Some("/target/")),
            ("0;URL=/target/", Some("/target/")),
            ("0;Url=/target/", Some("/target/")),
            ("0; url = /target/", Some("/target/")),
            ("0,url=/target/", Some("/target/")),
            ("0; url='/target/'", Some("/target/")),
            ("0; url=\"/target/\" ", Some("/target/")),
            ("0;/target/", Some("/target/")),
            ("3.5; URL=https://x.example/", Some("https://x.example/")),
            ("0", None),
            ("5;", None),
            ("url=/x/", None),
        ] {
            assert_eq!(refresh_url(content).as_deref(), want, "{content}");
        }
    }

    fn page(rel: &str, html: &str) -> PageFacts {
        let files = Files::new(["a/index.html".to_string()], false);
        let ctx = LinkCtx {
            site: None,
            files: &files,
            ignore: &[],
            spa_fallback: false,
        };
        facts(rel, html, &ctx)
    }

    fn broken(f: &PageFacts) -> Vec<&str> {
        f.broken.iter().map(|b| b.written.as_str()).collect()
    }

    /// <base> (stress site s2 of the held-out review): its own href is not a link, the
    /// first one with an href counts, and an absolute base verify can't place leaves
    /// relative links unjudged rather than resolving them from the page.
    #[test]
    fn base_href_as_browsers_use_it() {
        let f = page(
            "p/index.html",
            r#"<html><head><base href="/a/"></head><body><a href="index.html">x</a><a href="gone/">y</a></body></html>"#,
        );
        assert_eq!(broken(&f), ["gone/"]);
        let f = page(
            "p/index.html",
            r#"<html><head><base target="_blank"><base href="/a/"></head><body><a href="index.html">x</a></body></html>"#,
        );
        assert!(broken(&f).is_empty(), "{:?}", broken(&f));
        let f = page(
            "p/index.html",
            r#"<html><head><base href="https://cdn.example/x/"></head><body><a href="index.html">x</a><a href="/gone/">y</a></body></html>"#,
        );
        assert_eq!(broken(&f), ["/gone/"]);
    }

    /// The canonical's href is not also a link, but another tag with the same href is.
    #[test]
    fn a_canonical_is_not_a_link_but_its_twin_is() {
        let f = page(
            "p/index.html",
            r#"<html><head><link rel="canonical" href="/gone/"></head><body><a href="/gone/">x</a></body></html>"#,
        );
        assert_eq!(broken(&f), ["/gone/"]);
        assert_eq!(f.canonicals, ["/gone/"]);
    }

    #[test]
    fn a_doctype_after_a_long_comment_is_a_document() {
        let html = format!(
            "\u{feff}<!-- {} -->\n<!doctype html><html><head><title>t</title></head></html>",
            "x".repeat(4000)
        );
        assert!(page("p/index.html", &html).is_document);
        assert!(!page("v.html", "google-site-verification: abc.html").is_document);
    }
}
