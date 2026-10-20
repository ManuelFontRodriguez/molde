//! How verify reads a URL (docs/design/verify.md §6): which file of the build it names,
//! given the site's public address, a base path, clean URLs and `<base href>`.
use crate::check::html::{percent_decode, unescape_entities};
use std::collections::BTreeSet;
use std::sync::OnceLock;

/// The site's public address, split as a browser compares it.
#[derive(Debug, Clone)]
pub(crate) struct Address {
    /// `https://example.com` (scheme and host, lower-cased, no trailing slash).
    pub(crate) origin: String,
    /// The base path the site is served under: `/` or `/repo/` (always ends with `/`).
    pub(crate) base: String,
}

impl Address {
    pub(crate) fn parse(base_url: &str) -> Option<Self> {
        let lower = base_url.to_ascii_lowercase();
        let scheme_end = lower.find("://")? + 3;
        if !lower.starts_with("http://") && !lower.starts_with("https://") {
            return None;
        }
        let rest = &base_url[scheme_end..];
        let host_end = rest.find('/').unwrap_or(rest.len());
        let host = &rest[..host_end];
        if host.is_empty() {
            return None;
        }
        let path = rest[host_end..].trim_end_matches('/');
        // an address is a scheme, a host and a path: a query or fragment is not a base
        if host.contains(['?', '#', '@']) || path.contains(['?', '#']) {
            return None;
        }
        // links are compared decoded (`resolve` decodes each segment), so the base is too:
        // `/caf%C3%A9/` and `/café/` are one base path
        let path: Vec<String> = path.split('/').map(percent_decode).collect();
        Some(Self {
            origin: format!("{}{}", &lower[..scheme_end], host.to_ascii_lowercase()),
            base: format!("{}/", path.join("/")),
        })
    }

    /// The public URL of a site path (`/x/` under base `/repo/` is `<origin>/repo/x/`).
    pub(crate) fn url_of(&self, site_path: &str) -> String {
        format!(
            "{}{}{}",
            self.origin,
            self.base.trim_end_matches('/'),
            site_path
        )
    }

    /// The path part of `url` if it is on this site's host (any scheme of http/https,
    /// protocol-relative too, host in any case). None for another host.
    fn own_public_path(&self, url: &str) -> Option<String> {
        let host = self.origin.split_once("://").map_or("", |(_, h)| h);
        let rest = url
            .strip_prefix("//")
            .or_else(|| strip_ci(url, "https://"))
            .or_else(|| strip_ci(url, "http://"))?;
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        if !rest[..end].eq_ignore_ascii_case(host) {
            return None;
        }
        let path = &rest[end..];
        Some(if path.is_empty() || path.starts_with(['?', '#']) {
            format!("/{path}")
        } else {
            path.to_string()
        })
    }
}

/// `s` without an ASCII `prefix` matched in any case. Compared as bytes: `s` may hold
/// any text (`カテゴリ/`), and a cut inside a character would panic.
fn strip_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    s.as_bytes()
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix.as_bytes()))
        .then(|| &s[prefix.len()..])
}

/// Is `written` relative to the page's folder (no scheme, not root-relative, not a bare
/// fragment or query)? Those are the links a `<base href>` moves.
pub(crate) fn is_relative(written: &str) -> bool {
    let v = unescape_entities(written.trim());
    !(v.is_empty()
        || v.starts_with(['/', '#', '?'])
        || v.split(['/', '?', '#'])
            .next()
            .is_some_and(|first| first.contains(':')))
}

/// What a written URL means for this build.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Target {
    /// Not this build's: another site, a scheme (`mailto:`, `data:`), a bare `#fragment`
    /// or `?query`, an empty value.
    Elsewhere,
    /// A path of this site, relative to the build folder (`/x/`), query and fragment gone.
    Site(String),
    /// On this host, but outside the base path: works on disk, 404s in production.
    OutsideBase(String),
    /// Climbs above the site root (`../../..`).
    AboveRoot,
}

/// Resolve `written` as a browser would from a page whose public folder is `page_dir`
/// (a public path ending in `/`, base path included), or from its `<base href>`.
pub(crate) fn resolve(
    written: &str,
    page_dir: &str,
    base_href: Option<&str>,
    site: Option<&Address>,
) -> Target {
    static SCHEME: OnceLock<regex::Regex> = OnceLock::new();
    let scheme = SCHEME.get_or_init(|| regex::Regex::new(r"^[A-Za-z][A-Za-z0-9+.-]*:").unwrap());
    let value = unescape_entities(written.trim());
    // a URL written absolute to the site's own address is its public path
    let value = match site.and_then(|s| s.own_public_path(&value)) {
        Some(path) => path,
        None => value,
    };
    if value.is_empty() || value.starts_with('#') || value.starts_with('?') {
        return Target::Elsewhere;
    }
    if value.starts_with("//") || scheme.is_match(&value) {
        return Target::Elsewhere;
    }
    // %-escapes are decoded per segment, after the split: `%2F` is part of a name, not a
    // separator, so `a%2Fb` names no file rather than `a/b`
    let path = value.split(['?', '#']).next().unwrap_or("");
    let dir = base_href.unwrap_or(page_dir);
    let joined = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("{dir}{path}")
    };
    let decoded: Vec<String> = joined
        .split('/')
        .map(|seg| percent_decode(seg).replace('/', "%2F"))
        .collect();
    let joined = decoded.join("/");
    let mut segments: Vec<&str> = Vec::new();
    let trailing = joined.ends_with('/') || joined.ends_with("/.") || joined.ends_with("/..");
    for seg in joined.split('/').skip(1) {
        match seg {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return Target::AboveRoot;
                }
            }
            _ => segments.push(seg),
        }
    }
    let mut public = format!("/{}", segments.join("/"));
    if trailing && !public.ends_with('/') {
        public.push('/');
    }
    let base = site.map_or("/", |s| s.base.as_str());
    if base == "/" {
        return Target::Site(public);
    }
    let base_no_slash = base.trim_end_matches('/');
    if public == base_no_slash || public == base {
        return Target::Site("/".into());
    }
    match public.strip_prefix(base_no_slash) {
        Some(rest) if rest.starts_with('/') => Target::Site(rest.to_string()),
        _ => Target::OutsideBase(public),
    }
}

/// The build's files, to resolve site paths against.
pub(crate) struct Files {
    files: BTreeSet<String>,
    clean_urls: bool,
}

impl Files {
    pub(crate) fn new(files: impl IntoIterator<Item = String>, clean_urls: bool) -> Self {
        Self {
            files: files.into_iter().collect(),
            clean_urls,
        }
    }

    /// The file a site path names, as the host serves it: the file itself, a folder's
    /// `index.html`, and with clean URLs `/a` from `a.html`.
    pub(crate) fn file_for(&self, site_path: &str) -> Option<String> {
        let rel = site_path.trim_start_matches('/');
        let rel = rel.trim_end_matches('/');
        let candidates = [
            (!rel.is_empty() && !site_path.ends_with('/')).then(|| rel.to_string()),
            Some(if rel.is_empty() {
                "index.html".to_string()
            } else {
                format!("{rel}/index.html")
            }),
            (self.clean_urls && !rel.is_empty()).then(|| format!("{rel}.html")),
        ];
        candidates
            .into_iter()
            .flatten()
            .find(|c| self.files.contains(c))
    }

    /// Would the path resolve with clean URLs on? (For the grouped hint.)
    pub(crate) fn would_resolve_clean(&self, site_path: &str) -> bool {
        let rel = site_path.trim_start_matches('/').trim_end_matches('/');
        !rel.is_empty() && self.files.contains(&format!("{rel}.html"))
    }
}

/// The public folder a page is served from: `x/index.html` under base `/repo/` is
/// `/repo/x/`; `about.html` is `/repo/`.
pub(crate) fn page_dir(rel: &str, site: Option<&Address>) -> String {
    let base = site.map_or("/", |s| s.base.as_str());
    match rel.rfind('/') {
        Some(i) => format!("{base}{}/", &rel[..i]),
        None => base.to_string(),
    }
}

/// The site path of a page file: `x/index.html` is `/x/`, `a.html` is `/a.html` (or `/a`
/// with clean URLs), `index.html` is `/`.
pub(crate) fn site_path_of(rel: &str, clean_urls: bool) -> String {
    if rel == "index.html" {
        return "/".into();
    }
    if let Some(dir) = rel.strip_suffix("/index.html") {
        return format!("/{dir}/");
    }
    if clean_urls {
        if let Some(stem) = rel.strip_suffix(".html") {
            return format!("/{stem}");
        }
    }
    format!("/{rel}")
}

/// Do two URLs name the same page, as a browser compares them? Scheme and host in any
/// case, an empty path the same as `/`, the path exactly.
pub(crate) fn same_url(a: &str, b: &str) -> bool {
    fn parts(u: &str) -> (String, &str) {
        let host_at = u.find("://").map_or(0, |i| i + 3);
        let path_at = u[host_at..].find('/').map_or(u.len(), |i| host_at + i);
        let path = &u[path_at..];
        (
            u[..path_at].to_ascii_lowercase(),
            if path.is_empty() { "/" } else { path },
        )
    }
    parts(a.trim()) == parts(b.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_and_base_paths() {
        let a = Address::parse("https://User.GitHub.io/repo/").unwrap();
        assert_eq!(a.origin, "https://user.github.io");
        assert_eq!(a.base, "/repo/");
        assert_eq!(Address::parse("https://x.example").unwrap().base, "/");
        assert!(Address::parse("ftp://x").is_none());
        for bad in [
            "example.com",
            "https://",
            "https://x.example/?q",
            "https://x.example/#f",
        ] {
            assert!(Address::parse(bad).is_none(), "{bad}");
        }
        assert_eq!(a.url_of("/x/"), "https://user.github.io/repo/x/");
        assert_eq!(
            Address::parse("https://x.example/caf%C3%A9/").unwrap().base,
            "/café/"
        );
    }

    #[test]
    fn links_resolve_against_the_base_path_and_must_stay_under_it() {
        let site = Address::parse("https://user.github.io/repo/").unwrap();
        let s = Some(&site);
        let dir = page_dir("posts/a/index.html", s);
        assert_eq!(dir, "/repo/posts/a/");
        assert_eq!(
            resolve("/repo/posts/b/", &dir, None, s),
            Target::Site("/posts/b/".into())
        );
        assert_eq!(
            resolve("../b/", &dir, None, s),
            Target::Site("/posts/b/".into())
        );
        assert_eq!(
            resolve("https://user.github.io/repo/", &dir, None, s),
            Target::Site("/".into())
        );
        assert_eq!(
            resolve("/posts/b/", &dir, None, s),
            Target::OutsideBase("/posts/b/".into())
        );
        assert_eq!(
            resolve("https://user.github.io/other/", &dir, None, s),
            Target::OutsideBase("/other/".into())
        );
        assert_eq!(
            resolve("/repository/", &dir, None, s),
            Target::OutsideBase("/repository/".into())
        );
        assert_eq!(
            resolve("https://elsewhere.example/x", &dir, None, s),
            Target::Elsewhere
        );
        assert_eq!(resolve("#top", &dir, None, s), Target::Elsewhere);
        assert_eq!(resolve("../../../../x", &dir, None, s), Target::AboveRoot);
    }

    #[test]
    fn base_href_and_root_sites() {
        let site = Address::parse("https://x.example").unwrap();
        let s = Some(&site);
        assert_eq!(
            resolve("b.css", "/a/", Some("/assets/"), s),
            Target::Site("/assets/b.css".into())
        );
        assert_eq!(
            resolve("HTTPS://X.Example", "/", None, s),
            Target::Site("/".into())
        );
        assert_eq!(
            resolve("/a?q=1#f", "/", None, None),
            Target::Site("/a".into())
        );
    }

    /// Non-ASCII hrefs used to panic (VitePress's ja/zh pages); `%2F` is not a separator.
    #[test]
    fn any_text_and_escaped_slashes() {
        let site = Address::parse("https://x.example").unwrap();
        let s = Some(&site);
        for written in ["カテゴリ/", "ü", "日本語/ページ.html", "h"] {
            assert!(
                matches!(resolve(written, "/a/", None, s), Target::Site(_)),
                "{written}"
            );
        }
        assert_eq!(
            resolve("caf%C3%A9/", "/", None, s),
            Target::Site("/café/".into())
        );
        assert_eq!(
            resolve("%2Fother%2Fx.html", "/a/", None, s),
            Target::Site("/a/%2Fother%2Fx.html".into())
        );
    }

    #[test]
    fn files_resolve_as_hosts_serve_them() {
        let f = Files::new(
            ["index.html", "a/index.html", "b.html", "c.css"].map(String::from),
            false,
        );
        assert_eq!(f.file_for("/").as_deref(), Some("index.html"));
        assert_eq!(f.file_for("/a/").as_deref(), Some("a/index.html"));
        assert_eq!(f.file_for("/a").as_deref(), Some("a/index.html"));
        assert_eq!(f.file_for("/c.css").as_deref(), Some("c.css"));
        assert_eq!(f.file_for("/b"), None);
        assert!(f.would_resolve_clean("/b"));
        let clean = Files::new(["b.html".to_string()], true);
        assert_eq!(clean.file_for("/b").as_deref(), Some("b.html"));
        assert_eq!(site_path_of("x/index.html", false), "/x/");
        assert_eq!(site_path_of("b.html", true), "/b");
        assert!(same_url("https://X.example", "https://x.example/"));
        assert!(!same_url("https://x.example/A", "https://x.example/a"));
    }
}
