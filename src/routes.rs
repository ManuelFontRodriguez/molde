use crate::locales;
use crate::render::{filters_env, public_path, slug};
use crate::site::{fill_re, path_problem, Alternate, IndexRule, Page, Paginate, Site};
use anyhow::{bail, Context, Result};
use minijinja::{context, Environment};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, OnceLock};

/// Expand every route against the snapshot into the full, ordered page set.
/// Under `locales:` a route expands once per locale (routes × locales × items), and
/// each page learns its hreflang group — the manifest is the single source of it.
pub fn expand_routes(site: &Site) -> Result<Vec<Page>> {
    let env = route_meta_env(site)?;
    let mut pages = Vec::new();
    // hreflang group per page: same route + same item index / page number
    let mut groups: Vec<(usize, usize)> = Vec::new();
    // one copy per data item, shared by all its pages: a site in 22 locales would
    // otherwise hold 22 copies of every item. Keyed by the item's address, which is
    // stable because every item is borrowed from `site.snapshot`, alive and unchanged
    // for this whole function; never key it by values that could be dropped.
    let mut shared: HashMap<*const serde_json::Value, Arc<serde_json::Value>> = HashMap::new();
    for (idx, route) in site.routes.routes.iter().enumerate() {
        let tags = locales::route_tags(&site.config, route);
        let dims: Vec<Option<&str>> = match tags.is_empty() {
            true => vec![None],
            false => tags.iter().map(|t| Some(t.as_str())).collect(),
        };
        for locale in dims {
            expand_route(
                site,
                &env,
                idx,
                locale,
                &mut pages,
                &mut groups,
                &mut shared,
            )?;
        }
    }
    assign_alternates(&mut pages, &groups);
    Ok(pages)
}

fn expand_route(
    site: &Site,
    env: &Environment,
    idx: usize,
    locale: Option<&str>,
    pages: &mut Vec<Page>,
    groups: &mut Vec<(usize, usize)>,
    shared: &mut HashMap<*const serde_json::Value, Arc<serde_json::Value>>,
) -> Result<()> {
    let route = &site.routes.routes[idx];
    let Some(pattern) = route.path.for_locale(locale) else {
        return Ok(()); // a per-locale path map without this locale: no page
    };
    let mut push = |member: usize,
                    path: String,
                    item: Option<Arc<serde_json::Value>>,
                    pagination: Option<minijinja::Value>|
     -> Result<()> {
        let ctx = meta_ctx(site, idx, locale, item.as_deref(), pagination.as_ref());
        pages.push(Page {
            route: idx,
            index: resolve_index(env, site, idx, &path, &ctx)?,
            lastmod: resolve_lastmod(env, site, idx, &path, &ctx)?,
            path,
            template: route.template.clone(),
            item,
            pagination,
            locale: locale.map(String::from),
            alternates: Arc::default(),
        });
        groups.push((idx, member));
        Ok(())
    };
    if let Some(pg) = &route.paginate {
        for (n, (path, pagination)) in paginate(site, idx, pg, locale)?.into_iter().enumerate() {
            push(n, path, None, Some(pagination))?;
        }
        return Ok(());
    }
    match &route.foreach {
        None => push(
            0,
            locales::localize_path(&site.config, locale, pattern),
            None,
            None,
        ),
        Some(collection) => {
            let what = match route.group_by {
                Some(_) => "group_by (over foreach)",
                None => "foreach",
            };
            let items = collection_items(site, idx, what, collection)?;
            let fields = route.localized_fields();
            if let Some(field) = &route.group_by {
                for (g, group) in group_items(items, field, idx, route)?
                    .into_iter()
                    .enumerate()
                {
                    let Some(path) =
                        fill_pattern(pattern, &group, locale, fields).with_context(|| {
                            format!("route {} ({}), group {}", idx, route.path, group[field])
                        })?
                    else {
                        continue;
                    };
                    push(
                        g,
                        locales::localize_path(&site.config, locale, &path),
                        Some(Arc::new(group)),
                        None,
                    )?;
                }
                return Ok(());
            }
            for (i, item) in items.iter().enumerate() {
                if !fields.is_empty() {
                    let what = format!("route {} ({}), {collection}[{i}]", idx, route.path);
                    locales::validate_localized_item(&site.config, fields, &what, item)?;
                    if !locale.is_some_and(|l| locales::item_exists_in(fields, l, item)) {
                        continue; // no value in this locale: no page, never a fallback
                    }
                }
                let Some(path) =
                    fill_pattern(pattern, item, locale, fields).with_context(|| {
                        format!("route {} ({}), {collection}[{i}]", idx, route.path)
                    })?
                else {
                    continue; // no value in this locale: no page, never a fallback
                };
                push(
                    i,
                    locales::localize_path(&site.config, locale, &path),
                    Some(
                        shared
                            .entry(item as *const serde_json::Value)
                            .or_insert_with(|| Arc::new(item.clone()))
                            .clone(),
                    ),
                    None,
                )?;
            }
            Ok(())
        }
    }
}

/// A route's collection: the top-level array `name` of data.json. When it is not there
/// (a typo, a renamed key) the error names the route and lists the arrays that are.
fn collection_items<'a>(
    site: &'a Site,
    idx: usize,
    what: &str,
    name: &str,
) -> Result<&'a [serde_json::Value]> {
    let at = || format!("route {} ({})", idx, site.routes.routes[idx].path);
    let top = site.snapshot.as_object();
    match top.and_then(|o| o.get(name)) {
        Some(serde_json::Value::Array(items)) => Ok(items),
        Some(_) => bail!(
            "{}: {what} '{name}' is in data.json but is not an array — it must be a top-level array",
            at()
        ),
        None => {
            let arrays: Vec<&str> = top
                .into_iter()
                .flatten()
                .filter(|(_, v)| v.is_array())
                .map(|(k, _)| k.as_str())
                .collect();
            let have = match arrays.is_empty() {
                true => "data.json has no top-level arrays".to_string(),
                false => format!("the top-level arrays in data.json are: {}", arrays.join(", ")),
            };
            bail!(
                "{}: {what} '{name}' is not in data.json; {have} — fix the name in routes.yaml or add the array",
                at()
            )
        }
    }
}

/// `group_by`: one item per distinct value of `field`, in the order each value first
/// appears, `{field: value, "items": [the collection's items with that value]}`. A list
/// field (tags) puts an item in every group it names; an empty list in none. An item with
/// no value at all is an error, never silently left out.
fn group_items(
    items: &[serde_json::Value],
    field: &str,
    idx: usize,
    route: &crate::site::Route,
) -> Result<Vec<serde_json::Value>> {
    use serde_json::Value;
    let key_of = |v: &Value| match v {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    };
    let mut order: Vec<String> = Vec::new();
    let mut members: HashMap<String, Vec<Value>> = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        let keys: Vec<String> = match item.get(field) {
            Some(Value::Array(values)) => values
                .iter()
                .map(|v| {
                    key_of(v).with_context(|| {
                        format!(
                            "route {idx} ({}): item {i}'s '{field}' list holds {v}, not a \
                             non-empty string or number to group by",
                            route.path
                        )
                    })
                })
                .collect::<Result<_>>()?,
            Some(v) => vec![key_of(v).with_context(|| {
                format!(
                    "route {idx} ({}): item {i} has '{field}' = {v}, not a non-empty string or \
                     number to group by",
                    route.path
                )
            })?],
            None => bail!(
                "route {idx} ({}): item {i} has no '{field}' to group by — give every item \
                 one, or leave the item out of the collection",
                route.path
            ),
        };
        let mut seen = BTreeSet::new();
        for key in keys {
            if !seen.insert(key.clone()) {
                continue; // a tag listed twice on one item: one membership
            }
            if !members.contains_key(&key) {
                order.push(key.clone());
            }
            members.entry(key).or_default().push(item.clone());
        }
    }
    Ok(order
        .into_iter()
        .map(|key| {
            let items = members.remove(&key).unwrap_or_default();
            serde_json::json!({ field: key, "items": items })
        })
        .collect())
}

/// Locale versions: the HTML pages of one group, noindex included; a group of one is
/// no group. `Page::hreflang` narrows it to the indexable ones.
fn assign_alternates(pages: &mut [Page], groups: &[(usize, usize)]) {
    let mut members: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (i, p) in pages.iter().enumerate() {
        if p.locale.is_some() && p.is_html() {
            members.entry(groups[i]).or_default().push(i);
        }
    }
    for m in members.values().filter(|m| m.len() >= 2) {
        let list: Arc<Vec<Alternate>> = Arc::new(
            m.iter()
                .map(|&i| Alternate {
                    locale: pages[i].locale.clone().unwrap_or_default(),
                    path: pages[i].path.clone(),
                    index: pages[i].index,
                })
                .collect(),
        );
        for &i in m {
            pages[i].alternates = list.clone();
        }
    }
}

/// Split a `paginate` route's list into (output path, `pagination` context) per page.
/// An empty list still yields page 1, so the route's own path always exists. Under
/// `locales:` each locale paginates only the items that exist in it.
pub(crate) fn paginate(
    site: &Site,
    idx: usize,
    pg: &Paginate,
    locale: Option<&str>,
) -> Result<Vec<(String, minijinja::Value)>> {
    let route = &site.routes.routes[idx];
    let at = || format!("route {} ({})", idx, route.path);
    if route.foreach.is_some() {
        bail!(
            "{}: `paginate` and `foreach` can't be combined on one route",
            at()
        );
    }
    if route.path.all().iter().any(|p| p.contains('{')) {
        bail!(
            "{}: a paginate route's path is page 1 and can't use placeholders — got {:?}",
            at(),
            route.path.to_string()
        );
    }
    if pg.per_page == 0 {
        bail!("{}: paginate.per_page must be at least 1", at());
    }
    for p in pg.path.all() {
        if let Some(problem) = path_problem(&p.replace("{n}", "1")) {
            bail!("{}: paginate.path {p:?} {problem}", at());
        }
        if !p.starts_with('/') || !p.contains("{n}") {
            bail!(
                "{}: paginate.path must start with / and contain {{n}} (the page number), \
                 e.g. \"/top/page/{{n}}/index.html\" — got {:?}",
                at(),
                p
            );
        }
        if p.replace("{n}", "").contains('{') {
            bail!("{}: paginate.path can only use {{n}} — got {:?}", at(), p);
        }
    }
    let (Some(first), Some(rest)) = (route.path.for_locale(locale), pg.path.for_locale(locale))
    else {
        return Ok(Vec::new());
    };
    let all = collection_items(site, idx, "paginate.over", &pg.over)?;
    let fields = route.localized_fields();
    let owned: Vec<serde_json::Value>;
    let list: &[serde_json::Value] = match locale {
        Some(l) if !fields.is_empty() => {
            for (i, item) in all.iter().enumerate() {
                let what = format!("route {} ({}), {}[{i}]", idx, route.path, pg.over);
                locales::validate_localized_item(&site.config, fields, &what, item)?;
            }
            owned = all
                .iter()
                .filter(|item| locales::item_exists_in(fields, l, item))
                .cloned()
                .collect();
            &owned
        }
        _ => all,
    };
    let pages = list.len().div_ceil(pg.per_page).max(1);
    let path_of = |n: usize| {
        let unprefixed = match n {
            1 => first.to_string(),
            n => rest.replace("{n}", &n.to_string()),
        };
        locales::localize_path(&site.config, locale, &unprefixed)
    };
    let public = |n: usize| public_path(&path_of(n)).to_string();
    // built once and shared: cloning a minijinja::Value is an Arc bump
    let links: minijinja::Value = (1..=pages)
        .map(|n| context!(n => n, path => public(n)))
        .collect::<Vec<_>>()
        .into();
    Ok((1..=pages)
        .map(|n| {
            let offset = (n - 1) * pg.per_page;
            let end = (offset + pg.per_page).min(list.len());
            let slice = list.get(offset..end).unwrap_or(&[]);
            let items = match fields.is_empty() {
                true => minijinja::Value::from_serialize(slice),
                false => slice
                    .iter()
                    .map(|i| locales::item_value(fields, i))
                    .collect::<Vec<_>>()
                    .into(),
            };
            let pagination = context!(
                n => n,
                pages => pages,
                per_page => pg.per_page,
                total => list.len(),
                offset => offset,
                items => items,
                prev => (n > 1).then(|| public(n - 1)),
                next => (n < pages).then(|| public(n + 1)),
                links => links.clone(),
            );
            (path_of(n), pagination)
        })
        .collect())
}

/// The locale a page renders with: its own, or the default for `locales: none`
/// pages. None without the capability.
pub(crate) fn render_locale<'a>(site: &'a Site, locale: Option<&'a str>) -> Option<&'a str> {
    locale.or(site.config.default_locale())
}

/// The context every route pattern (title, description, index, lastmod) renders with.
pub(crate) fn meta_ctx(
    site: &Site,
    route: usize,
    locale: Option<&str>,
    item: Option<&serde_json::Value>,
    pagination: Option<&minijinja::Value>,
) -> minijinja::Value {
    let fields = site.routes.routes[route].localized_fields();
    let item = item.map(|i| locales::item_value(fields, i));
    let base = context!(item => item, data => site.snapshot_value());
    let base = match render_locale(site, locale) {
        Some(l) => context!(locale => l, locales => site.config.locales, ..base),
        None => base,
    };
    // like render_page: `pagination` exists only on paginate routes (`is defined`)
    match pagination {
        Some(p) => context!(pagination => p, ..base),
        None => base,
    }
}

/// Route metadata that decides the manifest (which pages are indexable, their
/// sitemap lastmod) is resolved at expansion time, so every consumer — build,
/// check, list_routes — sees the same answer. Patterns get the same filters as
/// templates.
fn route_meta_env(site: &Site) -> Result<Environment<'static>> {
    let mut env = filters_env(site)?;
    for (idx, route) in site.routes.routes.iter().enumerate() {
        if let Some(IndexRule::Pattern(pattern)) = &route.index {
            env.add_template_owned(format!("__index_{idx}.txt"), pattern.clone())
                .with_context(|| format!("index pattern for route {} ({})", idx, route.path))?;
        }
        if let Some(pattern) = &route.lastmod {
            env.add_template_owned(format!("__lastmod_{idx}.txt"), pattern.clone())
                .with_context(|| format!("lastmod pattern for route {} ({})", idx, route.path))?;
        }
    }
    Ok(env)
}

fn resolve_lastmod(
    env: &Environment,
    site: &Site,
    idx: usize,
    path: &str,
    ctx: &minijinja::Value,
) -> Result<Option<String>> {
    let route = &site.routes.routes[idx];
    if route.lastmod.is_none() {
        return Ok(None);
    }
    let rendered = env
        .get_template(&format!("__lastmod_{idx}.txt"))
        .and_then(|t| t.render(ctx))
        .with_context(|| {
            format!(
                "lastmod pattern for route {} ({}), page {path}",
                idx, route.path
            )
        })?;
    let rendered = rendered.trim();
    Ok((!rendered.is_empty()).then(|| rendered.to_string()))
}

/// Sitemap <lastmod>: a calendar date `YYYY-MM-DD` or an RFC 3339 datetime
/// (`2026-09-23T09:00:00Z`, `…T09:00:00.5+02:00`). Both are valid W3C Datetime,
/// which is what the sitemap protocol asks for.
pub fn is_valid_lastmod(s: &str) -> bool {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(
            // [0-9], not \d: \d matches every Unicode digit (٢٠٢٦), which `parse` then
            // rejects; a lastmod is ASCII by definition
            r"^([0-9]{4})-([0-9]{2})-([0-9]{2})(?:[Tt]([0-9]{2}):([0-9]{2}):([0-9]{2})(?:\.[0-9]+)?(?:[Zz]|[+-]([0-9]{2}):([0-9]{2})))?$",
        )
        .unwrap()
    });
    let Some(c) = re.captures(s) else {
        return false;
    };
    let num = |i: usize| c.get(i).map(|m| m.as_str().parse::<u32>().unwrap());
    let (y, m, d) = (num(1).unwrap(), num(2).unwrap(), num(3).unwrap());
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    if d == 0 || d > days {
        return false;
    }
    // RFC 3339 allows second 60 (leap second)
    let time_ok = match (num(4), num(5), num(6)) {
        (Some(h), Some(mi), Some(sec)) => h < 24 && mi < 60 && sec <= 60,
        _ => true,
    };
    let offset_ok = match (num(7), num(8)) {
        (Some(h), Some(mi)) => h < 24 && mi < 60,
        _ => true,
    };
    time_ok && offset_ok
}

fn resolve_index(
    env: &Environment,
    site: &Site,
    idx: usize,
    path: &str,
    ctx: &minijinja::Value,
) -> Result<bool> {
    let route = &site.routes.routes[idx];
    match &route.index {
        None => Ok(true),
        Some(IndexRule::Fixed(b)) => Ok(*b),
        Some(IndexRule::Pattern(_)) => {
            let rendered = env
                .get_template(&format!("__index_{idx}.txt"))
                .and_then(|t| t.render(ctx))
                .with_context(|| {
                    format!(
                        "index pattern for route {} ({}), page {path}",
                        idx, route.path
                    )
                })?;
            // minijinja prints booleans Jinja-style (`True`), YAML-minded authors
            // write `true` — both spellings, nothing looser
            match rendered.trim() {
                "true" | "True" => Ok(true),
                "false" | "False" => Ok(false),
                other => bail!(
                    "index pattern for route {} ({}) rendered {other:?} for page {path} — \
                     it must render exactly `true` or `false` (a missing field renders empty)",
                    idx,
                    route.path
                ),
            }
        }
    }
}

/// "/books/{id}/index.html" + item -> "/books/dune/index.html", and
/// `{genre|slug}` writes the value as a slug ("Science Fiction" ->
/// "science-fiction"). A localized
/// field fills with its value in `locale`; None = the item has no page there.
fn fill_pattern(
    pattern: &str,
    item: &serde_json::Value,
    locale: Option<&str>,
    localized: &[String],
) -> Result<Option<String>> {
    let re = fill_re();
    let mut out = String::new();
    let mut last = 0;
    for cap in re.captures_iter(pattern) {
        let m = cap.get(0).unwrap();
        let key = &cap[1];
        let number;
        let val = if localized.iter().any(|f| f == key) {
            let field = item.get(key).with_context(|| {
                format!("item has no localized field '{key}', which the path pattern uses")
            })?;
            match locale.and_then(|l| field.get(l)).and_then(|v| v.as_str()) {
                Some(v) => v,
                None => return Ok(None),
            }
        } else {
            match item.get(key) {
                Some(serde_json::Value::String(s)) => s,
                // an integer id (42) is a fine path segment; floats and booleans are not
                // (1.0 and 1 would be two spellings of one page)
                Some(serde_json::Value::Number(n)) if n.is_i64() || n.is_u64() => {
                    number = n.to_string();
                    &number
                }
                Some(other) => bail!(
                    "item field '{key}' is {other}, not a string or an integer, which the path \
                     pattern needs — store it as a string in data.json"
                ),
                None => bail!("item has no field '{key}', which the path pattern uses"),
            }
        };
        let slugged;
        let val = if cap.get(2).is_some() {
            slugged = slug(val);
            if slugged.is_empty() {
                bail!(
                    "item field '{key}' value {val:?} has no letter or digit a slug can keep \
                     (only ASCII letters and digits, accents dropped, are kept) — give the \
                     items a slug field of their own and use it in the path"
                );
            }
            slugged.as_str()
        } else {
            val
        };
        // data is untrusted input to the router: a {field} value must stay a
        // single, safe path segment — never a traversal or a nested dir
        if val.is_empty() || val.contains('/') || val.contains('\\') || val.contains("..") {
            bail!("item field '{key}' value {val:?} is not a safe path segment");
        }
        if let Some(problem) = path_problem(&format!("/{val}")) {
            bail!("item field '{key}' value {val:?} cannot be part of a path: it {problem}");
        }
        out.push_str(&pattern[last..m.start()]);
        out.push_str(val);
        last = m.end();
    }
    out.push_str(&pattern[last..]);
    Ok(Some(out))
}
