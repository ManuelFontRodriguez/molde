use crate::site::Site;
use anyhow::Result;
use std::fs;
use std::path::Path;

/// Conformance check (site.yaml `budgets:`): every build file a rule matches stays
/// within its limits. The gzip size is the file's .gz sibling; a file without one
/// (binaries) is measured raw. The .gz siblings themselves are never matched.
pub(crate) fn check_budgets(build_dir: &Path, site: &Site) -> Result<Vec<serde_json::Value>> {
    let Some(budgets) = &site.config.budgets else {
        return Ok(Vec::new());
    };
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(build_dir) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy();
        if let Some(stem) = name.strip_suffix(".gz") {
            if path.with_file_name(stem).is_file() {
                continue;
            }
        }
        files.push(path.strip_prefix(build_dir)?.to_path_buf());
    }
    files.sort();
    let mut errors = Vec::new();
    for b in budgets {
        let re = glob_regex(&b.pattern);
        let mut matched = false;
        for rel in &files {
            let url = format!("/{}", rel.to_string_lossy().replace('\\', "/"));
            if !re.is_match(&url) {
                continue;
            }
            matched = true;
            let full = build_dir.join(rel);
            let bytes = fs::metadata(&full)?.len();
            let mut gz_name = full.file_name().unwrap().to_os_string();
            gz_name.push(".gz");
            let gz = full.with_file_name(gz_name);
            let gzip_bytes = if gz.is_file() {
                fs::metadata(&gz)?.len()
            } else {
                bytes
            };
            for (metric, size, limit) in [
                ("bytes", bytes, b.max_bytes),
                ("gzip_bytes", gzip_bytes, b.max_gzip_bytes),
            ] {
                let Some(limit) = limit else { continue };
                if size > limit {
                    let mut e = serde_json::json!({
                        "layer": "output",
                        "check": "budget.exceeded",
                        "file": url,
                        "match": b.pattern,
                        "metric": metric,
                        "size": size,
                        "limit": limit,
                    });
                    // run_check attaches route + template when a route wrote this file
                    e["page"] = serde_json::json!(url.trim_start_matches('/'));
                    errors.push(e);
                }
            }
        }
        if !matched {
            errors.push(serde_json::json!({
                "layer": "output",
                "check": "budget.unmatched",
                "match": b.pattern,
            }));
        }
    }
    Ok(errors)
}

/// Compile a budget glob to an anchored regex over "/"-rooted build paths.
pub fn glob_regex(glob: &str) -> regex::Regex {
    // globs match files; a trailing "/" means everything under that directory
    let glob = &match glob.strip_suffix('/') {
        Some(dir) => format!("{dir}/**"),
        None => glob.to_string(),
    };
    let (mut re, rest) = match glob.strip_prefix('/') {
        Some(rest) => (String::from("^/"), rest),
        None => (String::from("^(?:.*/)?"), glob.as_str()),
    };
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                if chars.peek() == Some(&'/') {
                    chars.next();
                    re.push_str("(?:.*/)?");
                } else {
                    re.push_str(".*");
                }
            }
            '*' => re.push_str("[^/]*"),
            '?' => re.push_str("[^/]"),
            c => re.push_str(&regex::escape(&c.to_string())),
        }
    }
    re.push('$');
    regex::Regex::new(&re).expect("escaped glob is a valid regex")
}
