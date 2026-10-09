//! Properties over arbitrary input: the reader is fed whatever a project's
//! `conf.py` holds, half-typed included.

use super::read_module;
use proptest::prelude::*;

/// Text made mostly of the characters Python's grammar turns on, so the
/// cases reach strings, brackets and indentation rather than plain names.
fn pythonish() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            Just("'".to_string()),
            Just("\"".to_string()),
            Just("\"\"\"".to_string()),
            Just("f'".to_string()),
            Just("{".to_string()),
            Just("}".to_string()),
            Just("[".to_string()),
            Just("]".to_string()),
            Just("(".to_string()),
            Just(")".to_string()),
            Just("\\".to_string()),
            Just("\n".to_string()),
            Just("\r".to_string()),
            Just("    ".to_string()),
            Just("\t".to_string()),
            Just("#".to_string()),
            Just(":".to_string()),
            Just(",".to_string()),
            Just(" = ".to_string()),
            Just(" += ".to_string()),
            Just("if x:".to_string()),
            Just("def f():".to_string()),
            Just("del ".to_string()),
            Just("from m import *".to_string()),
            Just("name".to_string()),
            Just("1.5e-3".to_string()),
            Just("\\N{".to_string()),
            "\\PC".prop_map(|c: String| c),
        ],
        0..60,
    )
    .prop_map(|parts| parts.concat())
}

proptest! {
    #[test]
    fn test_read_module_never_panics(source in pythonish()) {
        // When / Then — completing is the property
        let _ = read_module(&source);
    }

    #[test]
    fn test_read_module_never_panics_on_any_text(source in "(\\PC|\n|\r|\t){0,200}") {
        // When / Then
        let _ = read_module(&source);
    }

    #[test]
    fn test_read_module_reads_back_any_string_list_it_is_given(
        items in prop::collection::vec("[^'\\\\\n\r]{0,12}", 0..8),
    ) {
        // Given
        let written: Vec<String> = items.iter().map(|item| format!("'{item}'")).collect();
        let source = format!("exclude_patterns = [{}]\n", written.join(", "));

        // When
        let reading = read_module(&source);

        // Then
        let Some(crate::reading::BoundValue::Literal(value)) =
            reading.binding("exclude_patterns").map(|binding| &binding.value)
        else {
            return Err(TestCaseError::fail("not read as a literal"));
        };
        let read: Vec<&str> = value
            .as_sequence()
            .unwrap_or_default()
            .iter()
            .filter_map(crate::value::Value::as_str)
            .collect();
        prop_assert_eq!(read, items.iter().map(String::as_str).collect::<Vec<_>>());
    }
}
