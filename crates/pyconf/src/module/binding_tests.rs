//! How the reader binds a name at the top level: assignments, their targets
//! and their values.

use super::test_support::*;
use crate::position::Position;
use crate::reading::BoundValue;
use crate::value::ValueKind;

#[test]
fn test_read_module_binds_a_name_to_a_literal() {
    // When
    let reading = read_module("project = 'Python'\nnumfig = True\n");

    // Then
    assert_eq!(literal(&reading, "project"), Some(string("Python")));
    assert_eq!(literal(&reading, "numfig"), Some(ValueKind::Bool(true)));
    assert_eq!(reading.error, None);
}

#[test]
fn test_read_module_records_where_a_name_is_bound() {
    // When
    let reading = read_module("\n\nroot_doc = 'contents'\n");

    // Then
    let binding = reading.binding("root_doc").expect("a binding");
    assert_eq!(binding.target.start, Position { line: 3, column: 1 });
    assert_eq!(binding.target.end, Position { line: 3, column: 9 });
    let BoundValue::Literal(value) = &binding.value else {
        panic!("not a literal");
    };
    assert_eq!(
        value.span.start,
        Position {
            line: 3,
            column: 12
        }
    );
}

#[test]
fn test_read_module_keeps_the_last_top_level_assignment() {
    // When
    let reading = read_module("html_theme = 'alabaster'\nhtml_theme = 'furo'\n");

    // Then
    assert_eq!(literal(&reading, "html_theme"), Some(string("furo")));
}

#[test]
fn test_read_module_forgets_modifications_a_reassignment_replaces() {
    // When
    let reading = read_module("x = []\nx.append(1)\nx = ['a']\n");

    // Then
    assert_eq!(modification_lines(&reading, "x"), Vec::<usize>::new());
}

#[test]
fn test_read_module_binds_a_computed_value_as_unread() {
    // When
    let reading = read_module(
        "import os\nversion = os.getenv('V')\nshort = f'{version}'\nlong = version + '.1'\n",
    );

    // Then
    assert!(is_unread(&reading, "version"));
    assert!(is_unread(&reading, "short"));
    assert!(is_unread(&reading, "long"));
}

#[test]
fn test_read_module_spans_an_unread_value() {
    // When
    let reading = read_module("x = compute(1)\n");

    // Then
    let Some(BoundValue::Unread(span)) = reading.binding("x").map(|binding| &binding.value) else {
        panic!("not unread");
    };
    assert_eq!((span.start.column, span.end.column), (5, 15));
}

#[test]
fn test_read_module_reads_an_annotated_assignment() {
    // When
    let reading = read_module("language: str = 'en'\ntoday: str\n");

    // Then
    assert_eq!(literal(&reading, "language"), Some(string("en")));
    assert!(reading.binding("today").is_none());
}

#[test]
fn test_read_module_binds_every_target_of_a_chained_assignment() {
    // When
    let reading = read_module("a = b = 'x'\n");

    // Then
    assert_eq!(literal(&reading, "a"), Some(string("x")));
    assert_eq!(literal(&reading, "b"), Some(string("x")));
}

#[test]
fn test_read_module_binds_unpacked_names_as_unread() {
    // When
    let reading = read_module("version, release = get_version_info()\n(a, [b, *c]) = 1, (2, 3)\n");

    // Then
    for name in ["version", "release", "a", "b", "c"] {
        assert!(is_unread(&reading, name), "{name}");
    }
}

#[test]
fn test_read_module_does_not_take_a_lambda_default_for_an_assignment() {
    // When
    let reading = read_module("key = lambda item=1: item\n");

    // Then
    assert!(is_unread(&reading, "key"));
    assert!(reading.binding("item").is_none());
}

#[test]
fn test_read_module_ignores_a_keyword_argument() {
    // When
    let reading = read_module("setup(name='x')\n");

    // Then
    assert!(reading.binding("name").is_none());
}

#[test]
fn test_read_module_unbinds_a_name_deleted_at_the_top_level() {
    // When
    let reading = read_module("tmp = 1\nkept = 2\ndel tmp, kept[0]\n");

    // Then
    assert!(reading.binding("tmp").is_none());
    assert_eq!(modification_lines(&reading, "kept"), [3]);
}

#[test]
fn test_read_module_reads_statements_separated_by_semicolons() {
    // When
    let reading = read_module("a = 1; b = 'two'\n");

    // Then
    assert_eq!(literal(&reading, "a"), Some(ValueKind::Int(1)));
    assert_eq!(literal(&reading, "b"), Some(string("two")));
}

#[test]
fn test_read_module_ignores_statements_that_bind_nothing() {
    // When
    let reading =
        read_module("pass\nassert x == 1\nglobal y\nprint(z)\n@decorator\ndef f(): pass\n");

    // Then
    assert_eq!(
        reading.bindings.keys().collect::<Vec<_>>(),
        ["f"],
        "only the function is bound"
    );
}

#[test]
fn test_read_module_keeps_what_it_read_before_a_syntax_error() {
    // When
    let reading = read_module("project = 'P'\nbroken = [1,\nlater = 2\n");

    // Then
    assert_eq!(literal(&reading, "project"), Some(string("P")));
    assert!(reading.binding("broken").is_none());
    assert!(reading.binding("later").is_none());
    assert!(reading.error.is_some());
}

#[test]
fn test_read_module_reads_a_soft_keyword_as_a_name() {
    // When
    let reading = read_module("match = 'x'\ntype = 1\n");

    // Then
    assert_eq!(literal(&reading, "match"), Some(string("x")));
    assert_eq!(literal(&reading, "type"), Some(ValueKind::Int(1)));
}
