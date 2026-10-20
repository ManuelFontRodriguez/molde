//! The `locales` capability (docs/reference.md, Locales): one site in several languages.
//!
//! Opt-in or nonexistent: without `locales:` in site.yaml nothing here runs, and a
//! site builds byte-identical to a molde without this module. With it, every route
//! expands once per locale; the default locale lives at the root and the others under
//! `/{prefix}/`. Paths are data — a route declares which item fields are localized,
//! and a missing locale value means "no page in that locale", never a fallback.
//! hreflang is a query over the expanded manifest, and its check is a set comparison.

use crate::i18n;
use crate::render::public_path;
use crate::site::{Page, Route, Site, SiteConfig};
use anyhow::{bail, Context, Result};
use minijinja::value::{Kwargs, Object, ObjectRepr, Value};
use minijinja::{Error, ErrorKind, State};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

/// `t()` calls with a key the default locale's file doesn't define, collected during
/// a render: (template, key, locale). Reported as `translations.unknown_key` instead
/// of aborting the render, so every other layer still runs.
pub type UnknownKeys = Arc<Mutex<BTreeSet<(String, String, String)>>>;

/// `pt-BR` -> `pt-br`: a tag's URL prefix.
pub fn prefix(tag: &str) -> String {
    tag.to_ascii_lowercase()
}

/// A language (`es`) or language-REGION (`pt-BR`). Syntax only: molde has no BCP 47
/// registry, and any well-formed tag is a valid site locale. Whether `date_format` /
/// `num_format` can format in it is decided at render time (`i18n::locale_data`).
pub fn valid_tag(tag: &str) -> bool {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^[a-z]{2,3}(-[A-Z]{2})?$").unwrap())
        .is_match(tag)
}

impl SiteConfig {
    /// The configured locales, first = default. None = the capability is off.
    pub fn locale_tags(&self) -> Option<&[String]> {
        self.locales.as_deref()
    }

    pub fn default_locale(&self) -> Option<&str> {
        self.locales
            .as_ref()
            .and_then(|l| l.first())
            .map(String::as_str)
    }
}

/// site.yaml checks for the capability — load errors, the build doesn't start.
pub fn validate_config(config: &SiteConfig) -> Result<()> {
    let Some(tags) = &config.locales else {
        if config.prefix_default_locale.is_some() {
            bail!("site.yaml: `prefix_default_locale` needs `locales:` — remove it or add locales");
        }
        return Ok(());
    };
    if config.locale.is_some() {
        bail!(
            "site.yaml: `locale` and `locales` can't both be set — `locale` is for \
             single-language sites; with `locales:` each page formats in its own locale"
        );
    }
    if tags.is_empty() {
        bail!("site.yaml: `locales` must list at least one locale (the first is the default)");
    }
    let mut prefixes = BTreeSet::new();
    for tag in tags {
        if !valid_tag(tag) {
            bail!(
                "site.yaml locales: {tag:?} is not a language (`es`) or language-REGION \
                 (`pt-BR`) tag"
            );
        }
        if !prefixes.insert(prefix(tag)) {
            bail!("site.yaml locales: {tag:?} is listed twice");
        }
    }
    Ok(())
}

/// routes.yaml checks for the capability, per route — load errors.
pub fn validate_route(config: &SiteConfig, idx: usize, route: &Route) -> Result<()> {
    let at = || format!("route {} ({})", idx, route.path);
    let per_locale_paginate = route
        .paginate
        .as_ref()
        .is_some_and(|p| matches!(p.path, crate::site::PathSpec::PerLocale(_)));
    let Some(tags) = config.locale_tags() else {
        if matches!(route.path, crate::site::PathSpec::PerLocale(_))
            || per_locale_paginate
            || route.locales.is_some()
            || route.localized.is_some()
        {
            bail!(
                "{}: per-locale paths, `locales:` and `localized:` need `locales:` in site.yaml",
                at()
            );
        }
        return Ok(());
    };
    let known = |t: &String| tags.contains(t);
    let unknown = |what: &str, t: &str| {
        anyhow::anyhow!(
            "{}: {what} names locale {t:?}, which isn't in site.yaml locales {:?} \
             (locales.unknown)",
            at(),
            tags
        )
    };
    for p in route.path.all() {
        if !p.starts_with('/') {
            bail!(
                "{}: under locales every path must start with / — got {p:?}",
                at()
            );
        }
    }
    if let crate::site::PathSpec::PerLocale(map) = &route.path {
        if map.is_empty() {
            bail!(
                "{}: a per-locale `path` map needs at least one locale",
                at()
            );
        }
        if let Some(t) = map.keys().find(|t| !known(t)) {
            return Err(unknown("`path`", t));
        }
    }
    match &route.locales {
        Some(crate::site::RouteLocales::None) => {
            if matches!(route.path, crate::site::PathSpec::PerLocale(_)) || per_locale_paginate {
                bail!(
                    "{}: a `locales: none` route has one fixed path — use a plain string",
                    at()
                );
            }
            if route.localized.is_some() {
                bail!(
                    "{}: a `locales: none` route has no locale, so no localized fields",
                    at()
                );
            }
        }
        Some(crate::site::RouteLocales::Only(list)) => {
            if list.is_empty() {
                bail!(
                    "{}: `locales:` lists no locale — use `none` for a root-only route",
                    at()
                );
            }
            if let Some(t) = list.iter().find(|t| !known(t)) {
                return Err(unknown("`locales:`", t));
            }
        }
        None => {}
    }
    if let Some(fields) = &route.localized {
        if route.foreach.is_none() && route.paginate.is_none() {
            bail!(
                "{}: `localized:` names item fields, so it needs `foreach` or `paginate`",
                at()
            );
        }
        if fields.is_empty() || fields.iter().any(|f| f.trim().is_empty()) {
            bail!("{}: `localized:` must list at least one field name", at());
        }
    }
    if let Some(pg) = &route.paginate {
        if let crate::site::PathSpec::PerLocale(map) = &pg.path {
            if let Some(t) = map.keys().find(|t| !known(t)) {
                return Err(unknown("`paginate.path`", t));
            }
            for tag in route_tags(config, route) {
                if route.path.for_locale(Some(&tag)).is_some() && !map.contains_key(&tag) {
                    bail!(
                        "{}: the route has a {tag} page 1 but `paginate.path` has no {tag} entry",
                        at()
                    );
                }
            }
        }
    }
    Ok(())
}

/// The locales a route renders in, in site order. Empty for `locales: none` (and when
/// the capability is off): such a route renders once, outside the locale dimension.
pub fn route_tags(config: &SiteConfig, route: &Route) -> Vec<String> {
    let Some(tags) = config.locale_tags() else {
        return Vec::new();
    };
    match &route.locales {
        Some(crate::site::RouteLocales::None) => Vec::new(),
        Some(crate::site::RouteLocales::Only(list)) => {
            tags.iter().filter(|t| list.contains(t)).cloned().collect()
        }
        None => tags.to_vec(),
    }
}

/// The one path rule: default locale at the root (unless `prefix_default_locale`),
/// every other locale under `/{prefix}`. `None` (outside the locale dimension) is the
/// path verbatim.
pub fn localize_path(config: &SiteConfig, locale: Option<&str>, unprefixed: &str) -> String {
    match locale {
        None => unprefixed.to_string(),
        Some(l)
            if Some(l) == config.default_locale()
                && !config.prefix_default_locale.unwrap_or(false) =>
        {
            unprefixed.to_string()
        }
        Some(l) => format!("/{}{}", prefix(l), unprefixed),
    }
}

/// A declared localized field must be present and `{locale: string | null}` over site
/// locales. An item without the field would otherwise drop out of every locale silently.
pub fn validate_localized_item(
    config: &SiteConfig,
    fields: &[String],
    what: &str,
    item: &serde_json::Value,
) -> Result<()> {
    let tags = config.locale_tags().unwrap_or_default();
    for field in fields {
        let Some(v) = item.get(field) else {
            bail!(
                "{what}: has no localized field '{field}' — every item needs it (use null \
                 for a locale without a value)"
            );
        };
        let Some(map) = v.as_object() else {
            bail!(
                "{what}: localized field '{field}' must be an object keyed by locale, \
                 e.g. {{\"es\": \"…\", \"en\": \"…\"}} — got {v}"
            );
        };
        for (k, val) in map {
            if !tags.contains(k) {
                bail!(
                    "{what}: localized field '{field}' has key {k:?}, which isn't in site.yaml \
                     locales {tags:?}"
                );
            }
            if !(val.is_string() || val.is_null()) {
                bail!("{what}: localized field '{field}'.{k} must be a string or null — got {val}");
            }
        }
    }
    Ok(())
}

/// The one existence rule, for `foreach` pages and paginated lists alike: an item
/// exists in `locale` when every declared field has a value there. So a detail page
/// is always listed, and `| loc` on a declared field never fails on a built page.
pub fn item_exists_in(fields: &[String], locale: &str, item: &serde_json::Value) -> bool {
    fields.iter().all(|f| {
        item.get(f)
            .and_then(|v| v.get(locale))
            .is_some_and(|x| !x.is_null())
    })
}

/// A declared localized field as templates see it: indexable (`[locale]`, `.en`) and
/// it refuses to print, because a title printing an object's text form would pass
/// every duplicate check. `| loc` picks the page's locale. A bare `{{ item.name }}`
/// fails in the formatter (lib.rs); every other way to text (filters, `~`, `tojson`,
/// printing the whole item) writes a sentinel that `check_printed` turns into an error.
#[derive(Debug)]
pub struct Localized {
    pub field: String,
    pub values: BTreeMap<String, Option<String>>,
}

impl Object for Localized {
    // Plain, not Map: a map would serialize (`tojson`) and iterate as its values
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Plain
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let v = self.values.get(key.as_str()?)?;
        Some(match v {
            Some(s) => Value::from(s.clone()),
            None => Value::from(()),
        })
    }

    fn render(self: &Arc<Self>, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name: String = self.field.chars().map(encode).collect();
        write!(f, "{SENTINEL}{name}{SENTINEL_END}")
    }
}

/// Private-use characters: case mapping, `replace` and JSON escaping leave them alone,
/// so the field name is carried in them too (`| upper` must not rename it).
const SENTINEL: char = '\u{E000}';
const SENTINEL_END: char = '\u{E001}';
const PLANE: u32 = 0xF0000;

fn encode(c: char) -> char {
    char::from_u32(PLANE + c as u32)
        .filter(|_| (c as u32) < 0xFFFE)
        .unwrap_or('?')
}

fn decode(c: char) -> char {
    char::from_u32((c as u32).wrapping_sub(PLANE)).unwrap_or('?')
}

/// A render that printed a localized value through any path but the formatter.
pub fn check_printed(out: &str) -> Result<()> {
    let Some(start) = out.find(SENTINEL) else {
        return Ok(());
    };
    let rest = &out[start + SENTINEL.len_utf8()..];
    let field: String = rest
        .split(SENTINEL_END)
        .next()
        .unwrap_or_default()
        .chars()
        .map(decode)
        .collect();
    bail!(
        "item.{field} is a localized value and was printed as text (a filter, `~`, `tojson` \
         or the whole item) — write `item.{field} | loc` (the page's locale) or \
         `item.{field}[locale]`"
    )
}

/// The render error for printing a localized value.
pub fn print_error(l: &Localized) -> Error {
    Error::new(
        ErrorKind::InvalidOperation,
        format!(
            "item.{0} is a localized value and can't be printed as is — write \
             `item.{0} | loc` (the page's locale) or `item.{0}[locale]`",
            l.field
        ),
    )
}

/// An item as templates and route patterns see it: declared localized fields become
/// `Localized` values, everything else is the plain data.
pub fn item_value(fields: &[String], item: &serde_json::Value) -> Value {
    let Some(map) = item.as_object().filter(|_| !fields.is_empty()) else {
        return Value::from_serialize(item);
    };
    let out: BTreeMap<String, Value> = map
        .iter()
        .map(|(k, v)| {
            let value = match v.as_object() {
                Some(obj) if fields.contains(k) => Value::from_object(Localized {
                    field: k.clone(),
                    values: obj
                        .iter()
                        .map(|(l, s)| (l.clone(), s.as_str().map(String::from)))
                        .collect(),
                }),
                _ => Value::from_serialize(v),
            };
            (k.clone(), value)
        })
        .collect();
    Value::from(out)
}

/// `translations/<tag>.yaml`, flattened to dotted keys.
#[derive(Default)]
pub struct Translations {
    /// false = no translations/ directory: `t()` is an error at first use.
    pub present: bool,
    pub by_locale: BTreeMap<String, BTreeMap<String, String>>,
}

pub fn load_translations(root: &Path, config: &SiteConfig) -> Result<Translations> {
    let Some(tags) = config.locale_tags() else {
        return Ok(Translations::default());
    };
    let dir = root.join("translations");
    if !dir.is_dir() {
        return Ok(Translations::default());
    }
    for entry in fs::read_dir(&dir)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if let Some(tag) = name.strip_suffix(".yaml") {
            if !tags.iter().any(|t| t == tag) {
                bail!(
                    "translations/{name}: '{tag}' isn't in site.yaml locales {tags:?} — \
                     rename or remove the file"
                );
            }
        }
    }
    let mut by_locale = BTreeMap::new();
    for tag in tags {
        let file = dir.join(format!("{tag}.yaml"));
        let mut flat = BTreeMap::new();
        if file.is_file() {
            let doc: serde_yaml::Value = serde_yaml::from_str(
                &crate::site::read_site_file(&file)
                    .with_context(|| format!("reading {}", file.display()))?,
            )
            .with_context(|| crate::site::SiteFile::new(format!("translations/{tag}.yaml")))?;
            if !doc.is_null() {
                flatten(&format!("translations/{tag}.yaml"), "", &doc, &mut flat)?;
            }
        } else if Some(tag.as_str()) == config.default_locale() {
            bail!(
                "translations/{tag}.yaml is missing — the default locale's file defines the \
                 key set every other locale must match"
            );
        }
        by_locale.insert(tag.clone(), flat);
    }
    let default = config.default_locale().unwrap_or_default();
    if let Some((key, _)) = by_locale[default].iter().find(|(_, v)| v.trim().is_empty()) {
        bail!("translations/{default}.yaml: key '{key}' is empty — the default locale defines every string");
    }
    Ok(Translations {
        present: true,
        by_locale,
    })
}

fn flatten(
    file: &str,
    at: &str,
    v: &serde_yaml::Value,
    out: &mut BTreeMap<String, String>,
) -> Result<()> {
    let Some(map) = v.as_mapping() else {
        bail!("{file}: the top level must be a mapping of keys to strings");
    };
    for (k, v) in map {
        let Some(k) = k.as_str() else {
            bail!("{file}: key {k:?} under '{at}' must be a string");
        };
        let key = if at.is_empty() {
            k.to_string()
        } else {
            format!("{at}.{k}")
        };
        match v {
            serde_yaml::Value::String(s) => {
                if out.insert(key.clone(), s.clone()).is_some() {
                    bail!(
                        "{file}: key '{key}' is defined twice (a flat dotted key and a nested one)"
                    );
                }
            }
            serde_yaml::Value::Mapping(_) => flatten(file, &key, v, out)?,
            other => bail!("{file}: '{key}' must be a string (quote it) — got {other:?}"),
        }
    }
    Ok(())
}

fn placeholder_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"\{(\w+)\}").unwrap())
}

fn placeholders(s: &str) -> BTreeSet<String> {
    placeholder_re()
        .captures_iter(s)
        .map(|c| c[1].to_string())
        .collect()
}

/// The page's locale from the render state (every page has one under locales;
/// `locales: none` pages get the default).
fn state_locale(state: &State) -> Result<String, Error> {
    state
        .lookup("locale")
        .and_then(|v| v.as_str().map(String::from))
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidOperation,
                "no `locale` in this context — `t()`, `| loc` and the locale filters need a page",
            )
        })
}

/// A render error that names the page being rendered, when there is one; minijinja adds
/// the template and line.
fn page_error(state: &State, msg: String) -> Error {
    let page = state
        .lookup("page")
        .and_then(|p| p.get_attr("path").ok())
        .and_then(|p| p.as_str().map(String::from));
    let msg = match page {
        Some(path) => format!("{msg} (page {path})"),
        None => msg,
    };
    Error::new(ErrorKind::InvalidOperation, msg)
}

/// The string a date filter works on, or a render error naming the filter, the value and
/// the page (a number or a missing field would otherwise surface as "value is not a
/// string").
pub(crate) fn text_arg<'a>(state: &State, filter: &str, v: &'a Value) -> Result<&'a str, Error> {
    v.as_str().ok_or_else(|| {
        page_error(
            state,
            format!(
                "{filter}: needs a date string like \"2026-09-23\" — got {}",
                describe(v)
            ),
        )
    })
}

/// A value as an error message names it: its kind and text, or "a missing field".
fn describe(v: &Value) -> String {
    match v.is_undefined() {
        true => "a missing field".to_string(),
        false => format!("{} {v}", v.kind()),
    }
}

/// The locale a formatting call uses: an explicit `locale=`, else `fixed` (the
/// single-language `locale:`), else the page's locale. Its CLDR data, or a render error
/// naming the filter, the locale and the page.
fn format_locale(
    filter: &str,
    state: &State,
    kwargs: &Kwargs,
    fixed: Option<&str>,
) -> Result<&'static i18n::CldrLocale, Error> {
    let tag = match (kwargs.get::<Option<&str>>("locale")?, fixed) {
        (Some(l), _) | (None, Some(l)) => l.to_string(),
        (None, None) => state_locale(state)?,
    };
    i18n::locale_data(filter, &tag).map_err(|msg| page_error(state, msg))
}

/// `date_format(style=…, pattern=…, locale=…)`, shared by `locale:` and `locales:`.
pub(crate) fn date_format_filter(
    state: &State,
    v: Value,
    kwargs: Kwargs,
    fixed: Option<&str>,
) -> Result<String, Error> {
    let v = text_arg(state, "date_format", &v)?;
    let loc = format_locale("date_format", state, &kwargs, fixed)?;
    let style = kwargs.get::<Option<&str>>("style")?;
    let pattern = kwargs.get::<Option<&str>>("pattern")?;
    kwargs.assert_all_used()?;
    let fail = |msg: String| page_error(state, format!("date_format: {msg}"));
    match (style, pattern) {
        (Some(_), Some(_)) => Err(fail("pass `style` or `pattern`, not both".into())),
        (None, Some(p)) => i18n::date_pattern(loc, v, p).map_err(fail),
        (style, None) => {
            let style = style.unwrap_or("long");
            if !i18n::STYLES.contains(&style) {
                return Err(fail(format!(
                    "style '{style}' unknown — styles: {}",
                    i18n::STYLES.join(", ")
                )));
            }
            Ok(i18n::date_format(loc, v, style))
        }
    }
}

/// `num_format(decimals=…, grouping=…, locale=…)`, shared by `locale:` and `locales:`.
/// Takes a decimal string or a number (a number as the template would print it).
pub(crate) fn num_format_filter(
    state: &State,
    v: Value,
    kwargs: Kwargs,
    fixed: Option<&str>,
) -> Result<String, Error> {
    let loc = format_locale("num_format", state, &kwargs, fixed)?;
    let decimals = kwargs.get::<Option<usize>>("decimals")?;
    let grouping = kwargs.get::<Option<bool>>("grouping")?;
    kwargs.assert_all_used()?;
    if decimals.is_some_and(|n| n > i18n::MAX_DECIMALS) {
        return Err(page_error(
            state,
            format!("num_format: decimals must be 0-{}", i18n::MAX_DECIMALS),
        ));
    }
    let text = match (v.as_str(), v.kind()) {
        (Some(s), _) => s.to_string(),
        (None, minijinja::value::ValueKind::Number) => v.to_string(),
        (None, _) => {
            return Err(page_error(
                state,
                format!(
                    "num_format: needs a decimal string or a number — got {}",
                    describe(&v)
                ),
            ))
        }
    };
    let opts = i18n::NumOptions { decimals, grouping };
    Ok(i18n::num_format(loc, &text, opts))
}

/// Registers `t`, `t_html`, `loc` and the per-page-locale `date_format` / `num_format`.
pub fn register(
    env: &mut minijinja::Environment<'static>,
    config: &SiteConfig,
    translations: Arc<Translations>,
    unknown: UnknownKeys,
) {
    let tags: Vec<String> = config.locale_tags().unwrap_or_default().to_vec();
    let default = config.default_locale().unwrap_or_default().to_string();

    // `html`: t_html, whose result is marked safe — its arguments are escaped, so only
    // the translation's own markup is unescaped
    let translate = move |state: &State,
                          key: &str,
                          kwargs: Kwargs,
                          html: bool|
          -> Result<String, Error> {
        let locale = state_locale(state)?;
        if !translations.present {
            return Err(Error::new(
                ErrorKind::InvalidOperation,
                format!("t('{key}'): there is no translations/ directory — add translations/{default}.yaml"),
            ));
        }
        let Some(contract) = translations.by_locale[&default].get(key) else {
            unknown
                .lock()
                .unwrap()
                .insert((state.name().to_string(), key.to_string(), locale));
            return Ok(String::new());
        };
        // `args=`: a context bag (text pools pick one of several variants that name
        // different subsets of it), so its unused entries are fine; explicit kwargs stay
        // strict and win over the bag on a name clash
        let bag: Option<Value> = kwargs.get("args")?;
        if let Some(b) = &bag {
            if b.kind() != minijinja::value::ValueKind::Map {
                return Err(Error::new(
                    ErrorKind::InvalidOperation,
                    format!("t('{key}'): `args=` needs a mapping — got {} {b}", b.kind()),
                ));
            }
        }
        // the default locale's placeholders are the call's contract, checked on every
        // page: a translation that names others keeps them literal and
        // `translations.args` reports it
        let mut args = BTreeMap::new();
        for name in placeholders(contract) {
            let explicit = name != "args" && kwargs.has(&name);
            let v: Option<Value> = match (&bag, explicit) {
                (Some(b), false) => {
                    Some(b.get_attr(&name)?).filter(|v| !v.is_undefined() && !v.is_none())
                }
                _ => kwargs.get(&name)?,
            };
            let Some(v) = v else {
                return Err(Error::new(
                    ErrorKind::MissingArgument,
                    format!("t('{key}') needs `{name}=…` (or an `args=` entry) for its {{{name}}} placeholder"),
                ));
            };
            if let Some(l) = v.downcast_object_ref::<Localized>() {
                return Err(print_error(l));
            }
            let text = match html && !v.is_safe() {
                true => crate::check::html::attr_escape(&v.to_string()),
                false => v.to_string(),
            };
            args.insert(name, text);
        }
        kwargs.assert_all_used()?;
        // absent in this locale: renders empty, `translations.missing` reports it
        let Some(text) = translations.by_locale.get(&locale).and_then(|m| m.get(key)) else {
            return Ok(String::new());
        };
        // one pass: an argument's value is never scanned for placeholders again
        Ok(placeholder_re()
            .replace_all(text, |c: &regex::Captures| {
                args.get(&c[1]).cloned().unwrap_or_else(|| c[0].to_string())
            })
            .into_owned())
    };
    let t = Arc::new(translate);
    let t1 = t.clone();
    env.add_function("t", move |state: &State, key: &str, kwargs: Kwargs| {
        t1(state, key, kwargs, false)
    });
    env.add_function("t_html", move |state: &State, key: &str, kwargs: Kwargs| {
        t(state, key, kwargs, true).map(Value::from_safe_string)
    });

    let loc_tags = tags.clone();
    env.add_filter(
        "loc",
        move |state: &State, v: Value| -> Result<Value, Error> {
            let locale = state_locale(state)?;
            if let Some(l) = v.downcast_object_ref::<Localized>() {
                return match l.values.get(&locale) {
                    Some(Some(s)) => Ok(Value::from(s.clone())),
                    _ => Err(Error::new(
                        ErrorKind::InvalidOperation,
                        format!("item.{} has no {locale} value", l.field),
                    )),
                };
            }
            // a plain {locale: value} map, e.g. from `data`
            let is_localized = v.kind() == minijinja::value::ValueKind::Map
                && v.try_iter().is_ok_and(|mut keys| {
                    keys.all(|k| k.as_str().is_some_and(|k| loc_tags.iter().any(|t| t == k)))
                });
            if !is_localized {
                return Err(Error::new(
                    ErrorKind::InvalidOperation,
                    format!("`| loc` needs a localized value (a map keyed by locale) — got {v}"),
                ));
            }
            match v.get_attr(&locale) {
                Ok(x) if !x.is_undefined() && !x.is_none() => Ok(x),
                _ => Err(Error::new(
                    ErrorKind::InvalidOperation,
                    format!("this localized value has no {locale} value"),
                )),
            }
        },
    );

    // Locale formatting reads the page's locale; an explicit `locale=` still wins.
    // Either may lack CLDR data: that fails here, per call, not at load.
    env.add_filter("date_format", |state: &State, v: Value, kwargs: Kwargs| {
        date_format_filter(state, v, kwargs, None)
    });
    env.add_filter("num_format", |state: &State, v: Value, kwargs: Kwargs| {
        num_format_filter(state, v, kwargs, None)
    });
}

/// Routes-layer checks (abort before rendering, like `paths.duplicate`).
pub fn routes_errors(site: &Site, pages: &[Page]) -> Vec<serde_json::Value> {
    let config = &site.config;
    let Some(tags) = config.locale_tags() else {
        return Vec::new();
    };
    let prefix_default = config.prefix_default_locale.unwrap_or(false);
    let default = config.default_locale().unwrap_or_default();
    let err = |check: &str, p: &Page| {
        serde_json::json!({
            "layer": "routes",
            "check": check,
            "page": p.path.trim_start_matches('/'),
            "route": site.routes.routes[p.route].path.to_string(),
            "template": p.template,
        })
    };
    let mut errors = Vec::new();
    for p in pages {
        if let Some(c) = p
            .path
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || "-._~/".contains(*c)))
        {
            let mut e = err("paths.invalid_chars", p);
            e["char"] = serde_json::json!(c.to_string());
            errors.push(e);
        }
        // pages that sit in the root namespace, where a prefix-named first segment
        // would shadow that locale's whole tree
        let at_root = match p.locale.as_deref() {
            None => true,
            Some(l) => l == default && !prefix_default,
        };
        if at_root {
            let first = p
                .path
                .trim_start_matches('/')
                .split('/')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if let Some(t) = tags
                .iter()
                .find(|t| prefix(t) == first && (prefix_default || t.as_str() != default))
            {
                let mut e = err("paths.locale_prefix", p);
                e["locale"] = serde_json::json!(t);
                errors.push(e);
            }
        }
    }
    if prefix_default
        && !pages
            .iter()
            .any(|p| p.locale.is_none() && p.path == "/index.html")
    {
        errors.push(serde_json::json!({
            "layer": "routes",
            "check": "paths.root_missing",
        }));
    }
    errors
}

/// Output checks that ship with the capability: `html.lang`, `hreflang.mismatch`
/// (with base_url), and the translation files' coverage.
pub fn output_errors(
    build_dir: &Path,
    site: &Site,
    pages: &[Page],
    unknown_keys: &[(String, String, String)],
) -> Result<Vec<serde_json::Value>> {
    let config = &site.config;
    if config.locale_tags().is_none() {
        return Ok(Vec::new());
    }
    use crate::check::html::{attr_re, attr_value, html_tag_re, link_tag_re};
    let (html_re, link_re) = (html_tag_re(), link_tag_re());
    let attrs = |tag: &str| -> BTreeMap<String, String> {
        attr_re()
            .captures_iter(tag)
            .map(|c| {
                (
                    c[1].to_ascii_lowercase(),
                    crate::check::html::unescape_entities(attr_value(&c)),
                )
            })
            .collect()
    };
    // each page is judged on its own: every core takes pages, results in page order
    let html_pages: Vec<&Page> = pages.iter().filter(|p| p.is_html()).collect();
    let per_page =
        crate::build::par_map(html_pages.len(), |i| -> Result<Vec<serde_json::Value>> {
            let p = html_pages[i];
            let mut errors = Vec::new();
            let rel = p.path.trim_start_matches('/');
            let Ok(html) = crate::check::read_page(&build_dir.join(rel)) else {
                return Ok(errors);
            };
            // `locales: none` pages are outside the locale dimension: no lang to match,
            // but they must not carry hreflang either (their expected set is empty)
            if let Some(locale) = p.locale.as_deref() {
                let lang = html_re
                    .find(&html)
                    .and_then(|m| attrs(m.as_str()).remove("lang"));
                // the locale itself, or a more specific tag of it (`en-US` on an `en` page):
                // both are valid, and say the same language
                let matches = |l: &str| {
                    l.eq_ignore_ascii_case(locale)
                        || (l.len() > locale.len()
                            && l.as_bytes()[locale.len()] == b'-'
                            && l[..locale.len()].eq_ignore_ascii_case(locale))
                };
                if !lang.as_deref().is_some_and(matches) {
                    errors.push(serde_json::json!({
                        "layer": "output",
                        "check": "html.lang",
                        "page": rel,
                        "found": lang,
                        "expected": locale,
                    }));
                }
            }
            let Some(base) = &config.base_url else {
                return Ok(errors);
            };
            let group = p.hreflang();
            let mut expected: BTreeMap<String, String> = group
                .iter()
                .map(|a| {
                    (
                        a.locale.to_ascii_lowercase(),
                        crate::render::page_url(base, &a.path),
                    )
                })
                .collect();
            if let Some(d) = group
                .iter()
                .find(|a| Some(a.locale.as_str()) == config.default_locale())
            {
                expected.insert("x-default".into(), crate::render::page_url(base, &d.path));
            }
            let mut found: BTreeMap<String, String> = BTreeMap::new();
            let mut duplicate: BTreeSet<String> = BTreeSet::new();
            for m in link_re.find_iter(&html) {
                let a = attrs(m.as_str());
                if a.get("rel")
                    .is_some_and(|r| r.eq_ignore_ascii_case("alternate"))
                {
                    if let Some(h) = a.get("hreflang") {
                        let h = h.to_ascii_lowercase();
                        let href = a.get("href").cloned().unwrap_or_default();
                        if found.insert(h.clone(), href).is_some() {
                            duplicate.insert(h);
                        }
                    }
                }
            }
            if found == expected && duplicate.is_empty() {
                return Ok(errors);
            }
            let missing: Vec<&String> = expected
                .keys()
                .filter(|k| !found.contains_key(*k))
                .collect();
            let extra: Vec<&String> = found
                .keys()
                .filter(|k| !expected.contains_key(*k))
                .collect();
            let wrong: Vec<serde_json::Value> = expected
                .iter()
                .filter_map(|(k, want)| {
                    let got = found.get(k)?;
                    (got != want)
                        .then(|| serde_json::json!({"hreflang": k, "found": got, "expected": want}))
                })
                .collect();
            errors.push(serde_json::json!({
                "layer": "output",
                "check": "hreflang.mismatch",
                "page": rel,
                "missing": missing,
                "extra": extra,
                "wrong_href": wrong,
                "duplicate": duplicate,
            }));
            Ok(errors)
        })?;
    let mut errors: Vec<serde_json::Value> = per_page.into_iter().flatten().collect();
    errors.extend(translation_errors(site, unknown_keys));
    Ok(errors)
}

fn translation_errors(
    site: &Site,
    unknown_keys: &[(String, String, String)],
) -> Vec<serde_json::Value> {
    let tr = &site.translations;
    let mut errors = Vec::new();
    if tr.present {
        let default = site.config.default_locale().unwrap_or_default();
        let base = &tr.by_locale[default];
        for (locale, strings) in tr.by_locale.iter().filter(|(l, _)| l.as_str() != default) {
            for (key, text) in base {
                match strings.get(key) {
                    Some(s) if !s.trim().is_empty() => {
                        let (want, got) = (placeholders(text), placeholders(s));
                        if want != got {
                            errors.push(serde_json::json!({
                                "layer": "translations",
                                "check": "translations.args",
                                "locale": locale,
                                "key": key,
                                "expected": want,
                                "found": got,
                            }));
                        }
                    }
                    _ => errors.push(serde_json::json!({
                        "layer": "translations",
                        "check": "translations.missing",
                        "locale": locale,
                        "key": key,
                    })),
                }
            }
            for key in strings.keys().filter(|k| !base.contains_key(*k)) {
                errors.push(serde_json::json!({
                    "layer": "translations",
                    "check": "translations.extra",
                    "locale": locale,
                    "key": key,
                }));
            }
        }
    }
    // one entry per (template, key), with the locales it was called in
    let mut by_key: BTreeMap<(&str, &str), Vec<&str>> = BTreeMap::new();
    for (template, key, locale) in unknown_keys {
        by_key.entry((template, key)).or_default().push(locale);
    }
    for ((template, key), locales) in by_key {
        let mut e = serde_json::json!({
            "layer": "translations",
            "check": "translations.unknown_key",
            "key": key,
            "locales": locales,
        });
        match pattern_source(site, template) {
            Some(src) => e["source"] = serde_json::json!(src),
            None => e["template"] = serde_json::json!(template),
        }
        errors.push(e);
    }
    errors
}

/// `__title_3.txt` -> "routes.yaml route 3 (/path) title": route patterns are
/// templates internally, but the author wrote them in routes.yaml.
fn pattern_source(site: &Site, template: &str) -> Option<String> {
    let (kind, idx) = template
        .strip_prefix("__")?
        .strip_suffix(".txt")?
        .rsplit_once('_')?;
    let idx: usize = idx.parse().ok()?;
    let route = site.routes.routes.get(idx)?;
    Some(format!("routes.yaml route {idx} ({}) {kind}", route.path))
}

/// Page counts per locale for one route, keyed by locale (`list_routes`, the sparse
/// warning).
pub fn counts_by_locale(site: &Site, route: usize, pages: &[Page]) -> BTreeMap<String, usize> {
    let mut counts: BTreeMap<String, usize> = route_tags(&site.config, &site.routes.routes[route])
        .into_iter()
        .map(|t| (t, 0))
        .collect();
    for p in pages.iter().filter(|p| p.route == route) {
        if let Some(l) = &p.locale {
            *counts.entry(l.clone()).or_default() += 1;
        }
    }
    counts
}

/// `i18n.sparse_locale`: a missing locale value is a legitimate "no page", so a data
/// builder that forgets a whole locale would pass every check. A locale with fewer
/// than half the default locale's pages for a route is flagged (a warning).
pub fn warnings(site: &Site, pages: &[Page]) -> Vec<serde_json::Value> {
    let Some(default) = site.config.default_locale() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (idx, route) in site.routes.routes.iter().enumerate() {
        let counts = counts_by_locale(site, idx, pages);
        let Some(&d) = counts.get(default) else {
            continue;
        };
        for tag in route_tags(&site.config, route) {
            let n = counts.get(&tag).copied().unwrap_or(0);
            if tag != default && 2 * n < d {
                let mut w = route_id(route);
                w["layer"] = serde_json::json!("routes");
                w["check"] = serde_json::json!("i18n.sparse_locale");
                w["locale"] = serde_json::json!(tag);
                w["pages"] = serde_json::json!(n);
                w["default_pages"] = serde_json::json!(d);
                out.push(w);
            }
        }
    }
    out
}

/// How the report names a route outside an error: its path pattern, its template and,
/// on `foreach` routes, the collection. Several routes can share one pattern (three
/// `foreach` routes at `/{slug}/index.html`); the template and collection tell them apart.
pub(crate) fn route_id(route: &Route) -> serde_json::Value {
    let mut id = serde_json::json!({ "route": route.path.to_string(), "template": route.template });
    if let Some(f) = &route.foreach {
        id["foreach"] = serde_json::json!(f);
    }
    id
}

/// `alternates` in a page's template context, for a language switcher:
/// `{locale, path, url?}` per locale version of the page, noindex ones included (a
/// reader can still switch to them; only hreflang leaves them out). No x-default.
pub fn alternates_value(site: &Site, page: &Page) -> Value {
    let list: Vec<Value> = page
        .alternates
        .iter()
        .map(|a| match &site.config.base_url {
            Some(base) => minijinja::context!(
                locale => a.locale,
                path => public_path(&a.path),
                url => crate::render::page_url(base, &a.path),
            ),
            None => minijinja::context!(locale => a.locale, path => public_path(&a.path)),
        })
        .collect();
    Value::from(list)
}
