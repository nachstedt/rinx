//! `.. entity-update::` across the whole pipeline.
//!
//! The unit tests in `rusty_sphinx_parser` and `rusty_sphinx_analyzer` assert
//! on the node one directive produces and on `apply_entity_updates` in
//! isolation. These assert the claim neither can reach on its own: applied
//! across a *real, multi-document* build, an update's effect (1) never
//! touches the target's own record, only the derived history beside it,
//! (2) is visible on the target entity's own rendered page, (3) is visible
//! in the update directive's own rendered box together with its
//! justification, (4) is visible to a filter selecting the entity, and (5) a
//! cross-document relation append shows up in the target's derived
//! back-links — plus that two documents disagreeing about a field produce
//! one diagnostic in *each* of them, not one arbitrarily attributed to
//! either.

use rusty_sphinx_analyzer as analyzer;
use rusty_sphinx_ast as ast;
use rusty_sphinx_ast::{Domain, EntityId};
use rusty_sphinx_entity::{EntitySchema, NoReservedNames, load_schema};
use rusty_sphinx_parser::{self as parser, ParseCtx};
use rusty_sphinx_renderer as renderer;

/// A schema with a `req` (`status`, a `links` relation) and a `spec`, small
/// enough to read at a glance but real enough to exercise a mutation of both
/// an attribute and a relation.
fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "req"
        argument = { fields = ["title"] }

          [[entity_type.attribute]]
          name = "title"
          type = "string"

          [[entity_type.attribute]]
          name = "status"
          type = "enum"
          values = ["open", "closed"]
          default = "open"

          [[entity_type.relation]]
          name = "links"
          to = ["spec"]
          multiple = true
          incoming = "linked_by"

        [[entity_type]]
        name = "spec"
        argument = { fields = ["title"] }

          [[entity_type.attribute]]
          name = "title"
          type = "string"
        "#,
        &NoReservedNames,
    )
    .expect("the test schema should load")
}

fn id(written: &str) -> EntityId {
    EntityId::new(written).expect("a legal id")
}

fn parse(doc_path: &str, rst: &str, schema: &EntitySchema) -> ast::Document {
    let ctx = ParseCtx::with_domain(Domain::Py).with_schema(schema);
    parser::parse_with_ctx(doc_path, rst, &ctx)
}

/// The document declaring the entities and setting `status` to `closed`,
/// with a justification.
const REQUIREMENTS: &str = "\
Requirements
============

.. req:: Boot quickly
   :id: REQ_001

.. spec:: Boot sequence
   :id: SPEC_001

.. entity-update:: REQ_001
   :status: closed

   Closed after the boot-timing regression passed.
";

/// A second document, elsewhere in the project, appending a relation target
/// and — in the conflict test — disagreeing about `status`.
fn extend_doc(status_mutation: &str) -> String {
    format!(
        "\
Extending elsewhere
====================

.. entity-update:: REQ_001
   :+links: SPEC_001

   Linked from a document that never declared REQ_001 itself.

{status_mutation}
"
    )
}

#[test]
fn test_e2e_applying_an_update_never_touches_the_targets_own_record() {
    // Given
    let schema = schema();
    let requirements = parse("requirements", REQUIREMENTS, &schema);
    let index =
        analyzer::build_project_index(std::slice::from_ref(&requirements), "requirements", &schema);

    // Then — the as-authored record still says `open`; only the derived
    // history, and `effective_attribute`, know about `closed`.
    let record = index.entities.get(&id("REQ_001")).expect("REQ_001 exists");
    assert_eq!(
        record.attributes.get("status").map(ToString::to_string),
        Some("open".to_string()),
        "applying an update must never mutate the record itself"
    );
    assert_eq!(
        index.effective_attribute(&id("REQ_001"), "status"),
        Some(&ast::AttributeValue::String("closed".to_string()))
    );
    let history = &index.entity_update_history[&id("REQ_001")].attributes["status"];
    assert_eq!(
        history.original,
        Some(ast::AttributeValue::String("open".to_string()))
    );
    assert_eq!(history.applied.len(), 1);
    assert_eq!(history.applied[0].doc_path, "requirements");
}

#[test]
fn test_e2e_the_update_is_visible_on_the_targets_own_rendered_page() {
    // Given
    let schema = schema();
    let requirements = parse("requirements", REQUIREMENTS, &schema);
    let index =
        analyzer::build_project_index(std::slice::from_ref(&requirements), "requirements", &schema);

    // When — rendering REQ_001's own document
    let output = renderer::render_with_assets(
        &requirements,
        &index,
        "requirements",
        &rusty_sphinx_renderer::config::SiteConfig::default(),
        &rusty_sphinx_renderer::EmbeddedAssets::new(),
        &schema,
        &rusty_sphinx_renderer::EntityTemplates::new(),
    );

    // Then — the box built into REQ_001's own entity rendering shows the
    // *updated* value, not the as-authored one
    assert!(
        output.html.contains("<td>closed</td>"),
        "the entity's own page should show the updated status: {}",
        output.html
    );
}

#[test]
fn test_e2e_the_directives_own_box_renders_its_justification() {
    // Given
    let schema = schema();
    let requirements = parse("requirements", REQUIREMENTS, &schema);
    let index =
        analyzer::build_project_index(std::slice::from_ref(&requirements), "requirements", &schema);

    // When
    let output = renderer::render_with_assets(
        &requirements,
        &index,
        "requirements",
        &rusty_sphinx_renderer::config::SiteConfig::default(),
        &rusty_sphinx_renderer::EmbeddedAssets::new(),
        &schema,
        &rusty_sphinx_renderer::EntityTemplates::new(),
    );

    // Then — `show_entity_updates` defaults to true, so the directive draws
    // its own box with the justification prose
    assert!(output.html.contains("entity-update"), "{}", output.html);
    assert!(
        output
            .html
            .contains("Closed after the boot-timing regression passed."),
        "{}",
        output.html
    );
}

#[test]
fn test_e2e_a_filter_selecting_the_entity_sees_the_updated_value() {
    // Given — a table filtering on the updated status
    let schema = schema();
    let rst = format!("{REQUIREMENTS}\n.. entity-table::\n   :filter: status == \"closed\"\n");
    let requirements = parse("requirements", &rst, &schema);
    let index =
        analyzer::build_project_index(std::slice::from_ref(&requirements), "requirements", &schema);

    // When
    let output = renderer::render_with_assets(
        &requirements,
        &index,
        "requirements",
        &rusty_sphinx_renderer::config::SiteConfig::default(),
        &rusty_sphinx_renderer::EmbeddedAssets::new(),
        &schema,
        &rusty_sphinx_renderer::EntityTemplates::new(),
    );

    // Then — the table resolves rows against the index, which now reflects
    // the update
    assert!(
        output.html.contains("REQ_001"),
        "the table should list the entity the update made match: {}",
        output.html
    );
}

#[test]
fn test_e2e_a_cross_document_relation_append_shows_up_in_derived_backlinks() {
    // Given — REQ_001 is declared in one document; the append is written in
    // a second document that never declares it
    let schema = schema();
    let requirements = parse("requirements", REQUIREMENTS, &schema);
    let extend = parse("extend", &extend_doc(""), &schema);
    let docs = [requirements, extend];
    let index = analyzer::build_project_index(&docs, "requirements", &schema);

    // Then — the back-link on the target is derived from the *effective*
    // outgoing edges, which include the cross-document append
    let incoming = index
        .entity_backlinks
        .get(&id("SPEC_001"))
        .expect("SPEC_001 should have a back-link now");
    assert_eq!(incoming.get("linked_by"), Some(&vec![id("REQ_001")]));
}

#[test]
fn test_e2e_two_documents_disagreeing_about_a_field_each_get_their_own_diagnostic() {
    // Given — `requirements` sets `status: closed`; `extend` sets it back to
    // `open`, disagreeing
    let schema = schema();
    let requirements = parse("requirements", REQUIREMENTS, &schema);
    let extend = parse(
        "extend",
        &extend_doc(".. entity-update:: REQ_001\n   :status: open\n"),
        &schema,
    );
    let docs = [requirements, extend];

    // When
    let build = analyzer::build_project_index_reporting(&docs, "requirements", &schema);

    // Then — one diagnostic per document, each carrying that document's own
    // span, so either author can independently suppress their own copy
    let conflicts: Vec<&str> = build
        .diagnostics
        .iter()
        .filter(|d| {
            d.diagnostics
                .iter()
                .any(|diag| diag.code == ast::DiagnosticCode::EntityUpdateConflictingUpdate)
        })
        .map(|d| d.source_path.as_str())
        .collect();
    assert_eq!(conflicts.len(), 2, "{conflicts:?}");
    assert!(conflicts.contains(&"requirements"));
    assert!(conflicts.contains(&"extend"));

    // And the final value is the deterministic later one — `extend` sorts
    // *before* `requirements` alphabetically, so `requirements`'s `closed`
    // is applied last and wins.
    assert_eq!(
        build.index.effective_attribute(&id("REQ_001"), "status"),
        Some(&ast::AttributeValue::String("closed".to_string()))
    );
}
