//! One pass over the build: every HTML file is read once, and only the facts the checks
//! need are kept (docs/design/verify.md §4). Checks then work from these facts.
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
    /// `<base href>`, as written.
    pub(crate) base_href: Option<String>,
    pub(crate) canonicals: Vec<String>,
    pub(crate) og_url: Vec<String>,
    pub(crate) og_title: Vec<String>,
    pub(crate) og_type: Vec<String>,
    pub(crate) title_count: usize,
    /// The first `<title>`, entities decoded, trimmed; empty when there is none.
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    pub(crate) refs: Vec<Ref>,
    /// Text escaped twice: the first occurrence, its context and the count.
    pub(crate) double_escaped: Option<(String, String, usize)>,
    /// JSON-LD blocks that don't parse: (block number from 1, parser message).
    pub(crate) jsonld_errors: Vec<(usize, String)>,
}

/// Every `.html` / `.htm` file under `dir` (any case), sorted, `/`-separated.
pub(crate) fn html_files(dir: &Path) -> Result<Vec<String>> {
    Ok(all_files(dir)?
        .into_iter()
        .filter(|f| {
            let lower = f.to_ascii_lowercase();
            lower.ends_with(".html") || lower.ends_with(".htm")
        })
        .collect())
}

/// Every file under `dir`, sorted, `/`-separated. Symlinks are not followed out of `dir`.
pub(crate) fn all_files(dir: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(dir).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let rel: PathBuf = entry.path().strip_prefix(dir)?.to_path_buf();
            files.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    files.sort();
    Ok(files)
}

/// Every stylesheet under `dir`, with the URLs it references.
pub(crate) fn css_refs_of(dir: &Path, files: &[String]) -> Result<Vec<(String, Vec<Ref>)>> {
    let sheets: Vec<&String> = files
        .iter()
        .filter(|f| f.to_ascii_lowercase().ends_with(".css"))
        .collect();
    par_map(sheets.len(), |i| {
        let css = String::from_utf8_lossy(&std::fs::read(dir.join(sheets[i]))?).into_owned();
        Ok((sheets[i].clone(), css_refs(&css)))
    })
}

pub(crate) fn scan(dir: &Path, files: &[String]) -> Result<Vec<PageFacts>> {
    par_map(files.len(), |i| {
        let raw = String::from_utf8_lossy(&std::fs::read(dir.join(&files[i]))?).into_owned();
        Ok(facts(&files[i], &raw))
    })
}

fn facts(rel: &str, raw: &str) -> PageFacts {
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
            regex::Regex::new(r"(?is)<base\b[^>]*>").unwrap(),
            regex::Regex::new(
                r#"(?is)<script\s(?:[^>]*\s)?type\s*=\s*["']?application/ld\+json(?:\s*;[^"'>]*)?(?:["'\s][^>]*)?>(.*?)</script"#,
            )
            .unwrap(),
            // commented-out blocks are invisible to browsers and crawlers, as in `check`
            regex::Regex::new(r"(?s)<!--.*?-->").unwrap(),
        )
    });
    let head_cut = raw.char_indices().nth(2048).map_or(raw.len(), |(i, _)| i);
    let is_document = doc_re.is_match(&raw[..head_cut]);
    let markup = markup_only(raw);
    let titled = without_foreign_titles(raw);
    let mut titles = title_re.captures_iter(&titled);
    let first_title = titles.next().map(|c| unescape_entities(c[1].trim()));
    let title_count = usize::from(first_title.is_some()) + titles.count();
    let refresh = meta_tag_re().find_iter(&markup).find_map(|m| {
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
        let content = content.filter(|_| is_refresh)?;
        // "0; url=/target/" (url= optional, quotes allowed)
        let after = content.split_once(';').map_or("", |(_, r)| r).trim();
        let url = after
            .strip_prefix("url=")
            .or_else(|| after.strip_prefix("URL="))
            .unwrap_or(after);
        let url = url.trim().trim_matches(['\'', '"']).to_string();
        (!url.is_empty()).then_some(url)
    });
    let base_href = base_re.find(&markup).and_then(|m| {
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
    PageFacts {
        rel: rel.to_string(),
        is_document,
        noindex: has_noindex(&markup),
        refresh,
        base_href,
        canonicals: tag_attrs(&markup, link_tag_re(), "rel", "canonical", "href"),
        og_url: tag_attrs(&markup, meta_tag_re(), "property", "og:url", "content"),
        og_title: tag_attrs(&markup, meta_tag_re(), "property", "og:title", "content"),
        og_type: tag_attrs(&markup, meta_tag_re(), "property", "og:type", "content"),
        title_count,
        title: first_title.unwrap_or_default(),
        description: tag_attr(&markup, meta_tag_re(), "name", "description", "content")
            .flatten()
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty()),
        refs: html_refs(raw),
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
