use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Sorted list of the .html files in a build (relative paths) — deterministic check order.
pub(crate) fn html_files(build_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(build_dir) {
        let entry = entry?;
        if entry.file_type().is_file()
            && entry.path().extension().and_then(|e| e.to_str()) == Some("html")
        {
            files.push(entry.path().strip_prefix(build_dir)?.to_path_buf());
        }
    }
    files.sort();
    Ok(files)
}

/// `<meta …>` tags.
pub(crate) fn meta_tag_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(?is)<meta\b[^>]*>").unwrap())
}

/// `<link …>` tags.
pub(crate) fn link_tag_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(?is)<link\b[^>]*>").unwrap())
}

/// The `<html …>` start tag.
pub(crate) fn html_tag_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(?is)<html\b[^>]*>").unwrap())
}

/// The attributes of a tag, as HTML allows them: `name="v"`, `name='v'` or `name=v`
/// (group 1 is the name; the value is in group 2, 3 or 4, see `attr_value`).
pub(crate) fn attr_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r#"([\w:-]+)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+))"#).unwrap()
    })
}

/// The value an `attr_re` match captured, whichever way it was quoted.
pub(crate) fn attr_value<'a>(cap: &regex::Captures<'a>) -> &'a str {
    (2..=4).find_map(|i| cap.get(i)).map_or("", |v| v.as_str())
}

/// Scripts, styles and comments: text that is not markup.
pub(crate) fn code_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"(?is)<script\b.*?</script\s*>|<style\b.*?</style\s*>|<!--.*?-->")
            .unwrap()
    })
}

/// `%XX` escapes decoded to the bytes they encode, read as UTF-8; a malformed escape
/// stays as written.
pub(crate) fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Does the page carry `<meta name="robots" content="…noindex…">` (or `none`)?
/// Attribute-order-independent and case-insensitive, like the head checks; values may be
/// double-quoted, single-quoted or unquoted (`<meta name=robots content=noindex>`).
/// A tag inside a comment, a script, a style or a `<template>` is not the page's: a
/// commented-out noindex leaves the page indexable, as crawlers read it.
pub(crate) fn has_noindex(html: &str) -> bool {
    // the spans are found only for a page that has a noindex tag at all, which most don't
    let mut hidden: Option<Vec<(usize, usize)>> = None;
    let mut in_hidden = |at: usize| {
        let spans = hidden.get_or_insert_with(|| {
            static TEMPLATE: OnceLock<regex::Regex> = OnceLock::new();
            let template = TEMPLATE
                .get_or_init(|| regex::Regex::new(r"(?is)<template\b.*?</template\s*>").unwrap());
            let mut v: Vec<(usize, usize)> = code_re()
                .find_iter(html)
                .chain(template.find_iter(html))
                .map(|m| (m.start(), m.end()))
                .collect();
            v.sort_unstable();
            v
        });
        spans.iter().any(|&(a, b)| a <= at && at < b)
    };
    meta_tag_re().find_iter(html).any(|m| {
        if !robots_noindex(m.as_str()) {
            return false;
        }
        !in_hidden(m.start())
    })
}

fn robots_noindex(tag: &str) -> bool {
    let mut is_robots = false;
    let mut noindex = false;
    for cap in attr_re().captures_iter(tag) {
        let value = attr_value(&cap);
        // an unquoted value of a self-closed tag keeps the slash: `content=noindex/>`
        let value = value.trim_end_matches('/');
        match cap[1].to_ascii_lowercase().as_str() {
            "name" => is_robots = value.eq_ignore_ascii_case("robots"),
            "content" => {
                noindex = value.split(',').any(|d| {
                    let d = d.trim();
                    d.eq_ignore_ascii_case("noindex") || d.eq_ignore_ascii_case("none")
                })
            }
            _ => {}
        }
    }
    is_robots && noindex
}

/// Attribute-context escape: quotes are covered, so a value cannot break out of
/// an href/content attribute; '/' is deliberately left readable (URLs).
pub(crate) fn attr_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// HTML character references in an attribute value, decoded in one pass (so `&amp;lt;`
/// stays `&lt;`): the named ones markup uses for URLs and text (`&amp; &lt; &gt; &quot;
/// &apos;`) and every numeric one (`&#47;`, `&#x2F;`). Anything else stays as written.
pub(crate) fn unescape_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let decoded = tail.find(';').filter(|&end| end <= 10).and_then(|end| {
            let name = &tail[1..end];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => {
                    let num = name.strip_prefix('#')?;
                    let code = match num.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => num.parse().ok()?,
                    };
                    char::from_u32(code)
                }
            }?;
            Some((c, end + 1))
        });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &tail[len..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

pub(crate) fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
