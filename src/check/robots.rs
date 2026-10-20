//! robots.txt (ships with base_url): molde writes one that names the sitemap, and a
//! `static/robots.txt` replaces it. Whichever is in the build must still name the
//! sitemap and must not shut every crawler out of a site that has indexable pages.
use anyhow::Result;
use std::path::Path;

/// `robots.sitemap_missing`: no `Sitemap:` line names `<base_url>/sitemap.xml` (`found`
/// lists the ones there are). `robots.blocks_all`: the rules for every crawler
/// (`User-agent: *`) disallow `/` and allow nothing back, while the site has pages to
/// index (`indexable`). Read as RFC 9309 reads the file. No robots.txt in the build, or
/// no `base_url`, means nothing to check.
pub fn check_robots(
    build_dir: &Path,
    base_url: Option<&str>,
    indexable: bool,
) -> Result<Vec<serde_json::Value>> {
    let Some(base) = base_url.map(|b| b.trim_end_matches('/')) else {
        return Ok(Vec::new());
    };
    let path = build_dir.join("robots.txt");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let lines = parse(&String::from_utf8_lossy(&std::fs::read(&path)?));

    let mut errors = Vec::new();
    let expected = format!("{base}/sitemap.xml");
    let sitemaps: Vec<&str> = lines
        .iter()
        .filter(|(k, _)| k == "sitemap")
        .map(|(_, v)| v.as_str())
        .collect();
    if !sitemaps.iter().any(|s| same_url(s, &expected)) {
        errors.push(serde_json::json!({
            "layer": "output",
            "check": "robots.sitemap_missing",
            "file": "robots.txt",
            "expected": expected,
            "found": sitemaps,
        }));
    }
    if indexable && blocks_everyone(&lines) {
        errors.push(serde_json::json!({
            "layer": "output",
            "check": "robots.blocks_all",
            "file": "robots.txt",
        }));
    }
    Ok(errors)
}

/// The file's `key: value` lines, keys lower-cased: a byte order mark, `#` comments,
/// blank lines and lines without a colon are dropped; CRLF ends a line like LF.
pub(crate) fn parse(text: &str) -> Vec<(String, String)> {
    text.trim_start_matches('\u{feff}')
        .lines()
        .filter_map(|line| {
            let line = line.split('#').next().unwrap_or("").trim();
            let (key, value) = line.split_once(':')?;
            Some((key.trim().to_ascii_lowercase(), value.trim().to_string()))
        })
        .collect()
}

/// The same URL, as a crawler compares them: scheme and host in any case, the rest exactly.
fn same_url(a: &str, b: &str) -> bool {
    let split = |u: &str| -> (String, String) {
        let host_end = u.find("://").map_or(0, |i| i + 3);
        let path_at = u[host_end..].find('/').map_or(u.len(), |i| host_end + i);
        (u[..path_at].to_ascii_lowercase(), u[path_at..].to_string())
    };
    split(a) == split(b)
}

/// Do the rules for every crawler disallow the whole site? RFC 9309 merges every group
/// that names `*`: `Disallow: /` (or `/*`) in them, and no non-empty `Allow` in them
/// (any `Allow` opens part of the site again).
pub(crate) fn blocks_everyone(lines: &[(String, String)]) -> bool {
    let (mut agents, mut in_rules) = (Vec::<&str>::new(), false);
    let (mut disallow_root, mut allows) = (false, false);
    for (key, value) in lines {
        match key.as_str() {
            "user-agent" => {
                if in_rules {
                    agents.clear();
                    in_rules = false;
                }
                agents.push(value.as_str());
            }
            "disallow" | "allow" if agents.contains(&"*") => {
                in_rules = true;
                if key == "disallow" {
                    disallow_root |= value == "/" || value == "/*";
                } else {
                    allows |= !value.is_empty();
                }
            }
            "disallow" | "allow" => in_rules = true,
            _ => {}
        }
    }
    disallow_root && !allows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocks(text: &str) -> bool {
        blocks_everyone(&parse(text))
    }

    #[test]
    fn the_file_is_read_as_rfc_9309_reads_it() {
        assert_eq!(
            parse("\u{feff}Sitemap: https://x/s.xml # comment\r\nUSER-AGENT : *\r\n\r\n# all\r\nno colon"),
            [
                ("sitemap".to_string(), "https://x/s.xml".to_string()),
                ("user-agent".to_string(), "*".to_string())
            ]
        );
        assert!(same_url(
            "HTTPS://X.Example/sitemap.xml",
            "https://x.example/sitemap.xml"
        ));
        assert!(!same_url(
            "https://x.example/Sitemap.xml",
            "https://x.example/sitemap.xml"
        ));
    }

    #[test]
    fn only_closed_rules_for_every_crawler_block_all() {
        assert!(blocks("User-agent: *\nDisallow: /"));
        assert!(blocks("\u{feff}User-agent: *\r\nDisallow: / # staging\r\n"));
        assert!(blocks("User-agent: bot\nUser-agent: *\nDisallow: /"));
        assert!(blocks("User-agent: *\nDisallow: /*"));
        assert!(!blocks("User-agent: *\nDisallow: /\nAllow: /public/"));
        assert!(!blocks("User-agent: *\nDisallow: /admin/"));
        assert!(!blocks("User-agent: *\nDisallow:"));
        assert!(!blocks(
            "User-agent: badbot\nDisallow: /\n\nUser-agent: *\nAllow: /"
        ));
        assert!(!blocks(
            "User-agent: *\nAllow: /\n\nSitemap: https://x/s.xml"
        ));
        // two groups for *: merged, so the Allow in the second opens the site
        assert!(!blocks(
            "User-agent: *\nDisallow: /\n\nUser-agent: *\nAllow: /"
        ));
        // an empty Allow allows nothing
        assert!(blocks("User-agent: *\nDisallow: /\nAllow:"));
    }
}
