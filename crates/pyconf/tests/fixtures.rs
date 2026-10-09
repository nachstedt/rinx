//! Reading real `conf.py` files, as projects write them (see
//! `testdata/README.md`).

use rinx_pyconf::{BoundValue, ModuleReading, ValueKind, read_module};

const CPYTHON: &str = include_str!("../testdata/cpython-3.14.2-conf.py");
const QUICKSTART: &str = include_str!("../testdata/sphinx-9.1.0-quickstart-conf.py");
const INVENTORY_FIXTURE: &str = include_str!("../../inventory/testdata/sphinx-9.1.0-src/conf.py");

fn literal<'a>(reading: &'a ModuleReading, name: &str) -> &'a ValueKind {
    match &reading.binding(name).expect(name).value {
        BoundValue::Literal(value) => &value.kind,
        BoundValue::Unread(_) => panic!("{name} is unread"),
    }
}

fn strings(value: &ValueKind) -> Vec<&str> {
    let (ValueKind::List(items) | ValueKind::Tuple(items)) = value else {
        panic!("not a sequence: {value:?}");
    };
    items
        .iter()
        .map(|item| item.as_str().expect("a string"))
        .collect()
}

fn is_unread(reading: &ModuleReading, name: &str) -> bool {
    matches!(
        reading.binding(name).map(|binding| &binding.value),
        Some(BoundValue::Unread(_))
    )
}

fn modification_lines(reading: &ModuleReading, name: &str) -> Vec<usize> {
    reading
        .binding(name)
        .expect(name)
        .modifications
        .iter()
        .map(|span| span.start.line)
        .collect()
}

#[test]
fn test_cpythons_conf_py_is_read_to_its_end() {
    // When
    let reading = read_module(CPYTHON);

    // Then
    assert_eq!(reading.error, None);
    assert!(reading.wildcard_imports.is_empty());
}

#[test]
fn test_cpythons_literal_settings_are_read() {
    // When
    let reading = read_module(CPYTHON);

    // Then
    assert_eq!(
        literal(&reading, "root_doc"),
        &ValueKind::Str("contents".into())
    );
    assert_eq!(
        literal(&reading, "highlight_language"),
        &ValueKind::Str("python3".into())
    );
    assert_eq!(
        strings(literal(&reading, "templates_path")),
        ["tools/templates"]
    );
    assert_eq!(
        strings(literal(&reading, "html_static_path")),
        ["_static", "tools/static"]
    );
    assert_eq!(
        literal(&reading, "toc_object_entries"),
        &ValueKind::Bool(false)
    );
}

#[test]
fn test_cpythons_modified_settings_keep_their_literal_and_name_each_change() {
    // When
    let reading = read_module(CPYTHON);

    // Then
    assert_eq!(
        strings(literal(&reading, "exclude_patterns")),
        ["includes/*.rst", "venv/*", "README.rst"]
    );
    assert_eq!(modification_lines(&reading, "exclude_patterns"), [119]);
    assert_eq!(modification_lines(&reading, "extensions"), [51]);
    assert_eq!(modification_lines(&reading, "nitpick_ignore"), [228, 244]);
}

#[test]
fn test_cpythons_computed_settings_are_unread() {
    // When
    let reading = read_module(CPYTHON);

    // Then
    for name in ["version", "release", "rst_epilog", "html_short_title"] {
        assert!(is_unread(&reading, name), "{name}");
    }
}

#[test]
fn test_cpythons_loop_variables_are_deleted_again() {
    // When
    let reading = read_module(CPYTHON);

    // Then — `del role, name` and `del _OPTIONAL_EXTENSIONS`
    for name in ["role", "name", "_OPTIONAL_EXTENSIONS"] {
        assert!(reading.binding(name).is_none(), "{name}");
    }
}

#[test]
fn test_the_quickstart_conf_py_is_all_literals() {
    // When
    let reading = read_module(QUICKSTART);

    // Then
    assert_eq!(reading.error, None);
    for (name, binding) in &reading.bindings {
        assert!(
            matches!(binding.value, BoundValue::Literal(_)),
            "{name} is unread"
        );
        assert!(binding.modifications.is_empty(), "{name} is modified");
    }
    assert_eq!(
        literal(&reading, "project"),
        &ValueKind::Str("Example".into())
    );
    assert_eq!(
        strings(literal(&reading, "exclude_patterns")),
        Vec::<&str>::new()
    );
    assert_eq!(strings(literal(&reading, "templates_path")), ["_templates"]);
}

#[test]
fn test_the_inventory_fixtures_conf_py_is_read() {
    // When
    let reading = read_module(INVENTORY_FIXTURE);

    // Then
    assert_eq!(reading.error, None);
    assert_eq!(
        literal(&reading, "project"),
        &ValueKind::Str("Fixture".into())
    );
}
