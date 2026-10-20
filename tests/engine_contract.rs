//! Engine contract: what a template author observes from the template
//! engine, pinned. Each behaviour here is stated in docs/reference.md "Engine contract";
//! a minijinja upgrade or feature flag that moves one fails this file first.
use minijinja::{context, Environment};
use std::fs;
use std::path::PathBuf;

fn site_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("molde-contract-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(dir.join("routes.yaml"), "routes: []\n").unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    fs::write(dir.join("templates/_p.html.j2"), "[{{ who }}]").unwrap();
    fs::write(
        dir.join("templates/_m.html.j2"),
        "{% macro hi() %}<{{ who }}>{% endmacro %}",
    )
    .unwrap();
    dir
}

/// molde's real template environment (same options, formatter and filters as a build).
fn env(tag: &str) -> Environment<'static> {
    molde::testing::engine(&molde::testing::load_site(&site_dir(tag)).unwrap()).unwrap()
}

fn render(env: &Environment, name: &str, src: &str, ctx: minijinja::Value) -> String {
    env.template_from_named_str(name, src)
        .unwrap()
        .render(ctx)
        .unwrap_or_else(|e| panic!("{name}: {e:#}"))
}

fn render_err(env: &Environment, name: &str, src: &str, ctx: minijinja::Value) -> String {
    let err = env
        .template_from_named_str(name, src)
        .unwrap()
        .render(ctx)
        .expect_err("must fail");
    format!("{err:#}")
}

const RAW: &str = "a/b <&>\"'";
const ESCAPED: &str = "a/b &lt;&amp;&gt;&quot;&#x27;";

#[test]
fn autoescape_is_decided_by_extension() {
    let env = env("ext");
    // after stripping .j2 / .jinja2 / .jinja: HTML for html, htm and xml, nothing else
    for name in ["a.html", "a.html.j2", "a.htm", "a.xml.j2", "a.html.jinja2"] {
        assert_eq!(
            render(&env, name, "{{ s }}", context! { s => RAW }),
            ESCAPED,
            "{name}"
        );
    }
    for name in [
        "a.json.j2",
        "a.js.j2",
        "a.yaml.j2",
        "a.txt.j2",
        "a.css.j2",
        "a.svg.j2",
        "a.j2",
        "a.jinja",
    ] {
        assert_eq!(
            render(&env, name, "{{ s }}", context! { s => RAW }),
            RAW,
            "{name}"
        );
    }
}

#[test]
fn html_escapes_five_characters_and_not_slash() {
    let env = env("five");
    let ctx = || context! { s => RAW };
    assert_eq!(render(&env, "a.html", "{{ s }}", ctx()), ESCAPED);
    assert_eq!(render(&env, "a.html", "{{ s | safe }}", ctx()), RAW);
    // escaping happens once, on output: filters see and return the raw string
    assert_eq!(
        render(&env, "a.html", "{{ s | upper }}", ctx()),
        "A/B &lt;&amp;&gt;&quot;&#x27;"
    );
    // the explicit `escape` / `e` filter escapes exactly like autoescape,
    // once, and in any output type; its result is safe
    for name in ["a.html", "a.txt"] {
        for src in ["{{ s | e }}", "{{ s | escape }}", "{{ s | e | e }}"] {
            assert_eq!(render(&env, name, src, ctx()), ESCAPED, "{name} {src}");
        }
    }
    assert_eq!(render(&env, "a.html", "{{ s | safe | e }}", ctx()), RAW);
    assert_eq!(
        render(
            &env,
            "a.html",
            "[{{ nope | e }}][{{ n | e }}][{{ 1.5 | e }}][{{ [1, '<'] | e }}]",
            context! { n => () }
        ),
        "[][][1.5][[1, &quot;&lt;&quot;]]"
    );
}

#[test]
fn none_and_undefined() {
    let env = env("none");
    let data: serde_json::Value = serde_json::from_str(r#"{"n": null, "m": {"a": 1}}"#).unwrap();
    let ctx = || minijinja::Value::from_serialize(&data);
    let r = |src: &str| render(&env, "a.html", src, ctx());
    // undefined (a name or key that doesn't exist): empty, falsy, iterable as empty
    assert_eq!(r("[{{ nope }}][{{ m.zzz }}][{{ m['zzz'] }}]"), "[][][]");
    assert_eq!(
        r("{{ nope is defined }} {{ nope is undefined }}"),
        "False True"
    );
    assert_eq!(r("{% if nope %}T{% else %}F{% endif %}"), "F");
    assert_eq!(r("{% for x in nope %}{{ x }}{% endfor %}ok"), "ok");
    assert_eq!(r("{{ nope | default('d') }}|{{ nope ~ 'x' }}"), "d|x");
    // an attribute OF an undefined value is an error, not empty
    assert!(render_err(&env, "a.html", "{{ nope.x }}", ctx()).contains("undefined value"));
    // none (a JSON null in data.json) is a value that PRINTS as nothing,
    // in every output type; only converting it to a string first spells "None"
    assert_eq!(r("[{{ n }}]"), "[]");
    for name in ["a.txt", "a.json", "a.xml"] {
        assert_eq!(render(&env, name, "[{{ n }}]", ctx()), "[]", "{name}");
    }
    assert_eq!(r("{{ n ~ 'x' }}|{{ n | string }}"), "Nonex|None");
    assert_eq!(r("{{ n is none }} {{ n is defined }}"), "True True");
    assert_eq!(r("{% if n %}T{% else %}F{% endif %}"), "F");
    // `default` only replaces undefined, unless its second argument is true
    assert_eq!(
        r("{{ n | default('d') }}|{{ n | default('d', true) }}"),
        "|d"
    );
    assert_eq!(r("[{{ n.x }}]"), "[]");
    assert!(render_err(&env, "a.html", "{{ n | length }}", ctx())
        .contains("cannot calculate length of value of type none"));
    // looping over none is empty, but a containment test on it errors (undefined: False)
    assert_eq!(r("{% for x in n %}{{ x }}{% endfor %}ok"), "ok");
    assert!(render_err(&env, "a.html", "{{ 'x' in n }}", ctx())
        .contains("cannot perform a containment check"));
    assert_eq!(r("{{ 'x' in nope }}"), "False");
}

#[test]
fn booleans_and_numbers_print_python_style() {
    let env = env("num");
    let data: serde_json::Value =
        serde_json::from_str(r#"{"t": true, "i": 3, "f": 1.0, "g": 0.5}"#).unwrap();
    let r = |src: &str| render(&env, "a.txt", src, minijinja::Value::from_serialize(&data));
    // `index: "{{ item.x }}"` patterns rely on this spelling
    assert_eq!(r("{{ t }} {{ not t }}"), "True False");
    // a float keeps its ".0"; an integer from data.json stays an integer
    assert_eq!(r("{{ i }} {{ f }} {{ g }}"), "3 1.0 0.5");
    // `/` is float division, `//` integer division
    assert_eq!(r("{{ 10 / 5 }} {{ 7 // 2 }} {{ i / 2 }}"), "2.0 3 1.5");
    assert_eq!(r("{{ 1 / 3 }}"), "0.3333333333333333");
    assert_eq!(r("{{ 1.5 | round }} {{ 1.25 | round(1) }}"), "2.0 1.3");
    // an integer stays an integer; halves round away from zero (Python rounds to even)
    assert_eq!(r("{{ 3 | round }} {{ 2.5 | round }}"), "3 3.0");
    // `int` truncates toward zero; a non-numeric string is an error, not 0
    assert_eq!(
        r("{{ 3.7 | int }} {{ -3.7 | int }} {{ '3' | int }}"),
        "3 -3 3"
    );
    assert!(render_err(&env, "a.txt", "{{ 'x' | int }}", context! {}).contains("invalid"));
    assert_eq!(
        r("{{ '%.2f' | format(2.5) }} {{ 12 | float }}"),
        "2.50 12.0"
    );
}

#[test]
fn tojson_sorts_keys_and_keeps_list_order() {
    let env = env("json");
    let r = |src: &str| render(&env, "a.html", src, context! {});
    assert_eq!(
        r("{{ {'b': 1, 'a': {'d': 1, 'c': 2}} | tojson }}"),
        r#"{"a":{"c":2,"d":1},"b":1}"#
    );
    assert_eq!(r("{{ [3, 1, 2] | tojson }}"), "[3,1,2]");
    // non-ASCII and '/' are left as-is; the HTML-significant characters are \u-escaped
    assert_eq!(r("{{ 'é/<' | tojson }}"), r#""é/\u003c""#);
    assert_eq!(r("{{ 1.0 | tojson }} {{ none | tojson }}"), "1.0 null");
    assert_eq!(
        r(r#"{{ {'a': "<>&'"} | tojson }}"#),
        "{\"a\":\"\\u003c\\u003e\\u0026\\u0027\"}"
    );
    assert_eq!(
        r("{{ {'b': 1, 'a': [1, 2]} | tojson(indent=2) }}"),
        "{\n  \"a\": [\n    1,\n    2\n  ],\n  \"b\": 1\n}"
    );
    // data.json objects come out key-sorted too, whatever their order in the file
    let data: serde_json::Value = serde_json::from_str(r#"{"m": {"z": 1, "a": 2}}"#).unwrap();
    assert_eq!(
        render(
            &env,
            "a.html",
            "{{ m | tojson }}",
            minijinja::Value::from_serialize(&data)
        ),
        r#"{"a":2,"z":1}"#
    );
}

#[test]
fn trailing_newline_is_dropped_and_whitespace_is_kept() {
    let env = env("ws");
    let r = |src: &str| render(&env, "a.txt", src, context! { t => true });
    assert_eq!(r("x\n"), "x");
    assert_eq!(r("x\n\n"), "x\n");
    // no trim_blocks / lstrip_blocks: the tag lines' newlines and indentation stay
    assert_eq!(r("{% if t %}\n  y\n{% endif %}\nz"), "\n  y\n\nz");
    assert_eq!(r("  {% if t %}y{% endif %}"), "  y");
    // explicit whitespace control is the way to trim
    assert_eq!(r("{%- if t -%}\n  y\n{%- endif %}|"), "y|");
}

#[test]
fn include_and_import_both_see_the_callers_context() {
    let env = env("ctx");
    let ctx = || context! { who => "W" };
    let r = |src: &str| render(&env, "a.html", src, ctx());
    assert_eq!(r("{% include '_p.html.j2' %}"), "[W]");
    assert_eq!(r("{% set who = 'L' %}{% include '_p.html.j2' %}"), "[L]");
    // an include inside a loop sees the loop variable
    assert_eq!(
        r("{% for who in ['A'] %}{% include '_p.html.j2' %}{% endfor %}"),
        "[A]"
    );
    // unlike Jinja2, where import is "without context" by default, minijinja macros
    // resolve names in the importing template's top-level context as it stood at the
    // import, earlier `set` variables included
    assert_eq!(r("{% import '_m.html.j2' as m %}{{ m.hi() }}"), "<W>");
    assert_eq!(
        r("{% set who = 'L' %}{% import '_m.html.j2' as m %}{{ m.hi() }}"),
        "<L>"
    );
    assert_eq!(
        r("{% set who = 'L' %}{% from '_m.html.j2' import hi %}{{ hi() }}"),
        "<L>"
    );
    // ... but not loop / `with` variables, nor a `set` after the import: pass those as
    // macro arguments
    assert_eq!(
        r("{% import '_m.html.j2' as m %}{% for who in ['A'] %}{{ m.hi() }}{% endfor %}"),
        "<W>"
    );
    assert_eq!(
        r("{% import '_m.html.j2' as m %}{% with who = 'L' %}{{ m.hi() }}{% endwith %}"),
        "<W>"
    );
    assert_eq!(
        r("{% import '_m.html.j2' as m %}{% set who = 'L' %}{{ m.hi() }}"),
        "<W>"
    );
}

#[test]
fn set_inside_a_loop_does_not_leak() {
    let env = env("scope");
    let r = |src: &str| render(&env, "a.txt", src, context! {});
    assert_eq!(
        r("{% set y = 1 %}{% for x in [1] %}{% set y = 2 %}{{ y }}{% endfor %}{{ y }}"),
        "21"
    );
    assert_eq!(
        r("{% for x in [1] %}{% set z = 2 %}{% endfor %}[{{ z }}]"),
        "[]"
    );
}

#[test]
fn dict_iteration_and_sort_order() {
    let env = env("order");
    let data: serde_json::Value =
        serde_json::from_str(r#"{"m": {"b": 1, "a": 2, "B": 3, "_": 4}}"#).unwrap();
    let r = |src: &str| render(&env, "a.txt", src, minijinja::Value::from_serialize(&data));
    // iterating a data.json object goes by key in byte order, not file order
    assert_eq!(r("{% for k in m %}{{ k }},{% endfor %}"), "B,_,a,b,");
    assert_eq!(
        r("{% for k, v in m | items %}{{ k }}={{ v }},{% endfor %}"),
        "B=3,_=4,a=2,b=1,"
    );
    // `dictsort` and `sort` fold ASCII case by default
    assert_eq!(
        r("{% for k, v in m | dictsort %}{{ k }}={{ v }},{% endfor %}"),
        "_=4,a=2,B=3,b=1,"
    );
    assert_eq!(
        r("{{ ['b', 'A', 'a', 'B'] | sort | join(',') }}"),
        "A,a,b,B"
    );
    assert_eq!(
        r("{{ ['b', 'A', 'a', 'B'] | sort(case_sensitive=true) | join(',') }}"),
        "A,B,a,b"
    );
    // no `unicode` feature: only ASCII is folded, so É and é stay apart, by code point
    assert_eq!(r("{{ ['é', 'É'] | sort | join(',') }}"), "É,é");
    assert_eq!(r("{{ ['z', 'É', 'a'] | sort | join(',') }}"), "a,z,É");
}

#[test]
fn filters_not_python_methods() {
    let env = env("methods");
    let ctx = || context! { s => "ab" };
    assert_eq!(render(&env, "a.txt", "{{ s | upper }}", ctx()), "AB");
    // no pycompat: str/dict methods are unknown
    assert!(render_err(&env, "a.txt", "{{ s.upper() }}", ctx()).contains("unknown method"));
    assert!(render_err(&env, "a.txt", "{{ {'a': 1}.items() }}", ctx()).contains("unknown method"));
    // the Python string methods agents reach for, and what to write instead
    let csv = || context! { s => "a,b" };
    for method in [
        "{{ s.split(',') }}",
        "{{ s.startswith('a') }}",
        "{{ s.endswith('b') }}",
    ] {
        assert!(
            render_err(&env, "a.txt", method, csv()).contains("unknown method"),
            "{method}"
        );
    }
    assert_eq!(
        render(&env, "a.txt", "{{ s | split(',') | join('|') }}", csv()),
        "a|b"
    );
    assert_eq!(
        render(
            &env,
            "a.txt",
            "{{ s is startingwith('a') }} {{ s is endingwith('b') }} {{ ',' in s }}",
            csv()
        ),
        "True True True"
    );
    // `urlencode` needs a minijinja feature molde doesn't enable
    let err = env
        .template_from_named_str("a.txt", "{{ s | urlencode }}")
        .and_then(|t| t.render(ctx()))
        .expect_err("urlencode must be unknown");
    assert!(format!("{err:#}").contains("unknown filter"), "{err:#}");
    // `{% break %}` / `{% continue %}` need `loop_controls`: a syntax error at parse time
    for stmt in ["break", "continue"] {
        let src = format!("{{% for x in [1] %}}{{% {stmt} %}}{{% endfor %}}");
        let err = env
            .template_from_named_str("a.txt", &src)
            .expect_err("loop controls must be unknown");
        assert!(format!("{err:#}").contains("unknown statement"), "{err:#}");
    }
}

/// A `namespace` changed in a loop body is not seen by that loop's `for ... if` filter:
/// the filter runs over the whole list first. Test inside the body instead.
#[test]
fn a_loop_filter_does_not_see_the_body_s_namespace() {
    let env = env("ns-filter");
    let ctx = || context! { items => vec![1, 2, 3] };
    let filtered = "{% set ns = namespace(n=0) %}{% for i in items if ns.n < 1 %}{{ i }}{% set ns.n = ns.n + 1 %}{% endfor %}";
    assert_eq!(render(&env, "a.txt", filtered, ctx()), "123");
    let inner = "{% set ns = namespace(n=0) %}{% for i in items %}{% if ns.n < 1 %}{{ i }}{% set ns.n = ns.n + 1 %}{% endif %}{% endfor %}";
    assert_eq!(render(&env, "a.txt", inner, ctx()), "1");
}

/// Data into an inline <script>: `tojson` gives a JS value that can't close the script;
/// a quoted `"{{ x }}"` gets HTML escaping, which JavaScript doesn't undo.
#[test]
fn tojson_is_how_data_enters_an_inline_script() {
    let env = env("script");
    let ctx = || context! { x => "</script><b>" };
    assert_eq!(
        render(
            &env,
            "a.html",
            "<script>var x = {{ x | tojson }};</script>",
            ctx()
        ),
        r#"<script>var x = "\u003c/script\u003e\u003cb\u003e";</script>"#
    );
    assert_eq!(
        render(
            &env,
            "a.html",
            "<script>var x = \"{{ x }}\";</script>",
            ctx()
        ),
        "<script>var x = \"&lt;/script&gt;&lt;b&gt;\";</script>"
    );
}
