use serde_json::json;

use super::test_support::{attribute, convert, entity, type_rule, value_rules};
use super::*;

#[test]
fn test_narrowing_marks_the_attribute_required_on_that_type_alone() {
    // Given two types and a rule scoped to one of them
    let needs = json!({
        "types": [{"directive": "person"}, {"directive": "team"}],
        "fields": {"role": {}},
    });
    let schemas = type_rule("person-has-role", "person", json!({"required": ["role"]}));

    // When
    let (schema, _) = convert(needs, Some(schemas));

    // Then
    assert!(attribute(entity(&schema, "person"), "role").is_some_and(|a| a.required));
    assert!(attribute(entity(&schema, "team"), "role").is_some_and(|a| !a.required));
}

#[test]
fn test_a_required_link_is_required_on_that_type() {
    // Given
    let needs = json!({"types": [{"directive": "spec"}], "links": {"reqs": {}}});
    let schemas = type_rule("spec-reqs", "spec", json!({"required": ["reqs"]}));

    // When
    let (schema, _) = convert(needs, Some(schemas));

    // Then
    assert!(entity(&schema, "spec").relation[0].required);
}

#[test]
fn test_an_id_pattern_becomes_the_types_id_pattern() {
    // Given the shape of the demo corpus' eleven `*-id-pattern` rules
    let needs = json!({"types": [{"directive": "req", "prefix": "R_"}]});
    let schemas = type_rule(
        "req-id-pattern",
        "req",
        json!({"properties": {"id": {"pattern": "^R_[0-9]+$"}}}),
    );

    // When
    let (schema, report) = convert(needs, Some(schemas));

    // Then the pattern is carried over onto the id, and nothing is left to
    // report
    let id = entity(&schema, "req").id.clone().expect("an id table");
    assert_eq!(id.prefix.as_deref(), Some("R_"));
    assert_eq!(id.pattern.as_deref(), Some("^R_[0-9]+$"));
    assert!(value_rules(&report).is_empty());
}

#[test]
fn test_a_pattern_on_a_string_field_becomes_the_attributes_pattern() {
    // Given
    let needs = json!({"types": [{"directive": "person"}], "fields": {"email": {}}});
    let schemas = type_rule(
        "person-email",
        "person",
        json!({"properties": {"email": {"pattern": "@"}}}),
    );

    // When
    let (schema, report) = convert(needs, Some(schemas));

    // Then
    let email = attribute(entity(&schema, "person"), "email").expect("declared");
    assert_eq!(email.pattern.as_deref(), Some("@"));
    assert!(value_rules(&report).is_empty());
}

#[test]
fn test_an_enum_on_a_string_field_turns_it_into_an_enum() {
    // Given
    let needs = json!({"types": [{"directive": "hazard"}], "fields": {"asil": {}}});
    let schemas = type_rule(
        "hazard-asil",
        "hazard",
        json!({"properties": {"asil": {"enum": ["QM", "A"]}}}),
    );

    // When
    let (schema, report) = convert(needs, Some(schemas));

    // Then
    let asil = attribute(entity(&schema, "hazard"), "asil").expect("declared");
    assert_eq!(asil.value_type, RawAttributeType::Enum);
    assert_eq!(asil.values, Some(vec!["QM".into(), "A".into()]));
    assert!(value_rules(&report).is_empty());
}

#[test]
fn test_the_narrowing_applies_to_the_selected_type_alone() {
    // Given two types and a pattern scoped to one of them
    let needs = json!({
        "types": [{"directive": "person"}, {"directive": "team"}],
        "fields": {"email": {}},
    });
    let schemas = type_rule(
        "person-email",
        "person",
        json!({"properties": {"email": {"pattern": "@"}}}),
    );

    // When
    let (schema, _) = convert(needs, Some(schemas));

    // Then
    assert!(attribute(entity(&schema, "person"), "email").is_some_and(|a| a.pattern.is_some()));
    assert!(attribute(entity(&schema, "team"), "email").is_some_and(|a| a.pattern.is_none()));
}

#[test]
fn test_minimums_of_one_and_empty_fragments_need_nothing_emitted() {
    // Given the demo corpus' `person-has-role`, `team-has-persons`,
    // `impl-has-implements-links` and `test-has-spec-or-impl` shapes
    let needs = json!({
        "types": [{"directive": "person"}],
        "fields": {"role": {}},
        "links": {"persons": {}, "links": {}},
    });
    let schemas = type_rule(
        "person-rules",
        "person",
        json!({
            "properties": {
                "role": {"type": "string", "minLength": 1},
                "persons": {"type": "array", "minItems": 1},
                "links": {},
            },
            "required": ["role"],
        }),
    );

    // When
    let (schema, report) = convert(needs, Some(schemas));

    // Then `required` still applies, and nothing is reported as missing
    assert!(attribute(entity(&schema, "person"), "role").is_some_and(|a| a.required));
    assert!(value_rules(&report).is_empty());
}

#[test]
fn test_a_pattern_on_a_link_is_reported() {
    // Given a pattern on a link, whose values are entity ids
    let needs = json!({"types": [{"directive": "spec"}], "links": {"reqs": {}}});
    let schemas = type_rule(
        "spec-reqs",
        "spec",
        json!({"properties": {"reqs": {"pattern": "^R_"}}}),
    );

    // When
    let (_, report) = convert(needs, Some(schemas));

    // Then
    let rules = value_rules(&report);
    assert_eq!(rules.len(), 1);
    assert!(rules[0].detail.contains("link `reqs`"));
}

#[test]
fn test_a_pattern_on_a_non_text_field_is_reported() {
    // Given a pattern on an integer field
    let needs = json!({
        "types": [{"directive": "req"}],
        "fields": {"effort": {"schema": {"type": "integer"}}},
    });
    let schemas = type_rule(
        "req-effort",
        "req",
        json!({"properties": {"effort": {"pattern": "^[0-9]$"}}}),
    );

    // When
    let (schema, report) = convert(needs, Some(schemas));

    // Then nothing is emitted, and the rule is reported
    assert!(attribute(entity(&schema, "req"), "effort").is_some_and(|a| a.pattern.is_none()));
    let rules = value_rules(&report);
    assert_eq!(rules.len(), 1);
    assert!(rules[0].detail.contains("`int`"));
}

#[test]
fn test_an_enum_on_a_non_text_field_is_reported() {
    // Given an enum on a boolean field
    let needs = json!({
        "types": [{"directive": "req"}],
        "fields": {"approved": {"schema": {"type": "boolean"}}},
    });
    let schemas = type_rule(
        "req-approved",
        "req",
        json!({"properties": {"approved": {"enum": [true]}}}),
    );

    // When
    let (_, report) = convert(needs, Some(schemas));

    // Then
    let rules = value_rules(&report);
    assert_eq!(rules.len(), 1);
    assert!(rules[0].detail.contains("an enum on `approved`"));
}

#[test]
fn test_a_constraint_on_an_undeclared_field_is_reported() {
    // Given a rule naming a field the project never declares
    let needs = json!({"types": [{"directive": "req"}]});
    let schemas = type_rule(
        "req-ghost",
        "req",
        json!({"properties": {"ghost": {"pattern": "."}}}),
    );

    // When
    let (_, report) = convert(needs, Some(schemas));

    // Then
    let rules = value_rules(&report);
    assert_eq!(rules.len(), 1);
    assert!(rules[0].detail.contains("does not declare"));
}
