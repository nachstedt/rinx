use serde_json::json;

use super::test_support::{attribute, convert, entity};
use super::*;

#[test]
fn test_need_type_becomes_an_entity_type_with_a_title_argument() {
    // Given one need type carrying a title and an id prefix
    let needs = json!({"types": [{"directive": "req", "title": "Requirement", "prefix": "R_"}]});

    // When
    let (schema, _) = convert(needs, None);

    // Then the directive name, label and prefix carry over, and the argument
    // fills a `title` attribute
    let req = entity(&schema, "req");
    assert_eq!(req.label.as_deref(), Some("Requirement"));
    assert_eq!(
        req.id.as_ref().and_then(|id| id.prefix.as_deref()),
        Some("R_")
    );
    assert_eq!(
        req.argument
            .as_ref()
            .map(|argument| argument.fields.clone()),
        Some(vec!["title".to_string()])
    );
    assert!(attribute(req, "title").is_some());
}

#[test]
fn test_id_required_is_carried_onto_every_type() {
    // Given a project that forces authors to write an explicit :id:
    let needs = json!({"id_required": true, "types": [{"directive": "req", "prefix": "R_"}, {"directive": "spec"}]});

    // When
    let (schema, _) = convert(needs, None);

    // Then
    assert!(
        schema
            .entity_type
            .iter()
            .all(|entity| entity.id.as_ref().is_some_and(|id| id.required))
    );
}

#[test]
fn test_a_type_without_a_title_falls_back_to_its_directive_name() {
    // Given a need type declaring no title
    let needs = json!({"types": [{"directive": "spec"}]});

    // When
    let (schema, _) = convert(needs, None);

    // Then
    assert_eq!(entity(&schema, "spec").label.as_deref(), Some("spec"));
}

#[test]
fn test_a_type_setting_no_id_rule_has_no_id_table() {
    // Given a need type with an empty prefix and no project-wide rule
    let needs = json!({"types": [{"directive": "spec", "prefix": ""}]});

    // When
    let (schema, _) = convert(needs, None);

    // Then
    assert_eq!(entity(&schema, "spec").id, None);
}

#[test]
fn test_a_name_that_is_both_a_field_and_a_link_becomes_the_relation() {
    // Given `spec` declared as a field and as a link type — the entity model
    // forbids one name being an attribute and a relation on one type
    let needs = json!({
        "types": [{"directive": "test"}],
        "fields": {"spec": {}},
        "links": {"spec": {"outgoing": "specs"}},
    });

    // When
    let (schema, _) = convert(needs, None);

    // Then it appears once, as the relation
    let test = entity(&schema, "test");
    assert!(attribute(test, "spec").is_none());
    assert_eq!(test.relation.iter().filter(|r| r.name == "spec").count(), 1);
}

#[test]
fn test_one_role_per_type_plus_the_sphinx_needs_spelling() {
    // Given two types
    let names = ["req".to_string(), "spec".to_string()];

    // When
    let declared = roles(&names);

    // Then each type gets its own role, and `:need:` resolves to any of them
    let declared: Vec<(&str, Option<Vec<String>>)> = declared
        .iter()
        .map(|role| (role.name.as_str(), role.types.clone()))
        .collect();
    assert_eq!(
        declared,
        [
            ("req", Some(vec!["req".to_string()])),
            ("spec", Some(vec!["spec".to_string()])),
            ("need", Some(vec!["req".to_string(), "spec".to_string()])),
        ]
    );
}

#[test]
fn test_a_type_named_need_is_not_shadowed_by_a_second_need_role() {
    // Given a project that names a type `need`
    let names = ["need".to_string()];

    // When
    let declared = roles(&names);

    // Then only one role of that name is declared — a duplicate is a schema
    // error
    assert_eq!(declared.len(), 1);
    assert_eq!(declared[0].types, Some(vec!["need".to_string()]));
}

#[test]
fn test_import_keys_are_carried_over_verbatim() {
    // Given the table the sphinx-needs demo declares
    let needs = json!({"import_keys": {"imported_project": "/needs_import.json"}});

    // When it is converted
    let (schema, report) = convert(needs, None);

    // Then the value crosses unchanged — sphinx-needs writes it
    // source-root-relative with a leading `/`, which is how this build
    // resolves a path in an entity schema — and it is no loss to report
    assert_eq!(
        schema
            .import_keys
            .get("imported_project")
            .map(String::as_str),
        Some("/needs_import.json")
    );
    assert!(
        report
            .iter()
            .all(|entry| !entry.detail.contains("import_keys"))
    );
}

#[test]
fn test_a_converted_schema_loads() {
    // Given a configuration shaped like the sphinx-needs demo's
    let needs = json!({
        "id_required": true,
        "types": [{"directive": "req", "title": "Requirement", "prefix": "R_"}, {"directive": "test"}],
        "fields": {"effort": {"schema": {"type": "integer", "enum": [1, 2]}}, "status": {"parse_variants": true}},
        "links": {"links": {}, "tests": {"outgoing": "tests", "incoming": "tested by"}},
    });

    // When
    let (schema, _) = convert(needs, None);

    // Then
    let loaded = crate::load_raw_schema(schema, &crate::NoReservedNames).expect("the schema loads");
    assert!(loaded.entity_type("req").is_some());
    assert!(loaded.role("need").is_some());
}

#[test]
fn test_label_for_capitalizes_as_python_does() {
    assert_eq!(label_for("safe_state"), "Safe state");
    assert_eq!(label_for("ASIL"), "Asil");
    assert_eq!(label_for(""), "");
}
