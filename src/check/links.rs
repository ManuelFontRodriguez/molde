use crate::build::par_map;
use crate::check::html::{code_re, html_files, percent_decode, unescape_entities};
use crate::check::read_page;
use anyhow::Result;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::OnceLock;

/// Conformance check 1: every URL a browser would fetch from this build resolves to a
/// file in it: `href` and `src` (any quoting, root-relative or relative), every
/// `srcset` candidate, and `url()`/`@import` in stylesheets, `<style>` blocks and
/// `style` attributes.
pub fn check_links(build_dir: &Path) -> Result<Vec<serde_json::Value>> {
    check_links_for(build_dir, None)
}

/// [`check_links`], with the site's own address: with `base_url` set, a URL written
/// absolute to it (`https://example.com/books/x/`, `//example.com/books/x/`, either
/// scheme) is this build's and is checked like `/books/x/`.
pub fn check_links_for(build_dir: &Path, base_url: Option<&str>) -> Result<Vec<serde_json::Value>> {
    let own = base_url.and_then(SiteAddress::parse);
    // whether a target resolves, asked of the disk once per target for the whole
    // build: a site links to the same pages from thousands of pages
    let resolves: std::sync::RwLock<HashMap<String, bool>> = Default::default();
    let resolves_target = |target: &str| -> bool {
        if let Some(&known) = resolves.read().unwrap().get(target) {
            return known;
        }
        let rel = target.trim_start_matches('/');
        let found =
            build_dir.join(rel).is_file() || build_dir.join(rel).join("index.html").is_file();
        resolves.write().unwrap().insert(target.to_string(), found);
        found
    };
    let check = |from: &Path, refs: Vec<Ref>| -> Vec<serde_json::Value> {
        let base = url_dir(from);
        let mut errors = Vec::new();
        let mut seen = BTreeSet::new();
        for r in refs {
            let Some(target) = resolve(&base, &r.written, own.as_ref()) else {
                continue; // another site, a scheme, or this same page
            };
            if !seen.insert((r.attribute.clone(), target.clone())) {
                continue;
            }
            let ok = target.is_some_and(|t| resolves_target(t.trim_end_matches('/')));
            if !ok {
                errors.push(serde_json::json!({
                    "layer": "output",
                    "check": "links.internal",
                    "page": from.display().to_string(),
                    "attribute": r.attribute,
                    "broken_href": r.written,
                }));
            }
        }
        errors
    };
    let pages = html_files(build_dir)?;
    let mut errors: Vec<serde_json::Value> = par_map(pages.len(), |i| {
        let html = read_page(&build_dir.join(&pages[i]))?;
        Ok(check(&pages[i], html_refs(&html)))
    })?
    .into_iter()
    .flatten()
    .collect();
    let sheets = css_files(build_dir)?;
    errors.extend(
        par_map(sheets.len(), |i| {
            let css = read_page(&build_dir.join(&sheets[i]))?;
            Ok(check(&sheets[i], css_refs(&css)))
        })?
        .into_iter()
        .flatten(),
    );
    Ok(errors)
}

/// One URL as written in the page or stylesheet, and where.
struct Ref {
    /// `href`, `src` (as written in the markup), `srcset`, `url()` or `@import`.
    attribute: String,
    written: String,
}

/// Every URL a page makes the browser fetch or follow.
fn html_refs(html: &str) -> Vec<Ref> {
    static RES: OnceLock<(regex::Regex, regex::Regex, regex::Regex, regex::Regex)> =
        OnceLock::new();
    let (ref_re, srcset_re, style_attr_re, style_block_re) = RES.get_or_init(|| {
        (
            // double-quoted, single-quoted or unquoted, as HTML allows
            regex::Regex::new(r#"(?i)\b(href|src)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+))"#)
                .unwrap(),
            regex::Regex::new(r#"(?i)\bsrcset\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap(),
            regex::Regex::new(r#"(?i)\bstyle\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap(),
            regex::Regex::new(r"(?is)<style\b[^>]*>(.*?)</style\s*>").unwrap(),
        )
    });
    // an unquoted `href=/x` is markup only outside scripts, styles and comments
    // (`el.src=/re/.test(s)` in a script is code)
    let code: Vec<(usize, usize)> = code_re()
        .find_iter(html)
        .map(|m| (m.start(), m.end()))
        .collect();
    let in_code = |at: usize| code.iter().any(|&(a, b)| a <= at && at < b);
    let mut refs = Vec::new();
    for cap in ref_re.captures_iter(html) {
        if cap.get(4).is_some_and(|bare| in_code(bare.start())) {
            continue;
        }
        let written = cap
            .get(2)
            .or(cap.get(3))
            .or(cap.get(4))
            .map_or("", |m| m.as_str());
        refs.push(Ref {
            attribute: cap[1].to_string(),
            written: written.to_string(),
        });
    }
    for cap in srcset_re.captures_iter(html) {
        let value = cap.get(1).or(cap.get(2)).map_or("", |m| m.as_str());
        // "a.png 1x, b.png 2x": each candidate is a URL and an optional descriptor
        for candidate in value.split(',') {
            if let Some(url) = candidate.split_whitespace().next() {
                refs.push(Ref {
                    attribute: "srcset".into(),
                    written: url.to_string(),
                });
            }
        }
    }
    for cap in style_attr_re.captures_iter(html) {
        let style = unescape_entities(cap.get(1).or(cap.get(2)).map_or("", |m| m.as_str()));
        refs.extend(css_refs(&style));
    }
    for cap in style_block_re.captures_iter(html) {
        refs.extend(css_refs(&cap[1]));
    }
    refs
}

/// Every `url()` and `@import` in CSS (comments skipped).
fn css_refs(css: &str) -> Vec<Ref> {
    static RES: OnceLock<(regex::Regex, regex::Regex, regex::Regex)> = OnceLock::new();
    let (comment_re, url_re, import_re) = RES.get_or_init(|| {
        (
            regex::Regex::new(r"(?s)/\*.*?\*/").unwrap(),
            regex::Regex::new(r#"(?i)\burl\(\s*(?:"([^"]*)"|'([^']*)'|([^)'"\s]*))\s*\)"#).unwrap(),
            regex::Regex::new(r#"(?i)@import\s+(?:"([^"]*)"|'([^']*)')"#).unwrap(),
        )
    });
    let css = comment_re.replace_all(css, "");
    let mut refs = Vec::new();
    for cap in url_re.captures_iter(&css) {
        let url = cap
            .get(1)
            .or(cap.get(2))
            .or(cap.get(3))
            .map_or("", |m| m.as_str());
        refs.push(Ref {
            attribute: "url()".into(),
            written: url.trim().to_string(),
        });
    }
    // `@import url(x)` is covered by url() above
    for cap in import_re.captures_iter(&css) {
        let url = cap.get(1).or(cap.get(2)).map_or("", |m| m.as_str());
        refs.push(Ref {
            attribute: "@import".into(),
            written: url.to_string(),
        });
    }
    refs
}

/// The folder a page or stylesheet is served from, as a URL path ending in `/`:
/// `books/dune/index.html` and `css/site.css` give `/books/dune/` and `/css/`.
fn url_dir(file: &Path) -> String {
    let parent = file
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"));
    match parent.as_deref() {
        None | Some("") => "/".into(),
        Some(dir) => format!("/{dir}/"),
    }
}

/// The site's public address, split as a browser compares it: the host case-insensitively,
/// the path (`/blog` in `https://example.com/blog`) exactly.
struct SiteAddress {
    host: String,
    path: String,
}

impl SiteAddress {
    fn parse(base_url: &str) -> Option<Self> {
        let rest = base_url
            .strip_prefix("https://")
            .or_else(|| base_url.strip_prefix("http://"))?;
        let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
        Some(Self {
            host: host.to_ascii_lowercase(),
            path: format!("/{}", path.trim_end_matches('/')).replace("//", "/"),
        })
    }

    /// The root-relative form of a URL written absolute to this site, or `None` when it
    /// names another site (or another folder of the same host, outside the site's path).
    fn own_path(&self, url: &str) -> Option<String> {
        let rest = url
            .strip_prefix("//")
            .or_else(|| strip_prefix_ci(url, "https://"))
            .or_else(|| strip_prefix_ci(url, "http://"))?;
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        if !rest[..end].eq_ignore_ascii_case(&self.host) {
            return None;
        }
        let after = &rest[end..];
        let after = if self.path == "/" {
            after
        } else {
            after.strip_prefix(self.path.as_str())?
        };
        match after.chars().next() {
            None => Some("/".into()),
            Some('/') => Some(after.into()),
            Some('?' | '#') => Some(format!("/{after}")),
            Some(_) => None, // `/blogroll` is not under `/blog`
        }
    }
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    (s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix))
        .then(|| &s[prefix.len()..])
}

/// The build path a URL names, resolved from the folder `base` the way a browser
/// resolves it. `None` when the URL is not this build's (a scheme such as `https:`,
/// `mailto:` or `data:`, a `//host` that isn't the site's own address, a bare
/// `#fragment` or `?query`, an empty value); `Some(None)` when it climbs above the site
/// root, which no file of the build can be.
fn resolve(base: &str, written: &str, own: Option<&SiteAddress>) -> Option<Option<String>> {
    static SCHEME: OnceLock<regex::Regex> = OnceLock::new();
    let scheme = SCHEME.get_or_init(|| regex::Regex::new(r"^[A-Za-z][A-Za-z0-9+.-]*:").unwrap());
    // the browser's reading of the attribute: entities first (`&amp;`)
    let value = unescape_entities(written.trim());
    // a URL written absolute to the site's own address is the same as its root-relative form
    let value = match own.and_then(|o| o.own_path(&value)) {
        Some(path) => path,
        None => value,
    };
    if value.is_empty()
        || value.starts_with("//")
        || value.starts_with('#')
        || value.starts_with('?')
        || scheme.is_match(&value)
    {
        return None;
    }
    // the file is the path alone: `?query` and `#fragment` never name one, and
    // `%`-escapes name the characters they encode (`/caf%C3%A9/` is `/café/`)
    let path = percent_decode(value.split(['?', '#']).next().unwrap_or(""));
    let joined = if path.starts_with('/') {
        path
    } else {
        format!("{base}{path}")
    };
    let mut segments: Vec<&str> = Vec::new();
    for seg in joined.split('/').skip(1) {
        match seg {
            "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return Some(None);
                }
            }
            _ => segments.push(seg),
        }
    }
    Some(Some(format!("/{}", segments.join("/"))))
}

/// Every stylesheet in the build, sorted.
fn css_files(build_dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(build_dir) {
        let entry = entry?;
        if entry.file_type().is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("css"))
        {
            files.push(entry.path().strip_prefix(build_dir)?.to_path_buf());
        }
    }
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_written_absolute_to_the_site_itself_are_checked() {
        let own = SiteAddress::parse("https://example.com").unwrap();
        let own = Some(&own);
        let base = "/books/dune/";
        for written in [
            "https://example.com/stocks/KO/",
            "http://example.com/stocks/KO/",
            "//example.com/stocks/KO/",
            "HTTPS://Example.COM/stocks/KO/",
        ] {
            assert_eq!(
                resolve(base, written, own),
                Some(Some("/stocks/KO/".into())),
                "{written}"
            );
        }
        assert_eq!(
            resolve(base, "https://example.com", own),
            Some(Some("/".into()))
        );
        assert_eq!(
            resolve(base, "https://example.com?x=1", own),
            Some(Some("/".into()))
        );
        assert_eq!(
            resolve(base, "https://example.com/a.css?v=2", own),
            Some(Some("/a.css".into()))
        );
        for other in [
            "https://other.example/stocks/KO/",
            "https://example.com.evil.example/x",
            "https://sub.example.com/x",
            "ftp://example.com/x",
        ] {
            assert_eq!(resolve(base, other, own), None, "{other}");
        }
        // a base_url with a path: only URLs under it are the site's
        let blog = SiteAddress::parse("https://example.com/blog/").unwrap();
        let blog = Some(&blog);
        assert_eq!(
            resolve(base, "https://example.com/blog/x/", blog),
            Some(Some("/x/".into()))
        );
        assert_eq!(
            resolve(base, "https://example.com/blog", blog),
            Some(Some("/".into()))
        );
        assert_eq!(resolve(base, "https://example.com/blogroll/", blog), None);
        assert_eq!(resolve(base, "https://example.com/x/", blog), None);
        // without base_url, absolute URLs stay another site's
        assert_eq!(resolve(base, "https://example.com/stocks/KO/", None), None);
    }

    #[test]
    fn urls_resolve_like_a_browser() {
        let base = "/books/dune/";
        assert_eq!(resolve(base, "../", None), Some(Some("/books/".into())));
        assert_eq!(
            resolve(base, "../../index/", None),
            Some(Some("/index/".into()))
        );
        assert_eq!(
            resolve(base, "img/a.png?v=2#x", None),
            Some(Some("/books/dune/img/a.png".into()))
        );
        assert_eq!(
            resolve(base, "./a.html", None),
            Some(Some("/books/dune/a.html".into()))
        );
        assert_eq!(resolve(base, "../../../x", None), Some(None));
        assert_eq!(resolve("/", "/a/../b", None), Some(Some("/b".into())));
        for external in [
            "https://x.example/a",
            "mailto:a@b.c",
            "data:image/png;base64,AA",
            "//cdn.example/x",
            "#top",
            "?page=2",
            "",
            "tel:+1",
            "javascript:void(0)",
        ] {
            assert_eq!(resolve(base, external, None), None, "{external}");
        }
        assert_eq!(url_dir(Path::new("books/dune/index.html")), "/books/dune/");
        assert_eq!(url_dir(Path::new("about.html")), "/");
        assert_eq!(url_dir(Path::new("css/site.css")), "/css/");
    }

    #[test]
    fn srcset_style_and_css_references_are_found() {
        let html = r#"<img srcset="/a.png 1x, b.png 2x"><div style="background:url('c.png')"></div>
            <style>/* url(skip.png) */ .x { background: url("/d.png") } @import "e.css";</style>"#;
        let found: Vec<(String, String)> = html_refs(html)
            .into_iter()
            .map(|r| (r.attribute, r.written))
            .collect();
        for want in [
            ("srcset", "/a.png"),
            ("srcset", "b.png"),
            ("url()", "c.png"),
            ("url()", "/d.png"),
            ("@import", "e.css"),
        ] {
            assert!(
                found.contains(&(want.0.to_string(), want.1.to_string())),
                "{want:?} in {found:?}"
            );
        }
        assert!(!found.iter().any(|(_, w)| w == "skip.png"), "{found:?}");
    }
}
