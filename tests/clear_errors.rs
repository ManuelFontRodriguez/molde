//! Mistakes an author or an agent makes often, and the message that tells them the fix:
//! each is a `config.invalid` or template error in the report that names the folder,
//! route, item or page.
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn site(tag: &str, routes: &str, data: &str, template: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("molde-clearerr-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).unwrap();
    fs::write(dir.join("routes.yaml"), routes).unwrap();
    fs::write(dir.join("data.json"), data).unwrap();
    fs::write(dir.join("templates/i.j2"), template).unwrap();
    dir
}

const BOOKS: &str =
    "routes:\n  - path: /b/{id}/index.html\n    template: i.j2\n    foreach: books\n";
const PAGE: &str =
    "<html><head><title>T {{ item.id }}</title></head><body>{{ item.id }}</body></html>";

/// The single error of the report, as its check id and message.
fn only(report: &Value) -> (String, String) {
    assert_eq!(report["verdict"], "fail", "{report}");
    let errors = report["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "{report}");
    (
        errors[0]["check"].as_str().unwrap().to_string(),
        errors[0]["message"].as_str().unwrap().to_string(),
    )
}

fn check(dir: &Path) -> Value {
    molde::run_check(dir, &dir.join("out")).unwrap()
}

#[test]
fn a_folder_that_is_not_a_site_says_so() {
    let dir = std::env::temp_dir().join(format!("molde-clearerr-empty-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let (check_id, message) = only(&molde::run_check(&dir, &dir.join("out")).unwrap());
    assert_eq!(check_id, "config.invalid");
    assert!(message.contains(&dir.display().to_string()), "{message}");
    assert!(message.contains("not a molde site"), "{message}");
    assert!(message.contains("no routes.yaml"), "{message}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_integer_field_is_a_path_segment() {
    let dir = site("int", BOOKS, r#"{"books":[{"id":42},{"id":7}]}"#, PAGE);
    let report = check(&dir);
    assert_eq!(report["verdict"], "pass", "{report}");
    let current = dir.join("out/current");
    assert!(current.join("b/42/index.html").is_file());
    assert!(current.join("b/7/index.html").is_file());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_float_or_bool_field_is_refused_naming_route_and_item() {
    for (tag, value) in [("float", "1.5"), ("bool", "true"), ("null", "null")] {
        let dir = site(
            tag,
            BOOKS,
            &format!(r#"{{"books":[{{"id":"a"}},{{"id":{value}}}]}}"#),
            PAGE,
        );
        let (id, message) = only(&check(&dir));
        assert_eq!(id, "config.invalid");
        assert!(
            message.contains("route 0 (/b/{id}/index.html), books[1]"),
            "{tag}: {message}"
        );
        assert!(message.contains("'id'"), "{tag}: {message}");
        assert!(
            message.contains("not a string or an integer"),
            "{tag}: {message}"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn a_missing_collection_lists_the_arrays_that_exist() {
    let dir = site(
        "collection",
        BOOKS,
        r#"{"generated":"x","authors":[],"shelves":[]}"#,
        PAGE,
    );
    let (id, message) = only(&check(&dir));
    assert_eq!(id, "config.invalid");
    assert!(
        message.contains("route 0 (/b/{id}/index.html)"),
        "{message}"
    );
    assert!(
        message.contains("foreach 'books' is not in data.json"),
        "{message}"
    );
    assert!(message.contains("authors, shelves"), "{message}");
    assert!(!message.contains("generated"), "{message}");

    fs::write(dir.join("data.json"), r#"{"books":"none"}"#).unwrap();
    let (_, message) = only(&check(&dir));
    assert!(
        message.contains("'books' is in data.json but is not an array"),
        "{message}"
    );

    fs::write(dir.join("data.json"), "[]").unwrap();
    let (_, message) = only(&check(&dir));
    assert!(message.contains("no top-level arrays"), "{message}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_group_by_over_a_missing_collection_names_the_route() {
    let routes = "routes:\n  - path: /g/{genre}/index.html\n    template: i.j2\n    foreach: books\n    group_by: genre\n";
    let dir = site("groupby", routes, r#"{"shelves":[]}"#, PAGE);
    let (id, message) = only(&check(&dir));
    assert_eq!(id, "config.invalid");
    assert!(
        message.contains("group_by (over foreach) 'books'"),
        "{message}"
    );
    assert!(message.contains("shelves"), "{message}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn date_filters_given_a_non_string_name_the_filter_page_and_value() {
    for (filter, tag) in [("date_format", "df"), ("year", "yr")] {
        let dir = site(
            tag,
            BOOKS,
            r#"{"books":[{"id":"a","published":1999}]}"#,
            &format!("<html>{{{{ item.published | {filter} }}}}</html>"),
        );
        fs::write(dir.join("site.yaml"), "locale: en-US\n").unwrap();
        let report = check(&dir);
        let errors = report["errors"].as_array().unwrap();
        assert_eq!(errors.len(), 1, "{report}");
        assert_eq!(errors[0]["check"], "template.render");
        assert_eq!(errors[0]["page"], "b/a/index.html");
        let message = errors[0]["message"].as_str().unwrap();
        assert!(
            message.contains(&format!("{filter}: needs a date string")),
            "{message}"
        );
        assert!(message.contains("got number 1999"), "{message}");
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn a_missing_field_given_to_a_date_filter_says_so() {
    let dir = site(
        "undef",
        BOOKS,
        r#"{"books":[{"id":"a"}]}"#,
        "<html>{{ item.published | date_format }}</html>",
    );
    fs::write(dir.join("site.yaml"), "locale: en-US\n").unwrap();
    let report = check(&dir);
    let message = report["errors"][0]["message"].as_str().unwrap();
    assert!(message.contains("got a missing field"), "{message}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_byte_order_mark_does_not_break_the_site_files() {
    let dir = site("bom", BOOKS, r#"{"books":[{"id":"a"}]}"#, PAGE);
    let bom = |name: &str, text: &str| {
        fs::write(dir.join(name), format!("\u{FEFF}{text}")).unwrap();
    };
    bom("routes.yaml", BOOKS);
    bom("data.json", r#"{"books":[{"id":"a"}]}"#);
    bom("site.yaml", "locale: en-US\n");
    fs::create_dir_all(dir.join("translations")).unwrap();
    bom("translations/en.yaml", "hello: Hello\n");
    let report = check(&dir);
    assert_eq!(report["verdict"], "pass", "{report}");
    let _ = fs::remove_dir_all(&dir);
}
