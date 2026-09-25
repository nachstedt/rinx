//! The cross-checks a schema must survive: duplicate and clashing names,
//! references to types and attributes that must exist, and the shadowing rule
//! that keeps a section from hiding a built-in directive.

use super::shape_tests::{load, load_errors};
use super::*;

#[test]
fn test_load_refuses_two_entity_types_with_one_name() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "req"

        [[entity_type]]
        name = "req"
    "#;

    // When
    let errors = load_errors(text);

    // Then
    assert!(matches!(
        errors.as_slice(),
        [SchemaError::DuplicateType { name }] if name == "req"
    ));
}

#[test]
fn test_load_reports_a_repeated_name_once_however_often_it_repeats() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "req"

        [[entity_type]]
        name = "req"

        [[entity_type]]
        name = "req"
    "#;

    // When
    let errors = load_errors(text);

    // Then
    assert_eq!(errors.len(), 1);
}

#[test]
fn test_load_refuses_two_roles_with_one_name() {
    // Given
    let text = r#"
        [[role]]
        name = "need"

        [[role]]
        name = "need"
    "#;

    // When
    let errors = load_errors(text);

    // Then
    assert!(matches!(
        errors.as_slice(),
        [SchemaError::DuplicateRole { name }] if name == "need"
    ));
}

#[test]
fn test_load_refuses_a_duplicate_declaration_of_each_kind() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "req"

          [[entity_type.attribute]]
          name = "status"
          type = "string"

          [[entity_type.attribute]]
          name = "status"
          type = "string"

          [[entity_type.section]]
          name = "notes"

          [[entity_type.section]]
          name = "notes"

          [[entity_type.relation]]
          name = "links"

          [[entity_type.relation]]
          name = "links"
    "#;

    // When
    let errors = load_errors(text);

    // Then — one per kind, each naming which kind it was
    let kinds: Vec<DeclarationKind> = errors
        .iter()
        .filter_map(|error| match error {
            SchemaError::DuplicateDeclaration { kind, .. } => Some(*kind),
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            DeclarationKind::Attribute,
            DeclarationKind::Section,
            DeclarationKind::Relation
        ]
    );
}

#[test]
fn test_load_refuses_an_attribute_and_relation_sharing_an_option_spelling() {
    // Given — `:links:` could not be resolved to one or the other
    let text = r#"
        [[entity_type]]
        name = "req"

          [[entity_type.attribute]]
          name = "links"
          type = "string"

          [[entity_type.relation]]
          name = "links"
    "#;

    // When
    let errors = load_errors(text);

    // Then
    assert!(errors.iter().any(|error| matches!(
        error,
        SchemaError::OptionNameClash { name, .. } if name == "links"
    )));
}

#[test]
fn test_load_allows_a_section_to_share_a_name_with_an_attribute() {
    // Given — options and sub-directives are separate namespaces in the source
    let text = r#"
        [[entity_type]]
        name = "req"

          [[entity_type.attribute]]
          name = "notes"
          type = "string"

          [[entity_type.section]]
          name = "notes"
    "#;

    // When
    let schema = load(text);

    // Then
    let entity_type = schema.entity_type("req").unwrap();
    assert!(entity_type.attribute("notes").is_some());
    assert!(entity_type.section("notes").is_some());
}

#[test]
fn test_load_refuses_a_section_shadowing_a_built_in_directive() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "req"

          [[entity_type.section]]
          name = "note"
    "#;

    // When — a loader that knows `note` is already a directive
    let reserved = |name: &str| name == "note";
    let result = load_schema(text, &reserved);

    // Then
    let SchemaErrors(errors) = result.unwrap_err();
    assert!(matches!(
        errors.as_slice(),
        [SchemaError::SectionShadowsDirective { name, .. }] if name == "note"
    ));
}

#[test]
fn test_load_accepts_a_section_name_no_directive_uses() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "req"

          [[entity_type.section]]
          name = "verification-criteria"
    "#;

    // When
    let reserved = |name: &str| name == "note";
    let schema = load_schema(text, &reserved).unwrap();

    // Then
    assert!(
        schema
            .entity_type("req")
            .unwrap()
            .section("verification-criteria")
            .is_some()
    );
}

#[test]
fn test_load_refuses_a_relation_naming_an_undeclared_target_type() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "req"

          [[entity_type.relation]]
          name = "links"
          to = ["spec"]
    "#;

    // When
    let errors = load_errors(text);

    // Then
    assert!(errors.iter().any(|error| matches!(
        error,
        SchemaError::UnknownType { name, .. } if name == "spec"
    )));
}

#[test]
fn test_load_refuses_a_role_naming_an_undeclared_type() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "req"

        [[role]]
        name = "need"
        types = ["req", "spec"]
    "#;

    // When
    let errors = load_errors(text);

    // Then
    assert!(errors.iter().any(|error| matches!(
        error,
        SchemaError::UnknownType { referenced_by, name } if referenced_by.contains("need") && name == "spec"
    )));
}

#[test]
fn test_load_refuses_an_argument_field_that_is_not_an_attribute() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "req"
        argument = { fields = ["title"] }
    "#;

    // When
    let errors = load_errors(text);

    // Then
    assert!(matches!(
        errors.as_slice(),
        [SchemaError::UnknownArgumentField { field, .. }] if field == "title"
    ));
}

#[test]
fn test_load_refuses_a_whole_argument_mapped_onto_several_fields() {
    // Given — nothing would divide the text between them
    let text = r#"
        [[entity_type]]
        name = "req"
        argument = { fields = ["name", "version"] }

          [[entity_type.attribute]]
          name = "name"
          type = "string"

          [[entity_type.attribute]]
          name = "version"
          type = "string"
    "#;

    // When
    let errors = load_errors(text);

    // Then
    assert!(matches!(
        errors.as_slice(),
        [SchemaError::WholeArgumentNeedsOneField { found: 2, .. }]
    ));
}

#[test]
fn test_load_accepts_several_fields_when_the_argument_is_split() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "audit-event"
        argument = { fields = ["name", "version"], split = "comma" }

          [[entity_type.attribute]]
          name = "name"
          type = "string"

          [[entity_type.attribute]]
          name = "version"
          type = "string"
    "#;

    // When
    let schema = load(text);

    // Then
    assert_eq!(
        schema.entity_type("audit-event").unwrap().argument.fields,
        vec!["name", "version"]
    );
}

#[test]
fn test_load_refuses_an_id_source_that_is_not_an_attribute() {
    // Given
    let text = r#"
        [[entity_type]]
        name = "audit-event"
        id = { from = ["name"] }
    "#;

    // When
    let errors = load_errors(text);

    // Then
    assert!(matches!(
        errors.as_slice(),
        [SchemaError::UnknownIdSource { attribute, .. }] if attribute == "name"
    ));
}

#[test]
fn test_load_refuses_an_id_pattern_that_does_not_compile() {
    // Given — a lookahead, which JSON Schema allows and this build cannot run
    let text = r#"
        [[entity_type]]
        name = "req"
        id = { pattern = "^(?=REQ)" }
    "#;

    // When
    let errors = load_errors(text);

    // Then
    assert!(matches!(
        errors.as_slice(),
        [SchemaError::InvalidPattern { type_name, declared_on, .. }]
            if type_name == "req" && declared_on == "the id"
    ));
}

#[test]
fn test_load_refuses_a_pattern_on_an_attribute_that_is_not_text() {
    // Given — each looks constrained, and nothing would ever check it
    let text = r#"
        [[entity_type]]
        name = "req"

          [[entity_type.attribute]]
          name = "priority"
          type = "int"
          pattern = "^[0-9]$"

          [[entity_type.attribute]]
          name = "status"
          type = "enum"
          values = ["open"]
          pattern = "^o"
    "#;

    // When
    let errors = load_errors(text);

    // Then
    assert_eq!(
        errors,
        [
            SchemaError::PatternOnNonTextType {
                type_name: "req".to_string(),
                attribute: "priority".to_string(),
                value_type: "int".to_string(),
            },
            SchemaError::PatternOnNonTextType {
                type_name: "req".to_string(),
                attribute: "status".to_string(),
                value_type: "enum".to_string(),
            },
        ]
    );
}

#[test]
fn test_load_refuses_an_attribute_that_is_both_required_and_defaulted() {
    // Given — it could never be missing, so one of the two is a mistake
    let text = r#"
        [[entity_type]]
        name = "req"

          [[entity_type.attribute]]
          name = "status"
          type = "string"
          required = true
          default = "open"
    "#;

    // When
    let errors = load_errors(text);

    // Then
    assert!(matches!(
        errors.as_slice(),
        [SchemaError::RequiredAttributeHasDefault { attribute, .. }] if attribute == "status"
    ));
}

#[test]
fn test_load_reports_every_fault_rather_than_stopping_at_the_first() {
    // Given — three independent mistakes
    let text = r#"
        [[entity_type]]
        name = "req"
        id = { from = ["nowhere"] }

          [[entity_type.relation]]
          name = "links"
          to = ["spec"]

        [[role]]
        name = "need"
        types = ["impl"]
    "#;

    // When
    let errors = load_errors(text);

    // Then
    assert!(
        errors.len() >= 3,
        "expected every fault to be reported, got {errors:?}"
    );
}

#[test]
fn test_load_surfaces_a_backlink_fault_from_the_derivation() {
    // Given — two labels for one back-link on a shared target
    let text = r#"
        [[entity_type]]
        name = "req"

          [[entity_type.relation]]
          name = "links"
          to = ["spec"]
          incoming = "linked_by"
          incoming_label = "Linked by"

        [[entity_type]]
        name = "test"

          [[entity_type.relation]]
          name = "verifies"
          to = ["spec"]
          incoming = "linked_by"
          incoming_label = "Verified by"

        [[entity_type]]
        name = "spec"
    "#;

    // When
    let errors = load_errors(text);

    // Then
    assert!(matches!(
        errors.as_slice(),
        [SchemaError::BacklinkLabelConflict { .. }]
    ));
}

#[test]
fn test_collect_duplicates_reports_each_repeated_name_once() {
    // Given
    let names = ["a", "b", "a", "c", "a", "b"];
    let mut errors = Vec::new();

    // When
    collect_duplicates(
        names.into_iter(),
        |name| SchemaError::DuplicateType { name },
        &mut errors,
    );

    // Then
    assert_eq!(errors.len(), 2);
}

#[test]
fn test_check_declared_types_passes_over_an_undeclared_set() {
    // Given — an omitted `to` means "any type", not "no types"
    let declared = vec!["req".to_string()];
    let mut errors = Vec::new();

    // When
    check_declared_types(None, "relation `links`", &declared, &mut errors);

    // Then
    assert!(errors.is_empty());
}

#[test]
fn test_no_reserved_names_reserves_nothing() {
    // Given
    let reserved = NoReservedNames;

    // When / Then
    assert!(!reserved.is_reserved("note"));
    assert!(!reserved.is_reserved("anything"));
}
