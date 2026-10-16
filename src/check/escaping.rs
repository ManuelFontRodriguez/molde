use crate::build::par_map;
use crate::check::html::html_files;
use crate::check::read_page;
use anyhow::Result;
use std::path::Path;
use std::sync::OnceLock;

/// Text escaped twice: `&amp;amp;` shows as "&amp;" in the browser, `&amp;lt;` as "&lt;".
/// It happens when a template escapes by hand what autoescape escapes again. One error
/// per page, with the first occurrence and the count. `<code>`, `<pre>`, `<textarea>`,
/// scripts, styles and comments are skipped: there an escaped entity can be the point.
pub(crate) fn check_escaping(build_dir: &Path) -> Result<Vec<serde_json::Value>> {
    static RES: OnceLock<(regex::Regex, regex::Regex)> = OnceLock::new();
    let (skip_re, twice_re) = RES.get_or_init(|| {
        (
            regex::Regex::new(
                r"(?is)<!--.*?-->|<script\b.*?</script\s*>|<style\b.*?</style\s*>|<pre\b.*?</pre\s*>|<code\b.*?</code\s*>|<textarea\b.*?</textarea\s*>",
            )
            .unwrap(),
            regex::Regex::new(r"&amp;(?:amp|lt|gt|quot|apos|nbsp|#[0-9]+|#[xX][0-9a-fA-F]+);")
                .unwrap(),
        )
    });
    let files = html_files(build_dir)?;
    let per_page = par_map(files.len(), |i| {
        let html = read_page(&build_dir.join(&files[i]))?;
        let text = skip_re.replace_all(&html, "");
        let mut found = twice_re.find_iter(&text);
        let Some(first) = found.next() else {
            return Ok(None);
        };
        // a little context around the first one, on one line
        let start = text[..first.start()]
            .char_indices()
            .rev()
            .nth(30)
            .map_or(0, |(i, _)| i);
        let end = text[first.end()..]
            .char_indices()
            .nth(30)
            .map_or(text.len(), |(i, _)| first.end() + i);
        let context: String = text[start..end]
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        Ok(Some(serde_json::json!({
            "layer": "output",
            "check": "html.double_escaped",
            "page": files[i].display().to_string(),
            "found": first.as_str(),
            "context": context.trim(),
            "count": 1 + found.count(),
        })))
    })?;
    Ok(per_page.into_iter().flatten().collect())
}
