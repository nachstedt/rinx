use std::collections::BTreeMap;

use rusty_sphinx_ast::{AttributeValue, EntityId};
use rusty_sphinx_entity::{EntitySchema, NoReservedNames, load_schema};
use rusty_sphinx_filter::{FieldName, FieldValue, FilterSubject};
use rusty_sphinx_index::{EntityRecord, ProjectIndex};

use super::EntitySubject;

fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "req"
        label = "Requirement"
          [[entity_type.attribute]]
          name = "status"
          type = "string"
          [[entity_type.attribute]]
          name = "tags"
          type = "list<string>"
          [[entity_type.attribute]]
          name = "priority"
          type = "int"

        [[entity_type]]
        name = "test"
          [[entity_type.relation]]
          name = "verifies"
          to = ["req"]
          incoming = "verified_by"
        "#,
        &NoReservedNames,
    )
    .expect("the test schema should load")
}

fn id(raw: &str) -> EntityId {
    EntityId::new(raw).unwrap()
}

/// An index holding one requirement and one test verifying it.
fn index() -> ProjectIndex {
    let mut index = ProjectIndex::default();
    index.entities.insert(
        id("REQ_1"),
        EntityRecord {
            type_name: "req".to_string(),
            doc_path: "specs/boot".to_string(),
            title: Some("Boot quickly".to_string()),
            attributes: BTreeMap::from([
                (
                    "status".to_string(),
                    AttributeValue::String("open".to_string()),
                ),
                (
                    "tags".to_string(),
                    AttributeValue::List(vec!["boot".to_string(), "kernel".to_string()]),
                ),
                ("priority".to_string(), AttributeValue::Int(2)),
            ]),
            outgoing: BTreeMap::new(),
        },
    );
    index.entities.insert(
        id("TEST_1"),
        EntityRecord {
            type_name: "test".to_string(),
            doc_path: "tests/boot".to_string(),
            title: None,
            attributes: BTreeMap::new(),
            outgoing: BTreeMap::from([("verifies".to_string(), vec![id("REQ_1")])]),
        },
    );
    index.entity_backlinks.insert(
        id("REQ_1"),
        BTreeMap::from([("verified_by".to_string(), vec![id("TEST_1")])]),
    );
    index
}

/// What `field` is worth on the entity `entity_id`.
fn field_of(entity_id: &str, field: &str) -> FieldValue {
    let index = index();
    let schema = schema();
    let parsed = id(entity_id);
    let record = index.entities.get(&parsed).expect("no such test entity");
    let subject = EntitySubject {
        id: &parsed,
        record,
        index: &index,
        schema: &schema,
    };
    subject.field(&FieldName::new(field).unwrap())
}

fn text(value: &str) -> FieldValue {
    FieldValue::Text(value.to_string())
}

#[test]
fn test_the_id_field_is_the_entitys_own_id() {
    // Given / When
    let value = field_of("REQ_1", "id");

    // Then
    assert_eq!(value, text("REQ_1"));
}

#[test]
fn test_the_type_field_is_the_directive_name() {
    // Given — sphinx-needs' meaning, which is what existing filters compare
    // against: `type == "req"`, never the label
    let value = field_of("REQ_1", "type");

    // Then
    assert_eq!(value, text("req"));
}

#[test]
fn test_the_type_name_field_is_the_schemas_label() {
    // Given — the confusing half of sphinx-needs' pairing, kept deliberately
    let value = field_of("REQ_1", "type_name");

    // Then
    assert_eq!(value, text("Requirement"));
}

#[test]
fn test_the_type_name_falls_back_to_the_directive_name() {
    // Given — a type the schema does not declare a label for
    let value = field_of("TEST_1", "type_name");

    // Then
    assert_eq!(value, text("test"));
}

#[test]
fn test_the_docname_field_is_the_document_path() {
    // Given / When
    let value = field_of("REQ_1", "docname");

    // Then
    assert_eq!(value, text("specs/boot"));
}

#[test]
fn test_the_title_field_is_the_written_title() {
    // Given / When
    let value = field_of("REQ_1", "title");

    // Then
    assert_eq!(value, text("Boot quickly"));
}

#[test]
fn test_an_untitled_entity_has_no_title_rather_than_its_id() {
    // Given — `display_text` falls back to the id so a *link* reads well, but
    // a filter asking `title is None` is asking whether one was written
    let value = field_of("TEST_1", "title");

    // Then
    assert_eq!(value, FieldValue::Missing);
}

#[test]
fn test_an_attribute_keeps_its_declared_type() {
    // Given — a filter comparing `priority == 2` must not meet a string
    let value = field_of("REQ_1", "priority");

    // Then
    assert_eq!(value, FieldValue::Int(2));
}

#[test]
fn test_a_list_attribute_arrives_as_a_list() {
    // Given / When
    let value = field_of("REQ_1", "tags");

    // Then
    assert_eq!(
        value,
        FieldValue::List(vec!["boot".to_string(), "kernel".to_string()])
    );
}

#[test]
fn test_an_outgoing_relation_is_the_list_of_its_targets() {
    // Given / When
    let value = field_of("TEST_1", "verifies");

    // Then
    assert_eq!(value, FieldValue::List(vec!["REQ_1".to_string()]));
}

#[test]
fn test_a_derived_backlink_is_readable_as_a_field() {
    // Given — `verified_by` is declared nowhere; it falls out of the graph
    let value = field_of("REQ_1", "verified_by");

    // Then
    assert_eq!(value, FieldValue::List(vec!["TEST_1".to_string()]));
}

#[test]
fn test_a_field_this_entity_does_not_have_is_missing() {
    // Given — one table may list several types, so a field only some of them
    // declare must simply not match the others
    let value = field_of("TEST_1", "status");

    // Then
    assert_eq!(value, FieldValue::Missing);
}

#[test]
fn test_a_field_no_type_declares_is_missing() {
    // Given — the parser refuses these, so reaching one means a hand-edited
    // `.ast`; it must still not panic
    let value = field_of("REQ_1", "asil");

    // Then
    assert_eq!(value, FieldValue::Missing);
}

#[test]
fn test_every_field_the_schema_declares_resolves_here() {
    // Given — the parser validates a column against the schema's vocabulary,
    // so a name that passes there must resolve here or the column silently
    // renders empty
    let schema = schema();
    let index = index();
    let parsed = id("REQ_1");
    let record = index.entities.get(&parsed).unwrap();
    let subject = EntitySubject {
        id: &parsed,
        record,
        index: &index,
        schema: &schema,
    };

    // When — every declared name, asked of the entity that has them
    let unresolvable: Vec<String> = [
        "id",
        "type",
        "type_name",
        "title",
        "docname",
        "status",
        "tags",
        "priority",
        "verified_by",
    ]
    .iter()
    .filter(|name| subject.field(&FieldName::new(name).unwrap()) == FieldValue::Missing)
    .map(|name| (*name).to_string())
    .collect();

    // Then
    assert!(unresolvable.is_empty(), "did not resolve: {unresolvable:?}");
    assert!(schema.declares_field("verified_by"));
}
