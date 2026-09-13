use std::collections::BTreeMap;

use rusty_sphinx_ast::{
    AttributeValue, Directive, Document, EntityId, EntityTable, EntityTableSource, Node,
    TableWidths,
};
use rusty_sphinx_entity::{EntitySchema, NoReservedNames, load_schema};
use rusty_sphinx_filter::{FieldName, parse_filter};
use rusty_sphinx_index::{EntityRecord, ProjectIndex};

use crate::{EmbeddedAssets, RenderOutput, blocks::EntityTemplates, config::SiteConfig};

fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "req"
        label = "Requirement"
          [[entity_type.attribute]]
          name = "status"
          label = "Current status"
          type = "string"

        [[entity_type]]
        name = "test"
          [[entity_type.relation]]
          name = "verifies"
          label = "Verifies"
          to = ["req"]
          incoming = "verified_by"
          incoming_label = "Verified by"
        "#,
        &NoReservedNames,
    )
    .expect("the test schema should load")
}

fn id(raw: &str) -> EntityId {
    EntityId::new(raw).unwrap()
}

fn index() -> ProjectIndex {
    let mut index = ProjectIndex::default();
    index.entities.insert(
        id("REQ_1"),
        EntityRecord {
            type_name: "req".to_string(),
            doc_path: "specs/boot".to_string(),
            title: Some("Boot quickly".to_string()),
            attributes: BTreeMap::from([(
                "status".to_string(),
                AttributeValue::String("open".to_string()),
            )]),
            outgoing: BTreeMap::new(),
            uml: BTreeMap::new(),
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
            uml: BTreeMap::new(),
        },
    );
    index.entity_backlinks.insert(
        id("REQ_1"),
        BTreeMap::from([("verified_by".to_string(), vec![id("TEST_1")])]),
    );
    index
}

fn table(columns: &[&str]) -> EntityTable {
    let mut table = EntityTable::new(EntityTableSource::EntityTable);
    table.columns = columns
        .iter()
        .map(|name| FieldName::new(name).unwrap())
        .collect();
    table
}

/// Renders `table` on `doc_path`, through the public entry point — so a test
/// also proves the directive is wired into the node dispatcher.
fn render(table: &EntityTable, doc_path: &str) -> RenderOutput {
    let doc = Document::new(
        doc_path.to_string(),
        vec![Node::Directive(Directive::EntityTable(Box::new(
            table.clone(),
        )))],
    );
    crate::render_with_assets(
        &doc,
        &index(),
        doc_path,
        &SiteConfig::default(),
        &EmbeddedAssets::new(),
        &schema(),
        &EntityTemplates::new(),
    )
}

fn html_of(table: &EntityTable) -> String {
    render(table, "specs/boot").html
}

#[test]
fn test_the_table_carries_one_class_for_both_spellings() {
    // Given — two documents using different names for one directive should not
    // need two stylesheets
    let entity_table = html_of(&table(&["id"]));
    let mut needtable = table(&["id"]);
    needtable.source = EntityTableSource::NeedTable;

    // When
    let other = html_of(&needtable);

    // Then
    assert!(entity_table.contains("<table class=\"entity-table\">"));
    assert_eq!(entity_table, other);
}

#[test]
fn test_a_row_is_rendered_for_every_selected_entity() {
    // Given
    let table = table(&["id"]);

    // When
    let html = html_of(&table);

    // Then
    assert_eq!(html.matches("<tr>").count(), 3); // one header, two entities
}

#[test]
fn test_headings_come_from_the_schemas_declared_labels() {
    // Given — never a second copy of a label held in the renderer
    let table = table(&["status", "verifies", "verified_by"]);

    // When
    let html = html_of(&table);

    // Then
    assert!(html.contains("<th class=\"head\">Current status</th>"));
    assert!(html.contains("<th class=\"head\">Verifies</th>"));
    assert!(html.contains("<th class=\"head\">Verified by</th>"));
}

#[test]
fn test_the_builtin_fields_have_headings_of_their_own() {
    // Given — no schema declares these, so this renderer is the one place
    // they are spelled in prose
    let table = table(&["id", "type", "title", "docname"]);

    // When
    let html = html_of(&table);

    // Then
    assert!(html.contains("<th class=\"head\">ID</th>"));
    assert!(html.contains("<th class=\"head\">Type</th>"));
    assert!(html.contains("<th class=\"head\">Title</th>"));
    assert!(html.contains("<th class=\"head\">Document</th>"));
}

#[test]
fn test_an_id_cell_links_to_the_entity_and_shows_its_id() {
    // Given
    let table = table(&["id"]);

    // When
    let html = html_of(&table);

    // Then — same page, so the href is the anchor's own document
    assert!(html.contains("href=\"boot.html#entity-REQ_1\""));
    assert!(html.contains(">REQ_1</a>"));
}

#[test]
fn test_a_title_cell_links_to_the_entity_and_shows_its_title() {
    // Given
    let table = table(&["title"]);

    // When
    let html = html_of(&table);

    // Then
    assert!(html.contains("href=\"boot.html#entity-REQ_1\""));
    assert!(html.contains(">Boot quickly</a>"));
}

#[test]
fn test_a_title_cell_falls_back_to_the_id_for_an_untitled_entity() {
    // Given — an entity whose type maps no title at all
    let table = table(&["title"]);

    // When
    let html = html_of(&table);

    // Then
    assert!(html.contains(">TEST_1</a>"));
}

#[test]
fn test_a_link_from_another_directory_is_relative_to_this_page() {
    // Given — the href helper is the same one the `:entity:` role uses
    let table = table(&["id"]);

    // When
    let html = render(&table, "guide/index").html;

    // Then
    assert!(html.contains("href=\"../specs/boot.html#entity-REQ_1\""));
}

#[test]
fn test_a_relation_cell_links_to_every_target() {
    // Given
    let table = table(&["verifies"]);

    // When
    let html = html_of(&table);

    // Then
    assert!(html.contains("#entity-REQ_1"));
}

#[test]
fn test_a_text_cell_is_escaped() {
    // Given — a status holding markup characters
    let mut index = index();
    index
        .entities
        .get_mut(&id("REQ_1"))
        .unwrap()
        .attributes
        .insert(
            "status".to_string(),
            AttributeValue::String("<b>open</b>".to_string()),
        );
    let doc = Document::new(
        "specs/boot".to_string(),
        vec![Node::Directive(Directive::EntityTable(Box::new(table(&[
            "status",
        ]))))],
    );

    // When
    let html = crate::render_with_assets(
        &doc,
        &index,
        "specs/boot",
        &SiteConfig::default(),
        &EmbeddedAssets::new(),
        &schema(),
        &EntityTemplates::new(),
    )
    .html;

    // Then
    assert!(html.contains("&lt;b&gt;open&lt;/b&gt;"));
    assert!(!html.contains("<b>open</b>"));
}

#[test]
fn test_an_empty_filter_result_is_reported() {
    // Given — an empty table is far more often a filter that no longer matches
    // than a deliberate statement that there is nothing to show
    let mut table = table(&["id"]);
    table.filter = Some(parse_filter(r#"type == "spec""#).unwrap());

    // When
    let output = render(&table, "specs/boot");

    // Then
    assert_eq!(output.empty_listing_errors.len(), 1);
    assert!(
        output.empty_listing_errors[0]
            .message()
            .contains("no entity")
    );
}

#[test]
fn test_a_table_with_rows_reports_nothing() {
    // Given
    let table = table(&["id"]);

    // When
    let output = render(&table, "specs/boot");

    // Then
    assert!(output.empty_listing_errors.is_empty());
}

#[test]
fn test_an_empty_table_still_renders_its_headings() {
    // Given — the reader should see what the table was asking for
    let mut table = table(&["id", "status"]);
    table.filter = Some(parse_filter(r#"type == "spec""#).unwrap());

    // When
    let html = html_of(&table);

    // Then
    assert!(html.contains("<th class=\"head\">ID</th>"));
    assert!(html.contains("<tbody>\n</tbody>"));
}

#[test]
fn test_the_empty_result_names_the_spelling_the_author_wrote() {
    // Given
    let mut table = table(&["id"]);
    table.source = EntityTableSource::NeedTable;
    table.filter = Some(parse_filter(r#"type == "spec""#).unwrap());

    // When
    let output = render(&table, "specs/boot");

    // Then
    assert!(
        output.empty_listing_errors[0]
            .message()
            .starts_with("needtable:")
    );
}

#[test]
fn test_the_shared_presentation_options_reach_the_markup() {
    // Given
    let mut table = table(&["id", "status"]);
    table.classes = vec!["wide".to_string()];
    table.width = Some("80%".to_string());
    table.widths = Some(TableWidths::Explicit(vec![30, 70]));
    table.name = Some(rusty_sphinx_ast::TargetName::new("every-requirement"));

    // When
    let html = html_of(&table);

    // Then
    assert!(html.contains("<a id=\"every-requirement\"></a>"));
    assert!(html.contains("class=\"entity-table wide\""));
    assert!(html.contains("style=\"width: 80%\""));
    assert!(html.contains("<colgroup>"));
}
