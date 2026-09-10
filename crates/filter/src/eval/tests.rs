use std::collections::BTreeMap;

use super::*;
use crate::parse::parse_filter;

/// A stand-in for whatever the renderer filters — a plain map of fields.
struct TestSubject(BTreeMap<String, FieldValue>);

impl TestSubject {
    fn new(fields: &[(&str, FieldValue)]) -> Self {
        Self(
            fields
                .iter()
                .map(|(name, value)| ((*name).to_string(), value.clone()))
                .collect(),
        )
    }
}

impl FilterSubject for TestSubject {
    fn field(&self, name: &FieldName) -> FieldValue {
        self.0
            .get(name.as_str())
            .cloned()
            .unwrap_or(FieldValue::Missing)
    }
}

fn text(value: &str) -> FieldValue {
    FieldValue::Text(value.to_string())
}

/// Whether `filter` selects a subject carrying `fields`.
fn selects(filter: &str, fields: &[(&str, FieldValue)]) -> bool {
    parse_filter(filter)
        .expect("expected this filter to parse")
        .matches(&TestSubject::new(fields))
}

#[test]
fn test_an_equality_matches_the_named_value() {
    // Given
    let fields = [("status", text("open"))];

    // When
    let matched = selects(r#"status == "open""#, &fields);

    // Then
    assert!(matched);
}

#[test]
fn test_an_equality_rejects_a_different_value() {
    // Given
    let fields = [("status", text("closed"))];

    // When
    let matched = selects(r#"status == "open""#, &fields);

    // Then
    assert!(!matched);
}

#[test]
fn test_an_equality_against_a_missing_field_is_false() {
    // Given — an entity whose type declares no `status` at all
    let fields = [("id", text("REQ_1"))];

    // When
    let matched = selects(r#"status == "open""#, &fields);

    // Then
    assert!(!matched);
}

#[test]
fn test_an_inequality_against_a_missing_field_is_true() {
    // Given — `!=` is the negation of `==`, so a field that equals nothing
    // differs from everything; anything else would make `a != b` and
    // `not (a == b)` disagree
    let fields = [("id", text("REQ_1"))];

    // When
    let matched = selects(r#"status != "open""#, &fields);

    // Then
    assert!(matched);
}

#[test]
fn test_two_missing_fields_are_not_equal_to_each_other() {
    // Given — two entities that both lack an owner are not "the same owner"
    let fields = [("id", text("REQ_1"))];

    // When
    let matched = selects("owner == reviewer", &fields);

    // Then
    assert!(!matched);
}

#[test]
fn test_two_fields_holding_the_same_text_are_equal() {
    // Given
    let fields = [("owner", text("platform")), ("reviewer", text("platform"))];

    // When
    let matched = selects("owner == reviewer", &fields);

    // Then
    assert!(matched);
}

#[test]
fn test_in_is_a_substring_test_over_text() {
    // Given — how every corpus filter selects by document path
    let fields = [("docname", text("safety_example/analysis"))];

    // When
    let matched = selects(r#""safety_example" in docname"#, &fields);

    // Then
    assert!(matched);
}

#[test]
fn test_in_is_a_membership_test_over_a_list() {
    // Given
    let fields = [(
        "tags",
        FieldValue::List(vec!["boot".to_string(), "kernel".to_string()]),
    )];

    // When
    let matched = selects(r#""boot" in tags"#, &fields);

    // Then
    assert!(matched);
}

#[test]
fn test_not_in_inverts_the_containment() {
    // Given
    let fields = [("tags", FieldValue::List(vec!["boot".to_string()]))];

    // When
    let matched = selects(r#""draft" not in tags"#, &fields);

    // Then
    assert!(matched);
}

#[test]
fn test_in_against_a_missing_haystack_is_false() {
    // Given
    let fields = [("id", text("REQ_1"))];

    // When
    let matched = selects(r#""x" in tags"#, &fields);

    // Then
    assert!(!matched);
}

#[test]
fn test_a_field_may_be_the_needle() {
    // Given
    let fields = [
        ("owner", text("boot")),
        ("tags", FieldValue::List(vec!["boot".to_string()])),
    ];

    // When
    let matched = selects("owner in tags", &fields);

    // Then
    assert!(matched);
}

#[test]
fn test_is_none_matches_only_a_missing_field() {
    // Given
    let present = [("owner", text("platform"))];
    let absent: [(&str, FieldValue); 0] = [];

    // When
    let (on_present, on_absent) = (
        selects("owner is None", &present),
        selects("owner is None", &absent),
    );

    // Then
    assert!(!on_present);
    assert!(on_absent);
}

#[test]
fn test_is_not_none_matches_only_a_present_field() {
    // Given — the clause every corpus filter opens with
    let fields = [("docname", text("index"))];

    // When
    let matched = selects("docname is not None", &fields);

    // Then
    assert!(matched);
}

#[test]
fn test_an_empty_string_is_still_present() {
    // Given — `is None` asks whether the field exists, not whether it is empty
    let fields = [("owner", text(""))];

    // When
    let matched = selects("owner is None", &fields);

    // Then
    assert!(!matched);
}

#[test]
fn test_a_bare_field_tests_truthiness() {
    // Given
    let with_tags = [("tags", FieldValue::List(vec!["boot".to_string()]))];
    let without = [("tags", FieldValue::List(Vec::new()))];

    // When
    let (populated, empty) = (selects("tags", &with_tags), selects("tags", &without));

    // Then
    assert!(populated);
    assert!(!empty);
}

#[test]
fn test_and_requires_both_sides() {
    // Given
    let fields = [("status", text("open")), ("owner", text("platform"))];

    // When
    let both = selects(r#"status == "open" and owner == "platform""#, &fields);
    let one = selects(r#"status == "open" and owner == "other""#, &fields);

    // Then
    assert!(both);
    assert!(!one);
}

#[test]
fn test_or_requires_either_side() {
    // Given
    let fields = [("status", text("open"))];

    // When
    let matched = selects(r#"status == "closed" or status == "open""#, &fields);

    // Then
    assert!(matched);
}

#[test]
fn test_not_inverts_its_operand() {
    // Given
    let fields = [("status", text("open"))];

    // When
    let matched = selects(r#"not status == "closed""#, &fields);

    // Then
    assert!(matched);
}

#[test]
fn test_a_grouped_disjunction_is_evaluated_before_the_conjunction() {
    // Given — the corpus' longest filter, and the one that would silently
    // select the wrong entities if precedence were wrong
    let fields = [("type", text("fsr")), ("title", text("Process monitor"))];

    // When
    let matched = selects(
        r#"type == "fsr" and ("Process" in title or "Component" in title)"#,
        &fields,
    );

    // Then
    assert!(matched);
}

#[test]
fn test_that_same_filter_rejects_a_different_type() {
    // Given
    let fields = [("type", text("req")), ("title", text("Process monitor"))];

    // When
    let matched = selects(
        r#"type == "fsr" and ("Process" in title or "Component" in title)"#,
        &fields,
    );

    // Then
    assert!(!matched);
}

#[test]
fn test_integers_and_booleans_compare_by_value() {
    // Given
    let fields = [
        ("count", FieldValue::Int(3)),
        ("automated", FieldValue::Bool(true)),
    ];

    // When
    let (by_number, by_flag) = (
        selects("count == 3", &fields),
        selects("automated == True", &fields),
    );

    // Then
    assert!(by_number);
    assert!(by_flag);
}

#[test]
fn test_a_number_does_not_equal_the_text_of_that_number() {
    // Given — Python does not equate these either
    let fields = [("count", FieldValue::Int(3))];

    // When
    let matched = selects(r#"count == "3""#, &fields);

    // Then
    assert!(!matched);
}

#[test]
fn test_two_lists_are_equal_when_they_hold_the_same_items() {
    // Given
    let fields = [
        ("tags", FieldValue::List(vec!["a".to_string()])),
        ("labels", FieldValue::List(vec!["a".to_string()])),
    ];

    // When
    let matched = selects("tags == labels", &fields);

    // Then
    assert!(matched);
}

#[test]
fn test_a_whole_corpus_filter_selects_the_entity_it_describes() {
    // Given
    let fields = [
        ("type", text("sysreq")),
        ("docname", text("safety_example/system_requirements")),
        ("id", text("SYSREQ_VDC_001")),
    ];

    // When
    let matched = selects(
        r#"type == "sysreq" and docname is not None and "safety_example" in docname and "VDC" in id"#,
        &fields,
    );

    // Then
    assert!(matched);
}
