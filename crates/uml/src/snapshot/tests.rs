use std::collections::BTreeMap;

use rinx_ast::{AttributeValue, EntityId};
use rinx_index::EntityRecord;

use super::*;

/// A project index holding the entities `records` describes.
pub(crate) fn index_with(records: Vec<(&str, EntityRecord)>) -> ProjectIndex {
    let mut index = ProjectIndex::default();
    for (id, record) in records {
        index
            .entities
            .insert(EntityId::new(id).expect("a valid id"), record);
    }
    index
}

/// A requirement on `doc_path` with `title`.
pub(crate) fn requirement(doc_path: &str, title: Option<&str>) -> EntityRecord {
    EntityRecord {
        type_name: "req".to_string(),
        doc_path: doc_path.to_string(),
        title: title.map(str::to_string),
        attributes: BTreeMap::new(),
        outgoing: BTreeMap::new(),
        uml: BTreeMap::new(),
    }
}

fn snapshot_of(index: &ProjectIndex, doc_path: &str) -> Snapshot {
    Snapshot::build(index, EntitySchema::empty_ref(), doc_path)
}

#[test]
fn test_contains_finds_a_declared_entity() {
    // Given
    let index = index_with(vec![("REQ_001", requirement("reqs.rst", Some("Login")))]);

    // When
    let snapshot = snapshot_of(&index, "index.rst");

    // Then
    assert!(snapshot.contains("REQ_001"));
    assert!(!snapshot.contains("REQ_404"));
}

#[test]
fn test_an_entitys_built_in_fields_are_all_present() {
    // Given
    let index = index_with(vec![("REQ_001", requirement("reqs.rst", Some("Login")))]);
    let snapshot = snapshot_of(&index, "index.rst");

    // When
    let entity = snapshot.entity("REQ_001").expect("the entity exists");

    // Then
    for field in ["id", "type", "type_name", "docname", "title"] {
        assert!(
            entity.get_attr(field).is_ok(),
            "{field} should be readable off a need"
        );
    }
}

#[test]
fn test_an_attribute_is_readable_by_its_own_name() {
    // Given — attributes share the namespace the built-ins live in, as they do
    // in sphinx-needs' own templates
    let mut record = requirement("reqs.rst", Some("Login"));
    record.attributes.insert(
        "status".to_string(),
        AttributeValue::String("open".to_string()),
    );
    let index = index_with(vec![("REQ_001", record)]);
    let snapshot = snapshot_of(&index, "index.rst");

    // When
    let entity = snapshot.entity("REQ_001").expect("the entity exists");

    // Then
    assert_eq!(entity.get_attr("status").unwrap().as_str(), Some("open"));
}

#[test]
fn test_a_built_in_name_wins_over_a_same_named_attribute() {
    // Given — a schema declaring an attribute called `id` must not be able to
    // make `need.id` mean something other than the entity's id
    let mut record = requirement("reqs.rst", Some("Login"));
    record.attributes.insert(
        "id".to_string(),
        AttributeValue::String("not-the-id".to_string()),
    );
    let index = index_with(vec![("REQ_001", record)]);
    let snapshot = snapshot_of(&index, "index.rst");

    // When
    let entity = snapshot.entity("REQ_001").expect("the entity exists");

    // Then
    assert_eq!(entity.get_attr("id").unwrap().as_str(), Some("REQ_001"));
}

#[test]
fn test_a_missing_title_reads_as_none_rather_than_empty() {
    // Given — `{% if need.title %}` must distinguish "not written" from
    // "written empty", the same distinction the filter language's `is None`
    // makes
    let index = index_with(vec![("REQ_001", requirement("reqs.rst", None))]);
    let snapshot = snapshot_of(&index, "index.rst");

    // When
    let entity = snapshot.entity("REQ_001").expect("the entity exists");

    // Then
    assert!(entity.get_attr("title").unwrap().is_none());
}

#[test]
fn test_title_falls_back_to_the_id_for_display() {
    // Given — an entity type that maps no title still needs a readable node
    let index = index_with(vec![("REQ_001", requirement("reqs.rst", None))]);

    // When
    let snapshot = snapshot_of(&index, "index.rst");

    // Then
    assert_eq!(snapshot.title("REQ_001"), "REQ_001");
}

#[test]
fn test_href_is_relative_to_the_page_doing_the_linking() {
    // Given — a diagram drawn on a page two directories below the entity
    let index = index_with(vec![("REQ_001", requirement("reqs.rst", Some("Login")))]);

    // When
    let snapshot = snapshot_of(&index, "team_a/guide/index.rst");

    // Then
    assert_eq!(
        snapshot.href("REQ_001").as_deref(),
        Some("../../reqs.html#entity-REQ_001")
    );
}

#[test]
fn test_href_lands_on_the_anchor_the_renderer_writes() {
    // Given — the whole point of sharing `entity_anchor`: a clickable node
    // must land where a `:ref:` to the same entity would
    let index = index_with(vec![("REQ_001", requirement("reqs.rst", Some("Login")))]);
    let snapshot = snapshot_of(&index, "index.rst");

    // When
    let href = snapshot.href("REQ_001").expect("the entity exists");

    // Then
    assert!(
        href.ends_with(&format!("#{}", rinx_index::entity_anchor("REQ_001"))),
        "{href}"
    );
}

#[test]
fn test_href_is_none_for_an_unknown_entity() {
    // Given
    let index = ProjectIndex::default();

    // When
    let snapshot = snapshot_of(&index, "index.rst");

    // Then
    assert_eq!(snapshot.href("REQ_404"), None);
}

#[test]
fn test_matching_selects_in_id_order() {
    // Given — the order decides the order nodes appear in the generated
    // PlantUML, and therefore the hash
    let index = index_with(vec![
        ("REQ_003", requirement("reqs.rst", Some("Third"))),
        ("REQ_001", requirement("reqs.rst", Some("First"))),
        ("REQ_002", requirement("reqs.rst", Some("Second"))),
    ]);
    let snapshot = snapshot_of(&index, "index.rst");
    let filter = rinx_filter::parse_filter("type == \"req\"").expect("a valid filter");

    // When
    let matched: Vec<&String> = snapshot.matching(&filter);

    // Then
    assert_eq!(matched, ["REQ_001", "REQ_002", "REQ_003"]);
}

#[test]
fn test_matching_uses_the_same_field_meanings_a_table_filter_does() {
    // Given — one entity with a status, one without
    let mut open = requirement("reqs.rst", Some("Open"));
    open.attributes.insert(
        "status".to_string(),
        AttributeValue::String("open".to_string()),
    );
    let index = index_with(vec![
        ("REQ_001", open),
        ("REQ_002", requirement("reqs.rst", Some("No status"))),
    ]);
    let snapshot = snapshot_of(&index, "index.rst");
    let filter = rinx_filter::parse_filter("status == \"open\"").expect("a valid filter");

    // When
    let matched: Vec<&String> = snapshot.matching(&filter);

    // Then — an unset field equals nothing at all, rather than erroring
    assert_eq!(matched, ["REQ_001"]);
}

#[test]
fn test_matches_agrees_with_matching_one_entity_at_a_time() {
    // Given
    let index = index_with(vec![("REQ_001", requirement("reqs.rst", Some("First")))]);
    let snapshot = snapshot_of(&index, "index.rst");
    let selecting = rinx_filter::parse_filter("type == \"req\"").expect("a valid filter");
    let rejecting = rinx_filter::parse_filter("type == \"test\"").expect("a valid filter");

    // When / Then
    assert!(snapshot.matches("REQ_001", &selecting));
    assert!(!snapshot.matches("REQ_001", &rejecting));
}

#[test]
fn test_matches_is_false_for_an_unknown_entity() {
    // Given
    let index = index_with(vec![]);
    let snapshot = snapshot_of(&index, "index.rst");
    let filter = rinx_filter::parse_filter("type == \"req\"").expect("a valid filter");

    // When / Then
    assert!(!snapshot.matches("REQ_404", &filter));
}

#[test]
fn test_all_holds_every_entity_by_id() {
    // Given
    let index = index_with(vec![
        ("REQ_001", requirement("reqs.rst", Some("First"))),
        ("REQ_002", requirement("reqs.rst", Some("Second"))),
    ]);
    let snapshot = snapshot_of(&index, "index.rst");

    // When
    let all = snapshot.all();

    // Then
    assert!(all.get_attr("REQ_001").unwrap().get_attr("title").is_ok());
    assert!(all.get_attr("REQ_002").is_ok());
}

#[test]
fn test_type_name_is_the_declared_type() {
    // Given
    let index = index_with(vec![("REQ_001", requirement("reqs.rst", Some("Login")))]);

    // When
    let snapshot = snapshot_of(&index, "index.rst");

    // Then
    assert_eq!(snapshot.type_name("REQ_001"), "req");
    assert_eq!(snapshot.type_name("REQ_404"), "");
}
