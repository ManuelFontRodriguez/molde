use crate::check::html::attr_escape;
use crate::i18n;
use crate::locales;
use crate::routes::{expand_routes, meta_ctx, render_locale};
use crate::site::{load_site, Page, Site};
use anyhow::{Context, Result};
use minijinja::{context, path_loader, Environment};
use std::path::Path;

/// Context on an error rendering one page, so `check` can name the page and route.
#[derive(Debug)]
pub(crate) struct PageFailed {
    pub(crate) page: String,
    pub(crate) route: usize,
}

impl std::fmt::Display for PageFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rendering {}", self.page)
    }
}

/// Autoescape by template name, pinned: HTML for .html/.htm/.xml, off for everything
/// else. minijinja's `json` feature (needed for `tojson`) would otherwise switch
/// .json/.js/.yaml templates to JSON escaping, so `"{{ item.name }}"` in a hand-written
/// JSON file would render `""Fish & Chips""`. JSON outputs stay plain text; use `tojson`.
fn auto_escape(name: &str) -> minijinja::AutoEscape {
    let name = [".j2", ".jinja2", ".jinja"]
        .iter()
        .find_map(|ext| name.strip_suffix(ext))
        .unwrap_or(name);
    match name.rsplit('.').next() {
        Some("html" | "htm" | "xml") => minijinja::AutoEscape::Html,
        _ => minijinja::AutoEscape::None,
    }
}

pub fn engine(site: &Site) -> Result<Environment<'static>> {
    let mut env = filters_env(site)?;
    env.set_loader(path_loader(site.root.join("templates")));
    // pre-parse the framework's own per-page templates once; re-parsing them for
    // every page dominates build time at scale. ".txt" name = no autoescape:
    // title patterns render as plain text, escaped exactly once by std_head.
    env.add_template_owned("__std_head.html".to_string(), STD_HEAD_TEMPLATE.to_string())
        .expect("std_head template is static and valid");
    for (idx, route) in site.routes.routes.iter().enumerate() {
        if let Some(pattern) = &route.title {
            env.add_template_owned(format!("__title_{idx}.txt"), pattern.clone())
                .with_context(|| format!("title pattern for route {} ({})", idx, route.path))?;
        }
        if let Some(pattern) = &route.description {
            env.add_template_owned(format!("__description_{idx}.txt"), pattern.clone())
                .with_context(|| {
                    format!("description pattern for route {} ({})", idx, route.path)
                })?;
        }
    }
    Ok(env)
}

/// The filters every template and route pattern sees.
pub(crate) fn filters_env(site: &Site) -> Result<Environment<'static>> {
    let mut env = Environment::new();
    // Engine contract (docs/reference.md "Engine contract", tests/engine_contract.rs):
    // every option that changes what a template renders is set here on purpose, even
    // where it equals minijinja's default, so an upgrade or a new feature flag can't
    // move it silently. The one left at its default is `debug` (on in debug builds):
    // it only enriches error messages, never what renders.
    //
    // Lenient: an undefined name or missing key renders empty and is falsy, so optional
    // data fields need no guards; the checks catch the empty titles/descriptions that
    // follow. An attribute OF an undefined value (`{{ nope.x }}`) is still an error.
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
    // A template's final newline is dropped: outputs end where the markup ends, and
    // route patterns (title, description, index, lastmod) never carry a stray "\n".
    env.set_keep_trailing_newline(false);
    // No implicit whitespace control: block tags keep the newline after them and the
    // indentation before them. Use `{%- -%}` where it matters.
    env.set_trim_blocks(false);
    env.set_lstrip_blocks(false);
    env.set_auto_escape_callback(auto_escape);
    // HTML autoescape with attr_escape's five characters. minijinja's default also
    // escapes '/', so every data-built href rendered as href="&#x2f;books&#x2f;dune&#x2f;":
    // valid, but unreadable to the agents and humans who grep the output. '/' needs no
    // escape in text or quoted attributes; safe values and other modes are untouched.
    //
    // JSON null prints as nothing, in every output type, as in Liquid, Nunjucks and
    // Twig: a nullable data field must never put the word "None" on a page. It is
    // still a value (`is none`, `default` keeps it); only printing it is empty.
    //
    // A declared localized field refuses to print: `{{ item.name }}` would put an
    // object's text form in a title and pass every duplicate check.
    env.set_formatter(|out, state, value| {
        if let Some(l) = value.downcast_object_ref::<locales::Localized>() {
            return Err(locales::print_error(l));
        }
        if value.is_none() {
            return Ok(());
        }
        let plain = !value.is_safe() && !value.is_undefined();
        if state.auto_escape() == minijinja::AutoEscape::Html && plain {
            let text = match value.as_str() {
                Some(s) => attr_escape(s),
                None => attr_escape(&value.to_string()),
            };
            return out.write_str(&text).map_err(minijinja::Error::from);
        }
        minijinja::escape_formatter(out, state, value)
    });
    // `escape` / `e` use the same five characters as autoescape, so an explicit escape
    // never writes '/' as `&#x2f;` either. Already-safe values pass through unchanged
    // (`| e | e` escapes once); undefined and none give "", as they print.
    let escape = |v: minijinja::Value| {
        if v.is_safe() {
            return v;
        }
        let text = match v.as_str() {
            _ if v.is_undefined() || v.is_none() => String::new(),
            Some(s) => attr_escape(s),
            None => attr_escape(&v.to_string()),
        };
        minijinja::Value::from_safe_string(text)
    };
    env.add_filter("escape", escape);
    env.add_filter("e", escape);
    // `year` needs no locale: it extracts, it doesn't format.
    env.add_filter(
        "year",
        |state: &minijinja::State, v: minijinja::Value| -> Result<String, minijinja::Error> {
            Ok(i18n::year(locales::text_arg(state, "year", &v)?))
        },
    );
    // the same function as `{field|slug}` in a route path, so a link built with
    // `| slug` always names the page the route made
    // accepts what a path placeholder accepts (a string or an integer) and refuses the
    // rest, so a null or missing value can't quietly link to `/genres/none/`
    env.add_filter(
        "slug",
        |v: minijinja::Value| -> Result<String, minijinja::Error> {
            if let Some(s) = v.as_str() {
                return Ok(slug(s));
            }
            if v.as_i64().is_some() && v.kind() == minijinja::value::ValueKind::Number {
                return Ok(slug(&v.to_string()));
            }
            Err(minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                format!(
                    "slug needs a string or an integer, got {} ({})",
                    if v.is_undefined() {
                        "undefined".into()
                    } else {
                        v.to_string()
                    },
                    v.kind()
                ),
            ))
        },
    );
    if site.config.locales.is_some() {
        locales::register(
            &mut env,
            &site.config,
            site.translations.clone(),
            site.unknown_keys.clone(),
        );
    } else if let Some(locale) = &site.config.locale {
        let tag = locale.clone();
        let t = tag.clone();
        env.add_filter(
            "date_format",
            move |state: &minijinja::State,
                  v: minijinja::Value,
                  kwargs: minijinja::value::Kwargs| {
                locales::date_format_filter(state, v, kwargs, Some(&t))
            },
        );
        env.add_filter(
            "num_format",
            move |state: &minijinja::State,
                  v: minijinja::Value,
                  kwargs: minijinja::value::Kwargs| {
                locales::num_format_filter(state, v, kwargs, Some(&tag))
            },
        );
    }
    Ok(env)
}

pub fn render_page(env: &Environment, site: &Site, page: &Page) -> Result<String> {
    let tmpl = env
        .get_template(&page.template)
        .with_context(|| format!("template {}", page.template))?;
    // pre-escaped by its own render pass; UNDEFINED renders as empty when the
    // web-standards capability is off
    let head = match std_head(env, site, page)? {
        Some(block) => minijinja::Value::from_safe_string(block),
        None => minijinja::Value::UNDEFINED,
    };
    let base =
        context!(data => site.snapshot_value(), std_head => head, page => page_context(site, page));
    // `locale`, `locales`, `alternates` exist only under site.yaml `locales:`
    let base = match render_locale(site, page.locale.as_deref()) {
        Some(l) => context!(
            locale => l,
            locales => site.config.locales,
            alternates => locales::alternates_value(site, page),
            ..base
        ),
        None => base,
    };
    // `item` / `pagination` exist only on the routes that have them (`is defined`)
    let fields = site.routes.routes[page.route].localized_fields();
    let ctx = match (&page.item, &page.pagination) {
        (Some(item), _) => context!(item => locales::item_value(fields, item), ..base),
        (None, Some(p)) => context!(pagination => p, ..base),
        (None, None) => base,
    };
    let html = tmpl.render(ctx)?;
    if !fields.is_empty() {
        locales::check_printed(&html).with_context(|| format!("template {}", page.template))?;
    }
    Ok(html)
}

/// `page` in the template context: where this page lives, so breadcrumbs, JSON-LD
/// `url`/`@id` and "you are here" nav don't have to rebuild the route in data.json.
/// `path` is the public path (`/books/dune/`, `/llms.txt`); `url` is `page_url` of it,
/// the same string canonical and the sitemap use, and exists only with a `base_url`.
fn page_context(site: &Site, page: &Page) -> minijinja::Value {
    let path = public_path(&page.path);
    match &site.config.base_url {
        Some(base) => context!(path => path, url => page_url(base, &page.path)),
        None => context!(path => path),
    }
}

/// Output path -> public path: "/books/dune/index.html" -> "/books/dune/", "/llms.txt" as is.
pub(crate) fn public_path(path: &str) -> &str {
    pretty_path(path)
}

/// `/a/index.html` -> `/a/`, `/index.html` -> `/`; any other path is unchanged. Only a
/// whole `index.html` file name is dropped: `/reindex.html` stays `/reindex.html`.
fn pretty_path(path: &str) -> &str {
    match path.strip_suffix("index.html") {
        Some(dir) if dir.is_empty() || dir.ends_with('/') => dir,
        _ => path,
    }
}

/// "/books/dune/" | "books/dune" | "/" -> "/books/dune/index.html" | "/index.html"
pub(crate) fn normalize_url_path(path: &str) -> String {
    let path = path.split(['?', '#']).next().unwrap_or("");
    let mut s = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    if s.ends_with('/') {
        s.push_str("index.html");
    } else if !s.rsplit('/').next().unwrap_or("").contains('.') {
        s.push_str("/index.html");
    }
    s
}

/// Render one page from live source, no build — the CLI twin of the MCP
/// render_page tool: grep the output instead of building and spelunking out dirs.
pub fn render_url(site_dir: &Path, url_path: &str) -> Result<String> {
    let site = load_site(site_dir)?;
    let normalized = normalize_url_path(url_path);
    let page = expand_routes(&site)?
        .into_iter()
        .find(|p| p.path == normalized)
        .with_context(|| {
            format!("no route renders '{normalized}' — see routes.yaml or the list_routes MCP tool")
        })?;
    let env = engine(&site)?;
    render_page(&env, &site, &page)
}

/// The ONE place a page path becomes a public URL — sitemap, canonical, OG and all
/// their checks derive from here so they cannot drift.
pub fn page_url(base_url: &str, path: &str) -> String {
    format!("{}{}", base_url.trim_end_matches('/'), pretty_path(path))
}

/// OG is all-or-nothing: a fragmentary block is worse than none. A noindex page
/// has no canonical URL to claim, so it gets the robots meta and no OG at all.
const STD_HEAD_TEMPLATE: &str = "{% if title %}<title>{{ title }}</title>\n{% endif %}\
{% if description %}<meta name=\"description\" content=\"{{ description }}\">\n{% endif %}\
{% if index %}<link rel=\"canonical\" href=\"{{ url }}\">\
{% if title %}\n<meta property=\"og:title\" content=\"{{ title }}\">\n\
{% if description %}<meta property=\"og:description\" content=\"{{ description }}\">\n{% endif %}\
<meta property=\"og:url\" content=\"{{ url }}\">\n\
<meta property=\"og:type\" content=\"website\">{% endif %}\
{% for a in alternates %}\n<link rel=\"alternate\" hreflang=\"{{ a.hreflang }}\" href=\"{{ a.url }}\">{% endfor %}\
{% else %}<meta name=\"robots\" content=\"noindex\">{% endif %}";

/// Per-page head block — a pure function of (config, route, path, item).
/// None when web-standards is off (no base_url in site.yaml) and for non-HTML
/// outputs, which have no <head>.
pub(crate) fn std_head(env: &Environment, site: &Site, page: &Page) -> Result<Option<String>> {
    let Some(base) = &site.config.base_url else {
        return Ok(None);
    };
    if !page.is_html() {
        return Ok(None);
    }
    let url = page_url(base, &page.path);
    // The title pattern renders as PLAIN text (its ".txt" template has no autoescape);
    // the std_head template below escapes it exactly once, in both element and
    // attribute contexts. Only the finished block is marked safe.
    let title = match &site.routes.routes[page.route].title {
        Some(_) => Some(
            env.get_template(&format!("__title_{}.txt", page.route))
                .and_then(|t| {
                    t.render(meta_ctx(
                        site,
                        page.route,
                        page.locale.as_deref(),
                        page.item.as_deref(),
                        page.pagination.as_ref(),
                    ))
                })
                .map_err(anyhow::Error::from)
                .and_then(|t| locales::check_printed(&t).map(|_| t))
                .with_context(|| format!("title pattern for route {}", page.route))?,
        ),
        None => None,
    };
    // same plain-text path as the title; trimmed, since multi-line patterns with
    // {% if %} blocks leave whitespace a description must not carry
    let description = match &site.routes.routes[page.route].description {
        Some(_) => Some(
            env.get_template(&format!("__description_{}.txt", page.route))
                .and_then(|t| {
                    t.render(meta_ctx(
                        site,
                        page.route,
                        page.locale.as_deref(),
                        page.item.as_deref(),
                        page.pagination.as_ref(),
                    ))
                })
                .map_err(anyhow::Error::from)
                .and_then(|t| locales::check_printed(&t).map(|_| t))
                .with_context(|| format!("description pattern for route {}", page.route))?
                .trim()
                .to_string(),
        ),
        None => None,
    };
    // The URL gets attr_escape and is marked safe: the same escape the environment's
    // formatter applies, done here once so the canonical is exactly page_url's string.
    let safe_url = |u: &str| minijinja::Value::from_safe_string(attr_escape(u));
    // hreflang: every group member (itself included), plus x-default = the default
    // locale's member when it has one
    let group = page.hreflang();
    let mut alternates: Vec<minijinja::Value> = group
        .iter()
        .map(|a| context!(hreflang => a.locale, url => safe_url(&page_url(base, &a.path))))
        .collect();
    if let Some(d) = group
        .iter()
        .find(|a| Some(a.locale.as_str()) == site.config.default_locale())
    {
        alternates
            .push(context!(hreflang => "x-default", url => safe_url(&page_url(base, &d.path))));
    }
    let block = env.get_template("__std_head.html")?.render(context!(
        title => title,
        description => description,
        index => page.index,
        url => safe_url(&url),
        alternates => alternates,
    ))?;
    Ok(Some(block))
}

/// A value as one URL path segment: lowercase ASCII letters and digits, words joined by
/// `-`. Accents are dropped (`Énergie` gives `energie`), a few letters are spelled out
/// (`ß` gives `ss`), and every other character separates words; letters with no ASCII
/// spelling (Cyrillic, CJK) are left out, so a value made only of them gives `""`.
pub(crate) fn slug(value: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let mut out = String::new();
    let mut gap = false;
    for c in value.nfkd() {
        let spelled = match c {
            'ß' | 'ẞ' => "ss",
            'æ' | 'Æ' => "ae",
            'œ' | 'Œ' => "oe",
            'ø' | 'Ø' => "o",
            'đ' | 'Đ' | 'ð' | 'Ð' => "d",
            'ł' | 'Ł' => "l",
            'þ' | 'Þ' => "th",
            'ı' => "i",
            _ if c.is_ascii_alphanumeric() => "",
            // combining accents left by the decomposition: part of the letter before
            _ if unicode_normalization::char::is_combining_mark(c) => continue,
            _ => {
                gap = true;
                continue;
            }
        };
        if gap && !out.is_empty() {
            out.push('-');
        }
        gap = false;
        if spelled.is_empty() {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push_str(spelled);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn slugs_are_lowercase_ascii_words_joined_by_hyphens() {
        for (value, want) in [
            ("Science Fiction", "science-fiction"),
            ("  True   Crime  ", "true-crime"),
            ("Fish & Chips", "fish-chips"),
            ("Rock & Roll 101", "rock-roll-101"),
            ("Poésie & Théâtre", "poesie-theatre"),
            ("Straße", "strasse"),
            ("Ærø Łódź", "aero-lodz"),
            ("ﬁnance", "finance"),
            ("already-a-slug", "already-a-slug"),
            ("snake_case", "snake-case"),
            ("Поэзия", ""),
            ("Tech Поэзия", "tech"),
            ("---", ""),
            ("STRAẞE", "strasse"),
            ("İstanbul", "istanbul"),
            ("ＡＢＣ１２３", "abc123"),
            ("a😀b", "a-b"),
            ("٣ books", "books"),
            ("1.5", "1-5"),
            ("e\u{301}te\u{301}", "ete"),
            ("zero\u{200b}width", "zero-width"),
        ] {
            assert_eq!(slug(value), want, "{value:?}");
        }
        // whatever goes in, what comes out is a safe ASCII segment
        for value in [
            "Ǆ ŉ Ⅻ",
            "\u{0}\t\n../..",
            "日本語 & Ελληνικά 2026",
            "-a-",
            "ﬀ ﬃ ǈ",
        ] {
            let s = slug(value);
            assert!(
                s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                    && !s.starts_with('-')
                    && !s.ends_with('-')
                    && !s.contains("--"),
                "{value:?} -> {s:?}"
            );
        }
    }
}
