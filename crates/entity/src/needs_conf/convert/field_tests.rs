use serde_json::json;

use super::*;
use crate::needs_conf::config::{FieldSchema, NeedStatus};

fn field(value: serde_json::Value) -> NeedField {
    serde_json::from_value(value).expect("a field")
}

#[test]
fn test_field_without_a_schema_is_an_untyped_string() {
    // Given
    let declared = field(json!({"nullable": true}));

    // When
    let attribute = attribute_from_field("contact", &declared);

    // Then
    assert_eq!(attribute.label.as_deref(), Some("Contact"));
    assert_eq!(attribute.value_type, RawAttributeType::String);
    assert_eq!(attribute.values, None);
}

#[test]
fn test_integer_boolean_and_array_schemas_map_onto_their_types() {
    // Given fields typed by their JSON-Schema fragment
    let cases = [
        ("integer", RawAttributeType::Int),
        ("number", RawAttributeType::Int),
        ("boolean", RawAttributeType::Bool),
        ("array", RawAttributeType::StringList),
        ("string", RawAttributeType::String),
    ];

    for (json_type, expected) in cases {
        // When
        let attribute =
            attribute_from_field("value", &field(json!({"schema": {"type": json_type}})));

        // Then
        assert_eq!(attribute.value_type, expected, "{json_type}");
    }
}

#[test]
fn test_enum_values_become_strings_even_when_written_as_numbers() {
    // Given sphinx-needs' `effort` field, whose enum holds integers
    let declared = field(json!({
        "description": "Story points",
        "schema": {"type": "integer", "enum": [1, 2, 3, 5]},
    }));

    // When
    let attribute = attribute_from_field("effort", &declared);

    // Then the type is an enum and every value is a string, which is the
    // only spelling the entity model has
    assert_eq!(attribute.value_type, RawAttributeType::Enum);
    assert_eq!(
        attribute.values,
        Some(vec!["1".into(), "2".into(), "3".into(), "5".into()])
    );
}

#[test]
fn test_an_enum_array_becomes_a_list_of_enum() {
    // Given
    let declared = field(json!({"schema": {"type": "array", "enum": ["a", "b"]}}));

    // When
    let attribute = attribute_from_field("kinds", &declared);

    // Then
    assert_eq!(attribute.value_type, RawAttributeType::EnumList);
    assert_eq!(attribute.values, Some(vec!["a".into(), "b".into()]));
}

#[test]
fn test_description_is_preferred_over_a_derived_label() {
    // Given a field that documents itself
    let declared = field(json!({"description": "Automotive Safety Integrity Level"}));

    // When
    let attribute = attribute_from_field("asil", &declared);

    // Then
    assert_eq!(
        attribute.label.as_deref(),
        Some("Automotive Safety Integrity Level")
    );
}

#[test]
fn test_sphinx_needs_builtins_are_declared_even_when_the_project_is_not() {
    // Given a project declaring no fields of its own
    // When
    let fields = global_fields(&NeedsConfig::default());

    // Then the built-in need options are still there — a `:tags:` is
    // sphinx-needs' vocabulary, not an unknown attribute
    assert_eq!(
        fields.get("tags").map(|a| a.value_type),
        Some(RawAttributeType::StringList)
    );
    assert_eq!(
        fields.get("collapse").map(|a| a.value_type),
        Some(RawAttributeType::Bool)
    );
}

#[test]
fn test_a_projects_own_declaration_overrides_the_builtin_in_place() {
    // Given a project that types `status` itself
    let config = NeedsConfig {
        fields: [(
            "status".to_string(),
            NeedField {
                schema: Some(FieldSchema {
                    json_type: Some("boolean".into()),
                    values: None,
                }),
                ..NeedField::default()
            },
        )]
        .into_iter()
        .collect(),
        ..NeedsConfig::default()
    };

    // When
    let fields = global_fields(&config);

    // Then it is retyped, and keeps the built-in's place
    assert_eq!(
        fields.get("status").map(|a| a.value_type),
        Some(RawAttributeType::Bool)
    );
    assert_eq!(fields.iter().next().map(|(name, _)| name), Some("status"));
}

#[test]
fn test_statuses_table_turns_status_into_an_enum() {
    // Given a project listing its allowed statuses
    let config = NeedsConfig {
        statuses: vec![
            NeedStatus {
                name: "open".into(),
            },
            NeedStatus {
                name: "closed".into(),
            },
        ],
        ..NeedsConfig::default()
    };

    // When
    let fields = global_fields(&config);

    // Then
    let status = fields.get("status").expect("declared");
    assert_eq!(status.value_type, RawAttributeType::Enum);
    assert_eq!(status.values, Some(vec!["open".into(), "closed".into()]));
}

#[test]
fn test_id_title_and_type_are_not_attributes() {
    // Given a project declaring fields the entity model handles structurally
    let config: NeedsConfig = serde_json::from_value(json!({
        "fields": {"id": {}, "title": {}, "type": {}, "owner": {}},
    }))
    .expect("a configuration");

    // When
    let fields = global_fields(&config);

    // Then only the ordinary one survives
    assert!(!fields.contains_key("id"));
    assert!(!fields.contains_key("title"));
    assert!(!fields.contains_key("type"));
    assert!(fields.contains_key("owner"));
}

#[test]
fn test_extra_options_are_fields_too() {
    // Given the older list spelling, by name and as a table
    let config: NeedsConfig = serde_json::from_value(json!({
        "extra_options": ["owner", {"name": "effort", "schema": {"type": "integer"}}],
    }))
    .expect("a configuration");

    // When
    let fields = global_fields(&config);

    // Then
    assert_eq!(
        fields.get("owner").map(|a| a.value_type),
        Some(RawAttributeType::String)
    );
    assert_eq!(
        fields.get("effort").map(|a| a.value_type),
        Some(RawAttributeType::Int)
    );
}
