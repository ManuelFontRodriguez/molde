use crate::store::list_builds;
use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Report-size cap: first_diff detail only for this many changed pages.
const DIFF_DETAIL_CAP: usize = 20;

/// Cap on full-diff lines emitted per changed page.
const DIFF_LINES_CAP: usize = 200;

/// Diff two immutable builds (default: the two most recent). The review unit
/// for a change: added/removed/changed page sets; `full` returns the changed
/// lines per page (LCS-based), otherwise just the first differing line.
pub fn diff_builds(
    out: &Path,
    from: Option<&str>,
    to: Option<&str>,
    full: bool,
) -> Result<serde_json::Value> {
    let ids = list_builds(out)?;
    let to = match to {
        Some(t) => t.to_string(),
        None => ids.last().context("no builds to diff")?.clone(),
    };
    let from = match from {
        Some(f) => f.to_string(),
        None => {
            let idx = ids
                .iter()
                .position(|i| *i == to)
                .with_context(|| format!("build '{to}' not found"))?;
            if idx == 0 {
                bail!("nothing to diff against: '{to}' is the oldest build");
            }
            ids[idx - 1].clone()
        }
    };
    // Only ids that list_builds returned: a caller-supplied id is never joined
    // into a path, so `../x` or `/etc` cannot read files outside out/builds.
    for id in [&from, &to] {
        if !ids.contains(id) {
            bail!("build '{id}' not found");
        }
    }
    let dir_a = out.join("builds").join(&from);
    let dir_b = out.join("builds").join(&to);
    let set_a: BTreeSet<PathBuf> = text_files(&dir_a)?.into_iter().collect();
    let set_b: BTreeSet<PathBuf> = text_files(&dir_b)?.into_iter().collect();

    let added: Vec<String> = set_b
        .difference(&set_a)
        .map(|p| p.display().to_string())
        .collect();
    let removed: Vec<String> = set_a
        .difference(&set_b)
        .map(|p| p.display().to_string())
        .collect();
    let mut changed = Vec::new();
    let mut unchanged = 0usize;
    for rel in set_a.intersection(&set_b) {
        let bytes_a = fs::read(dir_a.join(rel))?;
        let bytes_b = fs::read(dir_b.join(rel))?;
        if bytes_a == bytes_b {
            unchanged += 1;
        } else if changed.len() >= DIFF_DETAIL_CAP {
            changed.push(serde_json::json!({ "page": rel.display().to_string() }));
        } else if full {
            let (lines, truncated, minimal) = line_diff(
                &String::from_utf8_lossy(&bytes_a),
                &String::from_utf8_lossy(&bytes_b),
                DIFF_LINES_CAP,
            );
            let mut entry = serde_json::json!({
                "page": rel.display().to_string(),
                "diff": lines,
                "diff_truncated": truncated,
            });
            if !minimal {
                // a changed region too large to minimise: listed whole, removed then added
                entry["diff_minimal"] = serde_json::json!(false);
            }
            changed.push(entry);
        } else {
            changed.push(serde_json::json!({
                "page": rel.display().to_string(),
                "first_diff": first_diff(&bytes_a, &bytes_b),
            }));
        }
    }
    Ok(serde_json::json!({
        "from": from,
        "to": to,
        "added": added,
        "removed": removed,
        "changed": changed,
        "unchanged": unchanged,
    }))
}

/// Changed lines only (LCS-based), formatted "-<old line no>: text" / "+<new line no>: text".
/// Small inputs by construction (single rendered pages), so O(n·m) is fine.
/// The comparison table holds one cell per (old line, new line) pair of the region
/// that differs: above this many cells (64 MB) the diff is not minimised.
const DIFF_MAX_CELLS: usize = 16_000_000;

/// Changed lines (`-n: old`, `+n: new`), at most `max_lines`; returns whether the list
/// was cut short and whether it is minimal. Lines equal at the start and the end are
/// skipped before comparing, so a small change in a long page is cheap; a changed
/// region too large to minimise is listed as all old lines removed, then all new lines
/// added (still correct, not minimal).
fn line_diff(a: &str, b: &str, max_lines: usize) -> (Vec<String>, bool, bool) {
    let lines_a: Vec<&str> = a.lines().collect();
    let lines_b: Vec<&str> = b.lines().collect();
    let prefix = lines_a
        .iter()
        .zip(&lines_b)
        .take_while(|(x, y)| x == y)
        .count();
    let suffix = lines_a[prefix..]
        .iter()
        .rev()
        .zip(lines_b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let mid_a = &lines_a[prefix..lines_a.len() - suffix];
    let mid_b = &lines_b[prefix..lines_b.len() - suffix];
    let (n, m) = (mid_a.len(), mid_b.len());
    let mut out = Vec::new();
    let mut push = |line: String| {
        if out.len() >= max_lines {
            return false;
        }
        out.push(line);
        true
    };
    let minimal = (n + 1).saturating_mul(m + 1) <= DIFF_MAX_CELLS;
    if !minimal {
        for (k, line) in mid_a.iter().enumerate() {
            if !push(format!("-{}: {}", prefix + k + 1, line)) {
                return (out, true, false);
            }
        }
        for (k, line) in mid_b.iter().enumerate() {
            if !push(format!("+{}: {}", prefix + k + 1, line)) {
                return (out, true, false);
            }
        }
        return (out, false, false);
    }
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if mid_a[i] == mid_b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        let line = if i < n && j < m && mid_a[i] == mid_b[j] {
            i += 1;
            j += 1;
            continue;
        } else if j == m || (i < n && lcs[i + 1][j] >= lcs[i][j + 1]) {
            i += 1;
            format!("-{}: {}", prefix + i, mid_a[i - 1])
        } else {
            j += 1;
            format!("+{}: {}", prefix + j, mid_b[j - 1])
        };
        if !push(line) {
            return (out, true, true);
        }
    }
    (out, false, true)
}

fn first_diff(a: &[u8], b: &[u8]) -> serde_json::Value {
    let a = String::from_utf8_lossy(a);
    let b = String::from_utf8_lossy(b);
    for (i, (line_a, line_b)) in a.lines().zip(b.lines()).enumerate() {
        if line_a != line_b {
            return serde_json::json!({ "line": i + 1, "from": line_a, "to": line_b });
        }
    }
    // one file is a line-prefix of the other
    let (n_a, n_b) = (a.lines().count(), b.lines().count());
    serde_json::json!({
        "line": n_a.min(n_b) + 1,
        "from": a.lines().nth(n_b),
        "to": b.lines().nth(n_a),
    })
}

/// Sorted list of every rendered file except the .gz siblings (relative paths).
fn text_files(build_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(build_dir) {
        let entry = entry?;
        if entry.file_type().is_file()
            && entry.path().extension().and_then(|e| e.to_str()) != Some("gz")
        {
            files.push(entry.path().strip_prefix(build_dir)?.to_path_buf());
        }
    }
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod line_diff_limits {
    use super::*;
    use std::time::Instant;

    #[test]
    fn a_small_change_in_a_long_page_is_exact_and_cheap() {
        let a: String = (0..50_000).map(|n| format!("line {n}\n")).collect();
        let b = a.replace("line 25000\n", "line 25000 changed\n");
        let started = Instant::now();
        let (lines, truncated, minimal) = line_diff(&a, &b, 200);
        assert_eq!(lines, ["-25001: line 25000", "+25001: line 25000 changed"]);
        assert!(!truncated && minimal);
        assert!(started.elapsed().as_secs() < 2, "{:?}", started.elapsed());
    }

    #[test]
    fn a_huge_changed_region_is_listed_without_the_table() {
        // 10,000 × 10,000 different lines would need 400 MB of table
        let a: String = (0..10_000).map(|n| format!("a{n}\n")).collect();
        let b: String = (0..10_000).map(|n| format!("b{n}\n")).collect();
        let (lines, truncated, minimal) = line_diff(&a, &b, 200);
        assert!(!minimal && truncated);
        assert_eq!(lines.len(), 200);
        assert_eq!(lines[0], "-1: a0");
        let (all, truncated, _) = line_diff(&a, &b, usize::MAX);
        assert!(!truncated);
        assert_eq!(all.len(), 20_000);
        assert_eq!(all[10_000], "+1: b0");
    }

    #[test]
    fn edits_between_shared_lines_keep_their_line_numbers() {
        let (lines, _, minimal) = line_diff("a\nb\nc\nd\n", "a\nB\nc\nd\nE\n", 200);
        assert!(minimal);
        assert_eq!(lines, ["-2: b", "+2: B", "+5: E"]);
    }
}
