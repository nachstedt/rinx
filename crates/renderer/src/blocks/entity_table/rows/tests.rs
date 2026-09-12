use std::collections::BTreeMap;

use rusty_sphinx_ast::{AttributeValue, EntityId, EntityTable, EntityTableSource};
use rusty_sphinx_entity::{EntitySchema, NoReservedNames, load_schema};
use rusty_sphinx_filter::{FieldName, parse_filter};
use rusty_sphinx_index::{EntityRecord, ProjectIndex};

use super::{Cell, select_rows};

fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "req"
          [[entity_type.attribute]]
          name = "status"
          type = "string"
          [[entity_type.attribute]]
          name = "tags"
          type = "list<string>"

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

fn requirement(status: &str, title: &str, doc: &str) -> EntityRecord {
    EntityRecord {
        type_name: "req".to_string(),
        doc_path: doc.to_string(),
        title: Some(title.to_string()),
        attributes: BTreeMap::from([(
            "status".to_string(),
            AttributeValue::String(status.to_string()),
        )]),
        outgoing: BTreeMap::new(),
        uml: BTreeMap::new(),
    }
}

/// Three requirements and one test, across two documents.
fn index() -> ProjectIndex {
    let mut index = ProjectIndex::default();
    index.entities.insert(
        id("REQ_2"),
        requirement("open", "Boot quickly", "specs/boot"),
    );
    index.entities.insert(
        id("REQ_1"),
        requirement("closed", "Shut down cleanly", "specs/boot"),
    );
    index.entities.insert(
        id("REQ_3"),
        requirement("open", "Survive power loss", "other/power"),
    );
    index.entities.insert(
        id("TEST_1"),
        EntityRecord {
            type_name: "test".to_string(),
            doc_path: "tests/boot".to_string(),
            title: None,
            attributes: BTreeMap::new(),
            outgoing: BTreeMap::from([("verifies".to_string(), vec![id("REQ_1")])]),
            uml: BTreeMap::new(),
        },
    );
    index
}

/// A table over `columns`, selecting with `filter` and ordering by `sort`.
fn table(filter: Option<&str>, columns: &[&str], sort: Option<&str>) -> EntityTable {
    let mut table = EntityTable::new(EntityTableSource::EntityTable);
    table.filter = filter.map(|text| parse_filter(text).expect("expected the filter to parse"));
    table.columns = columns
        .iter()
        .map(|name| FieldName::new(name).unwrap())
        .collect();
    table.sort = sort.map(|name| FieldName::new(name).unwrap());
    table
}

/// The ids `table` selects, in the order it shows them.
fn selected_ids(table: &EntityTable) -> Vec<String> {
    select_rows(table, &index(), &schema())
        .into_iter()
        .map(|(id, _)| id.to_string())
        .collect()
}

/// The cells of the one row `table` selects.
fn single_row(table: &EntityTable) -> Vec<Cell> {
    let rows = select_rows(table, &index(), &schema());
    assert_eq!(rows.len(), 1, "expected exactly one row");
    rows.into_iter().next().unwrap().1.cells
}

#[test]
fn test_a_table_with_no_filter_lists_every_entity() {
    // Given
    let table = table(None, &["id"], None);

    // When
    let ids = selected_ids(&table);

    // Then
    assert_eq!(ids, ["REQ_1", "REQ_2", "REQ_3", "TEST_1"]);
}

#[test]
fn test_rows_are_in_id_order_by_default() {
    // Given — a page is a cached build artefact, so the default order must be
    // deterministic rather than merely consistent within one run
    let table = table(None, &["id"], None);

    // When
    let ids = selected_ids(&table);

    // Then — REQ_1 was inserted second, and still comes first
    assert_eq!(ids[0], "REQ_1");
}

#[test]
fn test_a_filter_selects_by_type() {
    // Given
    let table = table(Some(r#"type == "test""#), &["id"], None);

    // When
    let ids = selected_ids(&table);

    // Then
    assert_eq!(ids, ["TEST_1"]);
}

#[test]
fn test_a_filter_selects_by_document_the_way_the_corpus_does() {
    // Given — the shape every filter in the benchmark corpus is built around
    let table = table(
        Some(r#"docname is not None and "specs" in docname"#),
        &["id"],
        None,
    );

    // When
    let ids = selected_ids(&table);

    // Then
    assert_eq!(ids, ["REQ_1", "REQ_2"]);
}

#[test]
fn test_a_filter_selects_on_an_attribute() {
    // Given
    let table = table(Some(r#"status == "open""#), &["id"], None);

    // When
    let ids = selected_ids(&table);

    // Then
    assert_eq!(ids, ["REQ_2", "REQ_3"]);
}

#[test]
fn test_a_filter_matching_nothing_selects_no_rows() {
    // Given
    let table = table(Some(r#"status == "archived""#), &["id"], None);

    // When
    let ids = selected_ids(&table);

    // Then
    assert!(ids.is_empty());
}

#[test]
fn test_sorting_orders_by_the_named_field() {
    // Given
    let table = table(Some(r#"type == "req""#), &["id"], Some("title"));

    // When
    let ids = selected_ids(&table);

    // Then — by title: Boot, Shut, Survive
    assert_eq!(ids, ["REQ_2", "REQ_1", "REQ_3"]);
}

#[test]
fn test_sorting_keeps_id_order_among_equal_keys() {
    // Given — two requirements share a status
    let table = table(Some(r#"type == "req""#), &["id"], Some("status"));

    // When
    let ids = selected_ids(&table);

    // Then — `closed` first, then the two `open` ones in id order
    assert_eq!(ids, ["REQ_1", "REQ_2", "REQ_3"]);
}

#[test]
fn test_entities_missing_the_sort_field_come_last() {
    // Given — an absent value has no place in an order
    let table = table(None, &["id"], Some("status"));

    // When
    let ids = selected_ids(&table);

    // Then
    assert_eq!(ids.last().unwrap(), "TEST_1");
}

#[test]
fn test_the_id_column_links_and_shows_the_id() {
    // Given — a reader who asked for ids wants to read ids, however
    // well-titled the entity is
    let table = table(Some(r#"id == "REQ_1""#), &["id"], None);

    // When
    let cells = single_row(&table);

    // Then
    assert_eq!(cells, [Cell::IdLink("REQ_1".to_string())]);
}

#[test]
fn test_the_title_column_links_to_the_entity_too() {
    // Given — sphinx-needs' own table links its title column, and a listing
    // whose rows cannot be reached is a report rather than navigation
    let table = table(Some(r#"id == "REQ_1""#), &["title"], None);

    // When
    let cells = single_row(&table);

    // Then
    assert_eq!(cells, [Cell::Links(vec!["REQ_1".to_string()])]);
}

#[test]
fn test_an_attribute_column_is_plain_text() {
    // Given
    let table = table(Some(r#"id == "REQ_1""#), &["status"], None);

    // When
    let cells = single_row(&table);

    // Then
    assert_eq!(cells, [Cell::Text("closed".to_string())]);
}

#[test]
fn test_a_column_the_entity_lacks_renders_empty() {
    // Given
    let table = table(Some(r#"id == "TEST_1""#), &["status"], None);

    // When
    let cells = single_row(&table);

    // Then
    assert_eq!(cells, [Cell::Text(String::new())]);
}

#[test]
fn test_a_relation_column_becomes_links() {
    // Given
    let table = table(Some(r#"id == "TEST_1""#), &["verifies"], None);

    // When
    let cells = single_row(&table);

    // Then
    assert_eq!(cells, [Cell::Links(vec!["REQ_1".to_string()])]);
}

#[test]
fn test_a_backlink_column_becomes_links() {
    // Given
    let mut index = index();
    index.entity_backlinks.insert(
        id("REQ_1"),
        BTreeMap::from([("verified_by".to_string(), vec![id("TEST_1")])]),
    );
    let table = table(Some(r#"id == "REQ_1""#), &["verified_by"], None);

    // When
    let rows = select_rows(&table, &index, &schema());

    // Then
    assert_eq!(rows[0].1.cells, [Cell::Links(vec!["TEST_1".to_string()])]);
}

#[test]
fn test_a_list_of_words_stays_text_rather_than_becoming_dead_links() {
    // Given — `tags` and `verifies` are both lists and only the index can tell
    // them apart, so the decision is made per cell
    let mut index = index();
    index
        .entities
        .get_mut(&id("REQ_1"))
        .unwrap()
        .attributes
        .insert(
            "tags".to_string(),
            AttributeValue::List(vec!["boot".to_string(), "kernel".to_string()]),
        );
    let table = table(Some(r#"id == "REQ_1""#), &["tags"], None);

    // When
    let rows = select_rows(&table, &index, &schema());

    // Then
    assert_eq!(rows[0].1.cells, [Cell::Text("boot, kernel".to_string())]);
}

#[test]
fn test_a_relation_naming_a_missing_entity_stays_text() {
    // Given — the index phase already reported it as `entity.unknown-target`;
    // a dead link in every table that lists it would multiply one fault
    let mut index = index();
    index
        .entities
        .get_mut(&id("TEST_1"))
        .unwrap()
        .outgoing
        .insert("verifies".to_string(), vec![id("REQ_MISSING")]);
    let table = table(Some(r#"id == "TEST_1""#), &["verifies"], None);

    // When
    let rows = select_rows(&table, &index, &schema());

    // Then
    assert_eq!(rows[0].1.cells, [Cell::Text("REQ_MISSING".to_string())]);
}

#[test]
fn test_cells_follow_the_column_order_written() {
    // Given
    let table = table(Some(r#"id == "REQ_1""#), &["status", "docname"], None);

    // When
    let cells = single_row(&table);

    // Then
    assert_eq!(
        cells,
        [
            Cell::Text("closed".to_string()),
            Cell::Text("specs/boot".to_string()),
        ]
    );
}
