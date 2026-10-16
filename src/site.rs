use crate::locales;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// routes.yaml — the router is data, not code.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Routes {
    pub routes: Vec<Route>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    /// Output path pattern, e.g. "/books/{id}/index.html". Under `locales:` it may
    /// be a map of per-locale patterns (`{es: /libros/{slug}/index.html, en: …}`);
    /// a locale the map leaves out gets no page.
    pub path: PathSpec,
    /// Template name inside templates/
    pub template: String,
    /// Optional: render once per item of this top-level collection in the snapshot.
    #[serde(default)]
    pub foreach: Option<String>,
    /// Optional, with `foreach`: one page per distinct value of this item field instead
    /// of one per item (genre or tag pages). The page's `item` is `{field: value,
    /// "items": [the items with that value]}`; a list field puts an item in every group.
    #[serde(default)]
    pub group_by: Option<String>,
    /// Optional: minijinja pattern for the page title, rendered per page as plain text.
    /// With web-standards on, std_head emits <title> + the full OG block from it.
    #[serde(default)]
    pub title: Option<String>,
    /// Optional: may search engines index these pages? Default true. `false` keeps the
    /// page rendered but out of sitemap.xml, and std_head emits a robots noindex meta
    /// instead of canonical + OG.
    #[serde(default)]
    pub index: Option<IndexRule>,
    /// Optional: minijinja pattern for the meta description, rendered per page as plain
    /// text like `title`. std_head emits <meta name="description"> and og:description.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional: minijinja pattern for the page's sitemap <lastmod>, rendered per page.
    /// Empty = omitted; otherwise YYYY-MM-DD or an RFC 3339 datetime. Derive it from
    /// the data (the last data change) — the framework never uses the build time.
    #[serde(default)]
    pub lastmod: Option<String>,
    /// Optional: schema.org @type(s) every page of this route must carry in its
    /// JSON-LD (top level, inside @graph, or nested). Checked by `jsonld.type_missing`.
    #[serde(default)]
    pub jsonld: Option<OneOrMany>,
    /// Optional: split a top-level array into pages of `per_page`. Page 1 renders at
    /// the route's own `path`, pages 2..n at `paginate.path`; templates and route
    /// patterns get `pagination`. Can't be combined with `foreach`.
    #[serde(default)]
    pub paginate: Option<Paginate>,
    /// Optional, under site.yaml `locales:`: the locales this route renders in (a
    /// subset, default all), or `none` — one page at its path verbatim, outside the
    /// locale dimension (a root chooser, a feed).
    #[serde(default)]
    pub locales: Option<RouteLocales>,
    /// Optional, under `locales:`: the item fields that are per-locale maps
    /// (`{"es": "…", "en": null}`). A page exists in a locale only if every such field
    /// its path uses has a value there; templates read them with `| loc`.
    #[serde(default)]
    pub localized: Option<Vec<String>>,
}

impl Route {
    /// The declared localized fields (empty when none).
    pub fn localized_fields(&self) -> &[String] {
        self.localized.as_deref().unwrap_or_default()
    }
}

/// A route path: one pattern, or one per locale.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(
    untagged,
    expecting = "`path` must be a path string, or a map of locale to path string"
)]
pub enum PathSpec {
    One(String),
    PerLocale(BTreeMap<String, String>),
}

impl PathSpec {
    /// The unprefixed pattern for a locale; None when a per-locale map leaves it out.
    pub fn for_locale(&self, locale: Option<&str>) -> Option<&str> {
        match self {
            PathSpec::One(s) => Some(s),
            PathSpec::PerLocale(m) => m.get(locale?).map(String::as_str),
        }
    }

    /// Every pattern it holds.
    pub fn all(&self) -> Vec<&str> {
        match self {
            PathSpec::One(s) => vec![s.as_str()],
            PathSpec::PerLocale(m) => m.values().map(String::as_str).collect(),
        }
    }
}

impl std::fmt::Display for PathSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathSpec::One(s) => f.write_str(s),
            PathSpec::PerLocale(m) => {
                let parts: Vec<String> = m.iter().map(|(l, p)| format!("{l}: {p}")).collect();
                write!(f, "{{{}}}", parts.join(", "))
            }
        }
    }
}

/// A route's `locales:` value: a list of site locales, or `none`.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(try_from = "RawRouteLocales")]
pub enum RouteLocales {
    None,
    Only(Vec<String>),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawRouteLocales {
    Word(String),
    List(Vec<String>),
}

impl TryFrom<RawRouteLocales> for RouteLocales {
    type Error = String;
    fn try_from(raw: RawRouteLocales) -> std::result::Result<Self, String> {
        match raw {
            RawRouteLocales::Word(w) if w == "none" => Ok(RouteLocales::None),
            RawRouteLocales::Word(w) => Err(format!(
                "a route's `locales:` is a list of site locales or `none` — got {w:?}"
            )),
            RawRouteLocales::List(v) => Ok(RouteLocales::Only(v)),
        }
    }
}

/// A YAML string or a list of strings (`jsonld: Book` ≡ `jsonld: [Book]`).
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            OneOrMany::One(s) => vec![s.clone()],
            OneOrMany::Many(v) => v.clone(),
        }
    }
}

/// A route's `paginate:` block.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Paginate {
    /// Top-level array in data.json to split.
    pub over: String,
    pub per_page: usize,
    /// Output path of pages 2..n; must contain `{n}`, e.g. "/top/page/{n}/index.html".
    /// Per-locale map allowed under `locales:`, like the route's `path`.
    pub path: PathSpec,
}

/// A route's `index` value: a fixed YAML bool, or a minijinja pattern rendered per
/// page (e.g. `"{{ item.indexable }}"`) that must come out as exactly `true`/`false`.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum IndexRule {
    Fixed(bool),
    Pattern(String),
}

/// site.yaml — optional per-site config. A present key enables its capability
/// AND the conformance checks that ship with it (they are one unit).
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SiteConfig {
    /// Enables web-standards: sitemap.xml emission + sitemap coverage check.
    #[serde(default)]
    pub base_url: Option<String>,
    /// Enables locale formatting filters (`date_format`, `num_format`), e.g. "es-ES".
    /// Any tag; formatting uses its CLDR data (`pt-PT`, else `pt`), and a tag without
    /// data fails when a filter renders, naming the page (`i18n::locale_data`).
    #[serde(default)]
    pub locale: Option<String>,
    /// Enables output size budgets: every file a rule matches must stay under its
    /// limits (`budget.exceeded`), and every rule must match something (`budget.unmatched`).
    #[serde(default)]
    pub budgets: Option<Vec<Budget>>,
    /// Enables the locale dimension (docs/reference.md, Locales): every route renders once per
    /// locale, the first (default) at the root and the rest under `/{prefix}/`, with
    /// hreflang, `t()` and the checks that keep them correct. Excludes `locale`.
    #[serde(default)]
    pub locales: Option<Vec<String>>,
    /// Under `locales:`: prefix the default locale too (`/es/…`). Default false.
    #[serde(default)]
    pub prefix_default_locale: Option<bool>,
}

/// One site.yaml `budgets:` rule. `match` is a glob over build paths (`*` within a
/// segment, `**` across segments, `?` one character); without a leading `/` it
/// matches at any depth. At least one limit is required.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    #[serde(rename = "match")]
    pub pattern: String,
    #[serde(default)]
    pub max_bytes: Option<u64>,
    #[serde(default)]
    pub max_gzip_bytes: Option<u64>,
}

/// A loaded site: routes + snapshot + config, all data, no code.
pub struct Site {
    pub root: PathBuf,
    pub routes: Routes,
    pub snapshot: serde_json::Value,
    pub config: SiteConfig,
    /// translations/<tag>.yaml (empty unless `locales:` and the directory exist).
    pub translations: Arc<locales::Translations>,
    /// Unknown `t()` keys met while rendering, for `translations.unknown_key`.
    pub unknown_keys: locales::UnknownKeys,
    // template-engine view of the snapshot, built once — serializing the whole
    // snapshot per page dominates build time at scale
    snapshot_value: OnceLock<minijinja::Value>,
}

impl Site {
    pub fn snapshot_value(&self) -> minijinja::Value {
        self.snapshot_value
            .get_or_init(|| minijinja::Value::from_serialize(&self.snapshot))
            .clone()
    }
}

/// A site file as text. A UTF-8 byte order mark (Windows editors and some exporters
/// write one) is dropped: neither JSON nor YAML parsers accept it, and it carries no data.
pub(crate) fn read_site_file(path: &Path) -> std::io::Result<String> {
    let text = fs::read_to_string(path)?;
    Ok(match text.strip_prefix('\u{FEFF}') {
        Some(rest) => rest.to_string(),
        None => text,
    })
}

pub fn load_site(root: &Path) -> Result<Site> {
    if !root.join("routes.yaml").is_file() {
        bail!(
            "{} is not a molde site: it has no routes.yaml. --site must name the folder that \
             holds routes.yaml, data.json and templates/ (molde check --site <folder>)",
            root.display()
        );
    }
    let mut routes: Routes = serde_yaml::from_str(
        &read_site_file(&root.join("routes.yaml")).context("reading routes.yaml")?,
    )
    .context(SiteFile::new("routes.yaml"))?;
    // `about.html` means `/about.html`: older sites wrote paths without the slash, and
    // every URL (canonical, sitemap) is built as base_url + path
    for route in &mut routes.routes {
        match &mut route.path {
            PathSpec::One(p) => add_leading_slash(p),
            PathSpec::PerLocale(m) => m.values_mut().for_each(add_leading_slash),
        }
    }
    let snapshot: serde_json::Value = serde_json::from_str(
        &read_site_file(&root.join("data.json")).context("reading data.json")?,
    )
    .context(SiteFile::new("data.json"))?;
    // absent = capabilities off; present but unreadable must not silently switch
    // off the sitemap and head checks
    let config = match read_site_file(&root.join("site.yaml")) {
        Ok(s) => serde_yaml::from_str(&s).context(SiteFile::new("site.yaml"))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => SiteConfig::default(),
        Err(e) => return Err(e).context("reading site.yaml"),
    };
    if let Some(base) = &config.base_url {
        // every canonical URL, sitemap <loc> and Open Graph URL is base_url + path
        let lower = base.to_ascii_lowercase();
        let host = ["https://", "http://"]
            .iter()
            .find(|scheme| lower.starts_with(*scheme))
            .map(|scheme| base[scheme.len()..].split('/').next().unwrap_or(""));
        if !host.is_some_and(|h| !h.is_empty() && !h.contains(char::is_whitespace))
            || base.contains(['?', '#'])
        {
            bail!(
                "site.yaml base_url {base:?} must be an absolute URL: http:// or https:// and a \
                 host, with no ?query or #fragment, e.g. \"https://example.com\" (it prefixes \
                 every canonical and sitemap URL)"
            );
        }
    }
    locales::validate_config(&config)?;
    for (idx, r) in routes.routes.iter().enumerate() {
        for pattern in r.path.all() {
            for cap in fill_re().captures_iter(pattern) {
                if let Some(filter) = cap.get(2).filter(|f| f.as_str() != "slug") {
                    bail!(
                        "route {idx} ({}): path {pattern:?} uses the filter {:?}; a path \
                         placeholder takes only `slug`, e.g. {{{}|slug}}",
                        r.path,
                        filter.as_str(),
                        &cap[1]
                    );
                }
                if r.foreach.is_none() && r.paginate.is_none() {
                    bail!(
                        "route {idx} ({}): path {pattern:?} has the placeholder {} but the \
                         route has no `foreach` to fill it from — add `foreach: <collection>` \
                         or write the path out",
                        r.path,
                        &cap[0]
                    );
                }
            }
            // placeholders stand for one safe segment each (checked when filled)
            let literal = fill_re().replace_all(pattern, "x");
            if literal.contains(['{', '}']) {
                bail!(
                    "route {idx} ({}): path {pattern:?} has a malformed placeholder — write \
                     {{field}} or {{field|slug}}, one pair of braces each",
                    r.path
                );
            }
            if let Some(problem) = path_problem(&literal) {
                bail!("route {} ({}): path {pattern:?} {problem}", idx, r.path);
            }
        }
        if let Some(field) = &r.group_by {
            if r.foreach.is_none() {
                bail!(
                    "route {idx} ({}): group_by needs foreach (the collection to group)",
                    r.path
                );
            }
            if r.localized_fields().iter().any(|f| f == field) {
                bail!(
                    "route {idx} ({}): group_by can't use the localized field '{field}' yet",
                    r.path
                );
            }
        }
        locales::validate_route(&config, idx, r)?;
    }
    for (idx, r) in routes.routes.iter().enumerate() {
        let Some(types) = r.jsonld.as_ref().map(OneOrMany::to_vec) else {
            continue;
        };
        if !r.path.all().iter().all(|p| p.ends_with(".html")) {
            bail!(
                "route {} ({}): `jsonld` only applies to .html pages — remove it",
                idx,
                r.path
            );
        }
        if types.is_empty() || types.iter().any(|t| t.trim().is_empty()) {
            bail!(
                "route {} ({}): `jsonld` must name at least one non-empty @type, got {:?}",
                idx,
                r.path,
                types
            );
        }
    }
    for b in config.budgets.iter().flatten() {
        if b.max_bytes.is_none() && b.max_gzip_bytes.is_none() {
            bail!(
                "site.yaml budgets: rule '{}' sets neither max_bytes nor max_gzip_bytes",
                b.pattern
            );
        }
    }
    let translations = Arc::new(locales::load_translations(root, &config)?);
    Ok(Site {
        root: root.to_path_buf(),
        routes,
        snapshot,
        config,
        translations,
        unknown_keys: Arc::default(),
        snapshot_value: OnceLock::new(),
    })
}

/// Context on an error parsing one of the site's own files, so `check` can
/// report it as `config.parse` with the file and line.
#[derive(Debug)]
pub(crate) struct SiteFile(pub(crate) String);

impl SiteFile {
    pub(crate) fn new(file: impl Into<String>) -> Self {
        SiteFile(file.into())
    }
}

impl std::fmt::Display for SiteFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parsing {}", self.0)
    }
}

/// One concrete output page: a route pattern expanded against the snapshot.
pub struct Page {
    /// Index into `Site::routes.routes` of the route that produced this page.
    pub route: usize,
    pub path: String,
    pub template: String,
    /// The data item this page renders. Shared: every locale version (and every
    /// route) of one item points at the same copy.
    pub item: Option<Arc<serde_json::Value>>,
    /// The route's `index` rule resolved for this page (default true).
    pub index: bool,
    /// The route's `lastmod` pattern rendered for this page, trimmed; None when the
    /// route has none or it rendered empty. Validated by `sitemap.lastmod_invalid`.
    pub lastmod: Option<String>,
    /// On `paginate` routes: this page's `pagination` context (n, pages, items, …).
    /// A template value, not JSON: `links` is built once per route and shared by
    /// every page (an Arc clone), so n pages cost O(n), not O(n²).
    pub pagination: Option<minijinja::Value>,
    /// Under `locales:`: the page's locale; None for `locales: none` routes (and
    /// without the capability).
    pub locale: Option<String>,
    /// The page's locale versions, itself included, in site locale order: the same
    /// route's HTML pages for the same item (or page number) in other locales, noindex
    /// ones included (a language switcher lists them). Empty unless there are at least
    /// two. hreflang is the indexable subset, see `hreflang`.
    pub alternates: Arc<Vec<Alternate>>,
}

/// One locale version of a page.
#[derive(Clone, Debug, PartialEq)]
pub struct Alternate {
    pub locale: String,
    pub path: String,
    pub index: bool,
}

impl Page {
    /// Only `.html` paths are web pages: sitemap, std_head and the head/title
    /// checks ignore everything else (feeds, llms.txt, JSON). Such outputs are
    /// still files in the build, so links and paths.duplicate still see them.
    pub fn is_html(&self) -> bool {
        self.path.ends_with(".html")
    }

    /// Listed in sitemap.xml: an HTML page whose route allows indexing.
    pub fn in_sitemap(&self) -> bool {
        self.is_html() && self.index
    }

    /// The hreflang group std_head emits and `hreflang.mismatch` expects: only
    /// indexable versions count (a noindex page has no URL to offer), a noindex page
    /// emits none, and a group of one is no group.
    pub fn hreflang(&self) -> Vec<&Alternate> {
        if !self.in_sitemap() {
            return Vec::new();
        }
        let group: Vec<&Alternate> = self.alternates.iter().filter(|a| a.index).collect();
        match group.len() >= 2 {
            true => group,
            false => Vec::new(),
        }
    }
}

fn add_leading_slash(path: &mut String) {
    if !path.starts_with('/') {
        path.insert(0, '/');
    }
}

/// What is wrong with an output path, or None. A path names one file under the build:
/// it starts with `/`, does not end with `/`, has no empty, `.` or `..` segment, no
/// segment longer than a file name may be, and none of the characters that would break
/// or change its URL. Letters in any script are fine.
pub(crate) fn path_problem(path: &str) -> Option<String> {
    if !path.starts_with('/') {
        return Some("must start with /, e.g. \"/about.html\"".into());
    }
    if path.ends_with('/') {
        return Some(format!(
            "names a folder; a route writes a file: use {:?}",
            format!("{path}index.html")
        ));
    }
    if let Some(c) = path.chars().find(|c| {
        c.is_control() || c.is_whitespace() || is_invisible(*c) || "?#%\"'<>\\".contains(*c)
    }) {
        return Some(format!(
            "contains {c:?}, which a URL path cannot carry as is"
        ));
    }
    for seg in path[1..].split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return Some(format!("has an empty, \".\" or \"..\" segment ({seg:?})"));
        }
        if seg.len() > 255 {
            return Some(
                "has a segment longer than 255 bytes, more than a file name may be".into(),
            );
        }
    }
    None
}

/// Format characters that render as nothing or reorder text (zero-width spaces and
/// joiners, bidi controls, the BOM, the soft hyphen): in a path they make one URL look
/// like another.
fn is_invisible(c: char) -> bool {
    matches!(c,
        '\u{00AD}' | '\u{061C}' | '\u{180E}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{206F}' | '\u{FEFF}')
}

pub(crate) fn fill_re() -> &'static regex::Regex {
    static FILL_RE: OnceLock<regex::Regex> = OnceLock::new();
    // `{field}` or `{field|slug}`
    FILL_RE.get_or_init(|| regex::Regex::new(r"\{(\w+)(?:\|(\w+))?\}").unwrap())
}
