//! Reading a schema file: the TOML shape, the value types, and what a
//! well-formed declaration turns into.

use super::*;
use crate::argument::ArgumentSplit;
use crate::attribute::AttributeType;

/// Loads a schema that is expected to be valid, reserving no directive names.
pub(super) fn load(text: &str) -> EntitySchema {
    load_schema(text, &NoReservedNames)
        .unwrap_or_else(|errors| panic!("expected a valid schema, got:\n{errors}"))
}

/// Loads a schema that is expected to be refused.
pub(super) fn load_errors(text: &str) -> Vec<SchemaError> {
    match load_schema(text, &NoReservedNames) {
        Ok(_) => panic!("expected the schema to be refused"),
        Err(SchemaErrors(errors)) => errors,
    }
}

#[test]
fn test_load_reads_an_empty_schema() {
    // Given
    let text = "";

    // When
    let schema = load(text);

    // Then
    assert!(schema.is_empty());
}

#[test]
fn test_load_reads_a_minimal_entity_type() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "req"
    "#;

    // When
    let schema = load(text);

    // Then
    let entity_type = schema.entity_type("req").unwrap();
    assert_eq!(entity_type.name, "req");
    assert!(entity_type.attributes.is_empty());
    assert!(!entity_type.argument.takes_argument());
}

#[test]
fn test_load_reads_every_attribute_type() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "req"

          [[entity_type.attribute]]
          name = "title"
          type = "string"

          [[entity_type.attribute]]
          name = "rationale"
          type = "text"

          [[entity_type.attribute]]
          name = "priority"
          type = "int"

          [[entity_type.attribute]]
          name = "safety"
          type = "bool"

          [[entity_type.attribute]]
          name = "status"
          type = "enum"
          values = ["open", "closed"]

          [[entity_type.attribute]]
          name = "tags"
          type = "list<string>"

          [[entity_type.attribute]]
          name = "phases"
          type = "list<enum>"
          values = ["design", "test"]
    "#;

    // When
    let schema = load(text);

    // Then
    let entity_type = schema.entity_type("req").unwrap();
    let kinds: Vec<&str> = entity_type
        .attributes
        .iter()
        .map(|a| a.value_type.as_str())
        .collect();
    assert_eq!(
        kinds,
        vec![
            "string",
            "text",
            "int",
            "bool",
            "enum",
            "list<string>",
            "list<enum>"
        ]
    );
    assert_eq!(
        entity_type.attribute("status").unwrap().value_type,
        AttributeType::Enum {
            values: vec!["open".to_string(), "closed".to_string()]
        }
    );
}

#[test]
fn test_load_reads_attribute_metadata() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "req"

          [[entity_type.attribute]]
          name = "status"
          label = "Current status"
          type = "string"
          default = "open"
    "#;

    // When
    let schema = load(text);

    // Then
    let attribute = schema
        .entity_type("req")
        .unwrap()
        .attribute("status")
        .unwrap();
    assert_eq!(attribute.display_label(), "Current status");
    assert_eq!(attribute.default.as_deref(), Some("open"));
    assert!(!attribute.required);
}

#[test]
fn test_load_reads_the_two_argument_shapes() {
    // Given — sphinx-needs' title and CPython's comma-separated signature
    let text = r#"
        [[entity_type]]
        name = "req"
        argument = { fields = ["title"] }

          [[entity_type.attribute]]
          name = "title"
          type = "string"

        [[entity_type]]
        name = "audit-event"
        argument = { fields = ["name", "args"], split = "comma" }

          [[entity_type.attribute]]
          name = "name"
          type = "string"

          [[entity_type.attribute]]
          name = "args"
          type = "string"
    "#;

    // When
    let schema = load(text);

    // Then
    let req = schema.entity_type("req").unwrap();
    assert_eq!(req.argument.split, ArgumentSplit::Whole);
    let audit = schema.entity_type("audit-event").unwrap();
    assert_eq!(audit.argument.split, ArgumentSplit::Comma);
    assert_eq!(audit.argument.fields, vec!["name", "args"]);
}

#[test]
fn test_load_reads_an_id_specification() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "req"
        id = { prefix = "REQ_", required = true }
    "#;

    // When
    let schema = load(text);

    // Then
    let id = &schema.entity_type("req").unwrap().id;
    assert_eq!(id.prefix.as_deref(), Some("REQ_"));
    assert!(id.required);
}

#[test]
fn test_load_reads_sections_with_their_cardinality() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "req"

          [[entity_type.section]]
          name = "verification-criteria"
          label = "Verification criteria"
          required = true

          [[entity_type.section]]
          name = "safety-comment"
          multiple = true
    "#;

    // When
    let schema = load(text);

    // Then
    let entity_type = schema.entity_type("req").unwrap();
    let criteria = entity_type.section("verification-criteria").unwrap();
    assert!(criteria.required);
    assert!(!criteria.multiple);
    assert_eq!(criteria.display_label(), "Verification criteria");
    let comment = entity_type.section("safety-comment").unwrap();
    assert!(!comment.required);
    assert!(comment.multiple);
}

#[test]
fn test_load_reads_relations_with_their_cardinality() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "req"

          [[entity_type.relation]]
          name = "links"
          label = "Links to"
          to = ["spec"]
          required = true
          multiple = true
          incoming = "linked_by"
          incoming_label = "Linked by"

        [[entity_type]]
        name = "spec"
    "#;

    // When
    let schema = load(text);

    // Then
    let relation = schema
        .entity_type("req")
        .unwrap()
        .relation("links")
        .unwrap();
    assert!(relation.required);
    assert!(relation.multiple);
    assert_eq!(relation.display_label(), "Links to");
    assert_eq!(relation.incoming_display_label(), Some("Linked by"));
    assert_eq!(schema.backlinks_for("spec").len(), 1);
}

#[test]
fn test_load_reads_top_level_roles() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "req"

        [[entity_type]]
        name = "spec"

        [[role]]
        name = "need"
        types = ["req", "spec"]

        [[role]]
        name = "any-entity"
    "#;

    // When
    let schema = load(text);

    // Then
    let need = schema.role("need").unwrap();
    assert!(need.accepts("req"));
    assert!(!need.accepts("nothing"));
    assert!(schema.role("any-entity").unwrap().accepts("nothing"));
}

#[test]
fn test_load_defaults_every_optional_flag_to_false() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "req"

          [[entity_type.attribute]]
          name = "status"
          type = "string"

          [[entity_type.section]]
          name = "notes"

          [[entity_type.relation]]
          name = "links"
    "#;

    // When
    let schema = load(text);

    // Then
    let entity_type = schema.entity_type("req").unwrap();
    assert!(!entity_type.attribute("status").unwrap().required);
    assert!(!entity_type.section("notes").unwrap().required);
    assert!(!entity_type.section("notes").unwrap().multiple);
    assert!(!entity_type.relation("links").unwrap().required);
    assert!(!entity_type.relation("links").unwrap().multiple);
    assert!(!entity_type.id.required);
}

#[test]
fn test_load_refuses_text_that_is_not_toml() {
    // Given
    let text = "this is not = = toml";

    // When
    let errors = load_errors(text);

    // Then
    assert!(matches!(errors.as_slice(), [SchemaError::Malformed { .. }]));
}

#[test]
fn test_load_refuses_an_unknown_key() {
    // Given — a mistyped key would otherwise be silently ignored
    let text = r#"
        [[entity_type]]
        name = "req"
        lable = "Requirement"
    "#;

    // When
    let errors = load_errors(text);

    // Then
    assert!(matches!(errors.as_slice(), [SchemaError::Malformed { .. }]));
}

#[test]
fn test_load_refuses_an_unknown_attribute_type() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "req"

          [[entity_type.attribute]]
          name = "status"
          type = "colour"
    "#;

    // When
    let errors = load_errors(text);

    // Then — a typed `RawAttributeType` makes this a shape error, so serde
    // refuses it and names the seven valid spellings
    assert!(matches!(
        errors.as_slice(),
        [SchemaError::Malformed { message }] if message.contains("list<string>")
    ));
}

#[test]
fn test_load_refuses_values_on_a_non_enum_attribute() {
    // Given — this looks like it constrains the value, and does not
    let text = r#"
        [[entity_type]]
        name = "req"

          [[entity_type.attribute]]
          name = "status"
          type = "string"
          values = ["open", "closed"]
    "#;

    // When
    let errors = load_errors(text);

    // Then
    assert!(matches!(
        errors.as_slice(),
        [SchemaError::EnumValuesMismatch { attribute, .. }] if attribute == "status"
    ));
}

#[test]
fn test_load_refuses_an_enum_attribute_without_values() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "req"

          [[entity_type.attribute]]
          name = "status"
          type = "enum"
    "#;

    // When
    let errors = load_errors(text);

    // Then
    assert!(matches!(
        errors.as_slice(),
        [SchemaError::EnumValuesMismatch { .. }]
    ));
}

#[test]
fn test_resolve_attribute_type_pairs_each_spelling_with_its_values() {
    // Given / When / Then — an unknown spelling is no longer reachable here:
    // `RawAttributeType` is a typed enum, so deserialization refuses one first
    assert_eq!(
        resolve_attribute_type(RawAttributeType::String, None),
        Ok(AttributeType::String)
    );
    assert_eq!(
        resolve_attribute_type(RawAttributeType::Enum, Some(vec!["a".to_string()])),
        Ok(AttributeType::Enum {
            values: vec!["a".to_string()]
        })
    );
    assert_eq!(
        resolve_attribute_type(RawAttributeType::Enum, None),
        Err(())
    );
    assert_eq!(
        resolve_attribute_type(RawAttributeType::String, Some(vec!["a".to_string()])),
        Err(())
    );
}
