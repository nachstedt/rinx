use std::collections::BTreeMap;

use rusty_sphinx_ast::{AttributeValue, EntityId, EntitySection};
use rusty_sphinx_entity::{NoReservedNames, load_schema};

use super::*;

/// A schema where a `req` links to a `spec`, deriving a `linked_by` back-link.
fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "req"

          [[entity_type.relation]]
          name = "links"
          to = ["spec"]
          incoming = "linked_by"

          [[entity_type.relation]]
          name = "notes"
          to = ["spec"]

        [[entity_type]]
        name = "spec"
        "#,
        &NoReservedNames,
    )
    .expect("the test schema should load")
}

fn entity(id: &str, type_name: &str, outgoing: &[(&str, &[&str])]) -> (EntityId, EntityRecord) {
    let record = EntityRecord {
        type_name: type_name.to_string(),
        doc_path: format!("docs/{type_name}"),
        title: None,
        attributes: BTreeMap::new(),
        outgoing: outgoing
            .iter()
            .map(|(relation, targets)| {
                (
                    (*relation).to_string(),
                    targets
                        .iter()
                        .map(|t| EntityId::new(t).unwrap())
                        .collect::<Vec<_>>(),
                )
            })
            .collect(),
    };
    (EntityId::new(id).unwrap(), record)
}

fn index_of(entities: Vec<(EntityId, EntityRecord)>) -> ProjectIndex {
    ProjectIndex {
        entities: entities.into_iter().collect(),
        ..ProjectIndex::default()
    }
}

fn id(raw: &str) -> EntityId {
    EntityId::new(raw).unwrap()
}

// ── index_entity ────────────────────────────────────────────────────

#[test]
fn test_index_entity_records_what_a_reader_of_the_index_needs() {
    // Given
    let body = EntityBody {
        type_name: "req".to_string(),
        id: id("REQ_001"),
        attributes: BTreeMap::from([(
            "title".to_string(),
            AttributeValue::String("Boot quickly".to_string()),
        )]),
        relations: BTreeMap::from([("links".to_string(), vec![id("SPEC_003")])]),
        sections: vec![EntitySection::content(Vec::new())],
        span: None,
    };
    let mut index = ProjectIndex::default();

    // When
    index_entity(&body, "specs/boot", &mut index);

    // Then
    let record = &index.entities[&id("REQ_001")];
    assert_eq!(record.type_name, "req");
    assert_eq!(record.doc_path, "specs/boot");
    assert_eq!(record.title.as_deref(), Some("Boot quickly"));
    assert_eq!(record.targets("links"), [id("SPEC_003")]);
}

#[test]
fn test_index_entity_keeps_no_section_prose() {
    // Given — a listing needs fields, not paragraphs, and the index is bounded
    let body = EntityBody {
        type_name: "req".to_string(),
        id: id("REQ_001"),
        attributes: BTreeMap::new(),
        relations: BTreeMap::new(),
        sections: vec![EntitySection::named(
            "rationale".to_string(),
            vec![rusty_sphinx_ast::Node::Paragraph(vec![
                rusty_sphinx_ast::InlineNode::Text("prose".to_string()),
            ])],
            None,
        )],
        span: None,
    };
    let mut index = ProjectIndex::default();

    // When
    index_entity(&body, "specs/boot", &mut index);

    // Then — the record has no field that could hold it
    let json = serde_json::to_string(&index.entities[&id("REQ_001")]).unwrap();
    assert!(
        !json.contains("prose"),
        "section prose leaked into the index"
    );
}

// ── derive_entity_backlinks ─────────────────────────────────────────

#[test]
fn test_derive_backlinks_points_the_target_back_at_its_source() {
    // Given
    let index = index_of(vec![
        entity("REQ_001", "req", &[("links", &["SPEC_003"])]),
        entity("SPEC_003", "spec", &[]),
    ]);

    // When
    let backlinks = derive_entity_backlinks(&index, &schema());

    // Then
    assert_eq!(backlinks[&id("SPEC_003")]["linked_by"], vec![id("REQ_001")]);
    assert!(!backlinks.contains_key(&id("REQ_001")));
}

#[test]
fn test_derive_backlinks_collects_every_source_of_one_backlink() {
    // Given
    let index = index_of(vec![
        entity("REQ_001", "req", &[("links", &["SPEC_003"])]),
        entity("REQ_002", "req", &[("links", &["SPEC_003"])]),
        entity("SPEC_003", "spec", &[]),
    ]);

    // When
    let backlinks = derive_entity_backlinks(&index, &schema());

    // Then
    assert_eq!(
        backlinks[&id("SPEC_003")]["linked_by"],
        vec![id("REQ_001"), id("REQ_002")]
    );
}

#[test]
fn test_derive_backlinks_lists_a_repeated_source_once() {
    // Given — the same target named twice on one relation
    let index = index_of(vec![
        entity("REQ_001", "req", &[("links", &["SPEC_003", "SPEC_003"])]),
        entity("SPEC_003", "spec", &[]),
    ]);

    // When
    let backlinks = derive_entity_backlinks(&index, &schema());

    // Then
    assert_eq!(backlinks[&id("SPEC_003")]["linked_by"], vec![id("REQ_001")]);
}

#[test]
fn test_derive_backlinks_skips_a_relation_declaring_no_incoming_name() {
    // Given — `notes` is deliberately one-directional
    let index = index_of(vec![
        entity("REQ_001", "req", &[("notes", &["SPEC_003"])]),
        entity("SPEC_003", "spec", &[]),
    ]);

    // When
    let backlinks = derive_entity_backlinks(&index, &schema());

    // Then
    assert!(backlinks.is_empty());
}

#[test]
fn test_derive_backlinks_ignores_an_edge_whose_target_does_not_exist() {
    // Given — naming it is collect_entity_diagnostics' job, not this one's
    let index = index_of(vec![entity("REQ_001", "req", &[("links", &["NOWHERE"])])]);

    // When
    let backlinks = derive_entity_backlinks(&index, &schema());

    // Then
    assert!(backlinks.is_empty());
}

#[test]
fn test_derive_backlinks_ignores_an_entity_of_an_undeclared_type() {
    // Given — a stale index entry from a schema that has since changed
    let index = index_of(vec![
        entity("X_1", "gone", &[("links", &["SPEC_003"])]),
        entity("SPEC_003", "spec", &[]),
    ]);

    // When
    let backlinks = derive_entity_backlinks(&index, &schema());

    // Then
    assert!(backlinks.is_empty());
}

#[test]
fn test_derive_backlinks_is_empty_for_a_project_without_entities() {
    // Given
    let index = ProjectIndex::default();

    // When
    let backlinks = derive_entity_backlinks(&index, &schema());

    // Then
    assert!(backlinks.is_empty());
}

// ── collect_entity_diagnostics ──────────────────────────────────────

#[test]
fn test_collect_diagnostics_reports_a_target_no_document_declares() {
    // Given
    let index = index_of(vec![entity("REQ_001", "req", &[("links", &["NOWHERE"])])]);

    // When
    let reported = collect_entity_diagnostics(&index, &schema());

    // Then — attributed to the document that wrote the source entity
    assert_eq!(reported.len(), 1);
    assert_eq!(reported[0].source_path, "docs/req");
    assert_eq!(
        reported[0].diagnostics[0].code,
        DiagnosticCode::EntityUnknownTarget
    );
    assert!(reported[0].diagnostics[0].message.contains("NOWHERE"));
}

#[test]
fn test_collect_diagnostics_reports_a_target_of_a_disallowed_type() {
    // Given — `links` accepts only a `spec`
    let index = index_of(vec![
        entity("REQ_001", "req", &[("links", &["REQ_002"])]),
        entity("REQ_002", "req", &[]),
    ]);

    // When
    let reported = collect_entity_diagnostics(&index, &schema());

    // Then
    assert_eq!(
        reported[0].diagnostics[0].code,
        DiagnosticCode::EntityDisallowedRelation
    );
    assert!(reported[0].diagnostics[0].message.contains("'req'"));
}

#[test]
fn test_collect_diagnostics_accepts_a_well_formed_graph() {
    // Given
    let index = index_of(vec![
        entity("REQ_001", "req", &[("links", &["SPEC_003"])]),
        entity("SPEC_003", "spec", &[]),
    ]);

    // When
    let reported = collect_entity_diagnostics(&index, &schema());

    // Then
    assert!(reported.is_empty());
}

#[test]
fn test_collect_diagnostics_groups_several_faults_by_document() {
    // Given
    let index = index_of(vec![entity(
        "REQ_001",
        "req",
        &[("links", &["NOWHERE", "ALSO_NOWHERE"])],
    )]);

    // When
    let reported = collect_entity_diagnostics(&index, &schema());

    // Then — one entry per document, both faults inside it
    assert_eq!(reported.len(), 1);
    assert_eq!(reported[0].diagnostics.len(), 2);
}

// ── collect_schema_mismatches ───────────────────────────────────────

#[test]
fn test_collect_schema_mismatches_accepts_documents_parsed_with_this_schema() {
    // Given
    let schema = schema();
    let hash = schema.hash().to_string();
    let documents = [("a", Some(hash.as_str())), ("b", Some(hash.as_str()))];

    // When
    let reported = collect_schema_mismatches(&documents, &schema);

    // Then
    assert!(reported.is_empty());
}

#[test]
fn test_collect_schema_mismatches_reports_a_document_parsed_with_another() {
    // Given
    let schema = schema();
    let hash = schema.hash().to_string();
    let documents = [("a", Some(hash.as_str())), ("b", Some("stale"))];

    // When
    let reported = collect_schema_mismatches(&documents, &schema);

    // Then
    assert_eq!(reported.len(), 1);
    assert_eq!(reported[0].source_path, "b");
    assert_eq!(
        reported[0].diagnostics[0].code,
        DiagnosticCode::EntitySchemaMismatch
    );
}

#[test]
fn test_collect_schema_mismatches_is_silent_about_a_document_with_no_schema() {
    // Given — most libraries in a multi-library site declare no schema because
    // they use no entities; reporting each of them would be noise
    let schema = schema();
    let documents = [("a", None)];

    // When
    let reported = collect_schema_mismatches(&documents, &schema);

    // Then
    assert!(reported.is_empty());
}

#[test]
fn test_collect_schema_mismatches_reports_only_the_document_that_disagrees() {
    // Given — one library uses entities and was parsed against another schema,
    // beside several that use none
    let schema = schema();
    let hash = schema.hash().to_string();
    let documents = [
        ("plain", None),
        ("also-plain", None),
        ("entities", Some(hash.as_str())),
        ("stale", Some("another-schema")),
    ];

    // When
    let reported = collect_schema_mismatches(&documents, &schema);

    // Then
    assert_eq!(reported.len(), 1);
    assert_eq!(reported[0].source_path, "stale");
}

#[test]
fn test_collect_schema_mismatches_is_quiet_when_nobody_uses_entities() {
    // Given — a project that does not use entities records no hash anywhere
    let empty = EntitySchema::empty();
    let documents = [("a", None), ("b", None)];

    // When
    let reported = collect_schema_mismatches(&documents, &empty);

    // Then
    assert!(reported.is_empty());
}

#[test]
fn test_collect_schema_mismatches_reports_a_schema_the_build_does_not_have() {
    // Given — the likelier misconfiguration: the library declares the schema
    // and the site forgot to
    let empty = EntitySchema::empty();
    let documents = [("a", Some("some-hash"))];

    // When
    let reported = collect_schema_mismatches(&documents, &empty);

    // Then
    assert_eq!(reported.len(), 1);
    assert_eq!(
        reported[0].diagnostics[0].code,
        DiagnosticCode::EntitySchemaMismatch
    );
}

// ── entity_diagnostic ───────────────────────────────────────────────

#[test]
fn test_entity_diagnostic_reports_no_position() {
    // Given — the merged index holds no source lines, so none is honest
    let diagnostic = entity_diagnostic(DiagnosticCode::EntityUnknownTarget, "why".to_string());

    // When / Then
    assert_eq!(diagnostic.span, None);
    assert_eq!(diagnostic.message, "why");
}

#[test]
fn test_group_produces_one_entry_per_document() {
    // Given
    let by_document = BTreeMap::from([
        (
            "a".to_string(),
            vec![entity_diagnostic(
                DiagnosticCode::EntityUnknownTarget,
                "one".to_string(),
            )],
        ),
        (
            "b".to_string(),
            vec![entity_diagnostic(
                DiagnosticCode::EntityUnknownTarget,
                "two".to_string(),
            )],
        ),
    ]);

    // When
    let grouped = group(by_document);

    // Then
    assert_eq!(grouped.len(), 2);
    assert_eq!(grouped[0].source_path, "a");
    assert_eq!(grouped[1].source_path, "b");
}
