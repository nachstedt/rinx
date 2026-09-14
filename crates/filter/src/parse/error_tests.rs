//! What the parser refuses, and where it says the fault is.
//!
//! Each Python construct below appears in sphinx-needs' own documentation or
//! in the benchmark corpus, so its message is what a migrating author actually
//! reads.

use super::*;

fn error(input: &str) -> FilterError {
    parse_filter(input).expect_err("expected this filter to be refused")
}

#[test]
fn test_an_empty_filter_is_refused_rather_than_matching_everything() {
    // Given — an empty `:filter:` is a mistake, and silently selecting every
    // entity is the degradation these diagnostics exist to catch
    let input = "";

    // When
    let failure = error(input);

    // Then
    assert_eq!(failure.kind, FilterErrorKind::UnexpectedEnd);
}

#[test]
fn test_a_trailing_operator_reports_the_missing_operand() {
    // Given
    let input = "status ==";

    // When
    let failure = error(input);

    // Then
    assert_eq!(failure.kind, FilterErrorKind::UnexpectedEnd);
}

#[test]
fn test_a_dangling_conjunction_reports_the_missing_operand() {
    // Given
    let input = r#"status == "open" and"#;

    // When
    let failure = error(input);

    // Then
    assert_eq!(failure.kind, FilterErrorKind::UnexpectedEnd);
}

#[test]
fn test_an_unclosed_group_is_reported() {
    // Given
    let input = "(a or b";

    // When
    let failure = error(input);

    // Then
    assert!(matches!(failure.kind, FilterErrorKind::UnexpectedToken(_)));
}

#[test]
fn test_a_stray_closing_parenthesis_is_reported_at_its_own_position() {
    // Given
    let input = "a)";

    // When
    let failure = error(input);

    // Then
    assert_eq!(
        failure.kind,
        FilterErrorKind::UnexpectedToken("`)`".to_string())
    );
    assert_eq!(failure.offset, 1);
}

#[test]
fn test_a_function_call_is_reported_at_the_function_name() {
    // Given — `[needs.constraints]` in the benchmark corpus is full of these
    let input = "len(mitigates)";

    // When
    let failure = error(input);

    // Then
    assert_eq!(failure.kind, FilterErrorKind::unsupported("function calls"));
    assert_eq!(failure.offset, 0);
    assert_eq!(failure.length, 3);
}

#[test]
fn test_a_function_call_is_reported_even_inside_a_larger_expression() {
    // Given
    let input = r#"type == "req" and len(mitigates)"#;

    // When
    let failure = error(input);

    // Then
    assert_eq!(failure.kind, FilterErrorKind::unsupported("function calls"));
    assert_eq!(failure.offset, 18);
}

#[test]
fn test_comparing_to_none_with_equals_suggests_is_none() {
    // Given
    let input = "owner == None";

    // When
    let failure = error(input);

    // Then
    assert_eq!(
        failure.kind,
        FilterErrorKind::unsupported_but("comparing to `None`", "use `is None`")
    );
}

#[test]
fn test_comparing_to_none_with_not_equals_suggests_is_not_none() {
    // Given — the mistake a Python author is most likely to make here
    let input = "docname != None";

    // When
    let failure = error(input);

    // Then
    assert_eq!(
        failure.kind,
        FilterErrorKind::unsupported_but("comparing to `None`", "use `is not None`")
    );
    assert_eq!(failure.offset, 11);
}

#[test]
fn test_is_followed_by_anything_but_none_is_refused() {
    // Given — `a is b` is identity in Python, which this language has no
    // notion of
    let input = r#"status is "open""#;

    // When
    let failure = error(input);

    // Then
    assert!(matches!(
        failure.kind,
        FilterErrorKind::Unsupported {
            construct: "`is` comparisons other than to `None`",
            ..
        }
    ));
}

#[test]
fn test_is_none_on_a_constant_is_refused() {
    // Given — answerable but meaningless, so it is not representable
    let input = r#""x" is None"#;

    // When
    let failure = error(input);

    // Then
    assert!(matches!(failure.kind, FilterErrorKind::UnexpectedToken(_)));
}

#[test]
fn test_an_operator_where_an_operand_belongs_names_the_operator() {
    // Given
    let input = "and status";

    // When
    let failure = error(input);

    // Then
    assert_eq!(
        failure.kind,
        FilterErrorKind::UnexpectedToken("`and`".to_string())
    );
    assert_eq!(failure.offset, 0);
}

#[test]
fn test_two_operands_in_a_row_are_reported_at_the_second() {
    // Given
    let input = r#"status "open""#;

    // When
    let failure = error(input);

    // Then
    assert_eq!(
        failure.kind,
        FilterErrorKind::UnexpectedToken("a string".to_string())
    );
    assert_eq!(failure.offset, 7);
}

#[test]
fn test_deeply_nested_parentheses_fail_rather_than_overflow_the_stack() {
    // Given — this parser runs on every keystroke of the live-preview path, so
    // pathological input must degrade rather than abort
    let input = format!("{}a{}", "(".repeat(200), ")".repeat(200));

    // When
    let failure = error(&input);

    // Then
    assert!(matches!(
        failure.kind,
        FilterErrorKind::Unsupported {
            construct: "expressions nested this deeply",
            ..
        }
    ));
}

#[test]
fn test_a_reported_range_always_covers_at_least_one_character() {
    // Given — a caret needs something to point at, wherever the fault is
    let inputs = ["", "status ==", "(a", "a)", "len(x)", "a && b"];

    // When
    let failures: Vec<FilterError> = inputs.iter().map(|input| error(input)).collect();

    // Then
    assert!(failures.iter().all(|failure| failure.length >= 1));
}

#[test]
fn test_a_reported_offset_stays_inside_the_input() {
    // Given
    let inputs = ["status ==", "docname is", "(a or b"];

    // When
    let failures: Vec<(usize, usize)> = inputs
        .iter()
        .map(|input| (error(input).offset, input.chars().count()))
        .collect();

    // Then
    assert!(failures.iter().all(|(offset, length)| *offset < *length));
}

#[test]
fn test_a_bare_attribute_access_is_still_refused_with_the_same_advice() {
    // Given — admitting `.startswith` must not admit `.` in general
    let input = "links.id == 'X'";

    // When
    let failure = error(input);

    // Then
    assert_eq!(
        failure.kind,
        FilterErrorKind::unsupported_but("attribute access", "name the field on its own")
    );
}

#[test]
fn test_an_unsupported_method_is_named_rather_than_called_a_syntax_error() {
    // Given — "function calls are not supported" would not tell the author
    // that `startswith` right beside it would have worked
    let input = r#"id.lower() == "x""#;

    // When
    let failure = error(input);

    // Then
    assert_eq!(
        failure.kind,
        FilterErrorKind::UnsupportedMethod("lower".to_string())
    );
    assert_eq!(
        failure.to_string(),
        "`.lower()` is not supported here; only `startswith` and `endswith` are"
    );
}

#[test]
fn test_an_unsupported_method_is_reported_at_its_own_name() {
    // Given
    let input = r#"id.lower() == "x""#;

    // When
    let failure = error(input);

    // Then — `id.` is three characters, and `lower` is five
    assert_eq!((failure.offset, failure.length), (3, 5));
}

#[test]
fn test_a_tuple_of_prefixes_is_refused_with_the_advice_every_tuple_gets() {
    // Given — Python's own `startswith` accepts a tuple; this language has no
    // list type, and the tokenizer refuses the `,` before the parser runs, so
    // the author reads the same advice every other tuple gets
    let input = r#"id.startswith(("A", "B"))"#;

    // When
    let failure = error(input);

    // Then
    assert_eq!(
        failure.kind,
        FilterErrorKind::unsupported_but("tuples", "combine conditions with `and` or `or`")
    );
}

#[test]
fn test_an_affix_argument_that_is_not_a_string_is_refused() {
    // Given
    let input = "id.startswith(42)";

    // When
    let failure = error(input);

    // Then
    assert_eq!(
        failure.to_string(),
        "unexpected `42` where `.startswith()` takes a string"
    );
}

#[test]
fn test_an_affix_test_on_a_constant_is_refused() {
    // Given — answerable, but meaningless: it says nothing about the entity
    let input = r#""abc".startswith("a")"#;

    // When
    let failure = error(input);

    // Then
    assert_eq!(
        failure.kind,
        FilterErrorKind::UnexpectedToken("a constant before `.startswith()`".to_string())
    );
}

#[test]
fn test_an_unclosed_affix_call_is_refused() {
    // Given
    let input = r#"id.startswith("A""#;

    // When
    let failure = error(input);

    // Then
    assert_eq!(
        failure.kind,
        FilterErrorKind::UnexpectedToken("the end of `.startswith(`".to_string())
    );
}
