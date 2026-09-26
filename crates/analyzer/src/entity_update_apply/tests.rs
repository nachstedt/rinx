use std::collections::BTreeMap;

use rinx_ast::{
    AttributeValue, EntityId, EntityUpdate, EntityUpdateSource, FieldMutation, FieldMutationMode,
    Position, Span, UpdateTarget,
};
use rinx_entity::{NoReservedNames, load_schema};
use rinx_index::{EntityRecord, EntityUpdateRecord, ProjectIndex};

use super::*;

/// A schema with one attribute type each way (scalar `status`, list `tags`
/// whose items must be lowercase) and a `multiple` relation to another type.
fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "req"

          [[entity_type.attribute]]
          name = "status"
          type = "enum"
          values = ["open", "closed"]

          [[entity_type.attribute]]
          name = "tags"
          type = "list<string>"
          pattern = "^[a-z-]+$"

          [[entity_type.relation]]
          name = "links"
          to = ["spec"]
          multiple = true

        [[entity_type]]
        name = "spec"
        "#,
        &NoReservedNames,
    )
    .expect("the test schema should load")
}

fn id(raw: &str) -> EntityId {
    EntityId::new(raw).unwrap()
}

fn requirement(status: &str, tags: &[&str], links: &[&str]) -> EntityRecord {
    EntityRecord {
        type_name: "req".to_string(),
        doc_path: "specs/boot.rst".to_string(),
        title: None,
        attributes: BTreeMap::from([
            (
                "status".to_string(),
                AttributeValue::String(status.to_string()),
            ),
            (
                "tags".to_string(),
                AttributeValue::List(tags.iter().map(ToString::to_string).collect()),
            ),
        ]),
        outgoing: BTreeMap::from([("links".to_string(), links.iter().map(|t| id(t)).collect())]),
        uml: BTreeMap::new(),
    }
}

fn span_at(line: u32) -> Span {
    Span {
        start: Position { line, column: 1 },
        end: Position { line, column: 1 },
        file: None,
    }
}

fn update_record(
    doc_path: &str,
    line: u32,
    target_raw: &str,
    fields: Vec<(&str, FieldMutationMode)>,
    strict: bool,
) -> EntityUpdateRecord {
    let candidate_id = EntityId::new(target_raw).ok();
    let filter = rinx_filter::parse_filter(target_raw).ok();
    let mut update = EntityUpdate::new(
        EntityUpdateSource::EntityUpdate,
        UpdateTarget {
            candidate_id,
            filter,
            raw: target_raw.to_string(),
        },
    );
    update.strict = strict;
    update.span = Some(span_at(line));
    update.fields = fields
        .into_iter()
        .map(|(field, mode)| FieldMutation {
            field: field.to_string(),
            mode,
            span: Some(span_at(line)),
        })
        .collect();
    EntityUpdateRecord {
        doc_path: doc_path.to_string(),
        update,
    }
}

fn codes(diagnostics: &[DocumentDiagnostics]) -> Vec<DiagnosticCode> {
    diagnostics
        .iter()
        .flat_map(|d| d.diagnostics.iter().map(|d| d.code))
        .collect()
}

// ── id-vs-filter target resolution ──────────────────────────────────────

#[test]
fn test_apply_entity_updates_targets_an_existing_id_over_a_filter_reading_of_the_same_text() {
    // Given — `REQ_001` both names a real entity and parses as
    // `Truthy(Field("REQ_001"))`
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_001"), requirement("open", &[], &[]));
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_001",
        vec![("status", FieldMutationMode::Set("closed".to_string()))],
        true,
    ));

    // When
    let diagnostics = apply_entity_updates(&mut index, &schema());

    // Then
    assert!(diagnostics.is_empty());
    assert_eq!(
        index.effective_attribute(&id("REQ_001"), "status"),
        Some(&AttributeValue::String("closed".to_string()))
    );
}

#[test]
fn test_apply_entity_updates_falls_back_to_filter_when_the_candidate_id_does_not_exist() {
    // Given — no entity named `REQ_999`, so the filter reading (which
    // selects nothing, since `REQ_999` is not a declared field) applies
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_001"), requirement("open", &[], &[]));
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_999",
        vec![("status", FieldMutationMode::Set("closed".to_string()))],
        false,
    ));

    // When
    apply_entity_updates(&mut index, &schema());

    // Then — REQ_001 was never touched
    assert_eq!(
        index.effective_attribute(&id("REQ_001"), "status"),
        Some(&AttributeValue::String("open".to_string()))
    );
}

#[test]
fn test_apply_entity_updates_reports_a_suppressible_span_for_a_strict_zero_match() {
    // Given
    let mut index = ProjectIndex::default();
    index.entity_updates.push(update_record(
        "a.rst",
        4,
        "type == \"does-not-exist\"",
        vec![],
        true,
    ));

    // When
    let diagnostics = apply_entity_updates(&mut index, &schema());

    // Then
    assert_eq!(
        codes(&diagnostics),
        vec![DiagnosticCode::EntityUpdateEmptyResult]
    );
    let entry = &diagnostics[0].diagnostics[0];
    assert_eq!(entry.span.unwrap().start.line, 4);
    assert_eq!(diagnostics[0].source_path, "a.rst");
}

#[test]
fn test_apply_entity_updates_is_silent_when_strict_is_false() {
    // Given
    let mut index = ProjectIndex::default();
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "type == \"does-not-exist\"",
        vec![],
        false,
    ));

    // When
    let diagnostics = apply_entity_updates(&mut index, &schema());

    // Then
    assert!(diagnostics.is_empty());
}

// ── field mutation modes, on an attribute ───────────────────────────────

#[test]
fn test_set_overwrites_a_scalar_attribute() {
    // Given
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_001"), requirement("open", &[], &[]));
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_001",
        vec![("status", FieldMutationMode::Set("closed".to_string()))],
        true,
    ));

    // When
    apply_entity_updates(&mut index, &schema());

    // Then
    assert_eq!(
        index.effective_attribute(&id("REQ_001"), "status"),
        Some(&AttributeValue::String("closed".to_string()))
    );
}

#[test]
fn test_append_adds_to_a_list_attribute_without_duplicating() {
    // Given
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_001"), requirement("open", &["boot"], &[]));
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_001",
        vec![(
            "tags",
            FieldMutationMode::Append("boot, safety-critical".to_string()),
        )],
        true,
    ));

    // When
    apply_entity_updates(&mut index, &schema());

    // Then — `boot` was already present and is not duplicated
    assert_eq!(
        index.effective_attribute(&id("REQ_001"), "tags"),
        Some(&AttributeValue::List(vec![
            "boot".to_string(),
            "safety-critical".to_string()
        ]))
    );
}

#[test]
fn test_remove_drops_one_item_from_a_list_attribute() {
    // Given
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_001"), requirement("open", &["boot", "kernel"], &[]));
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_001",
        vec![("tags", FieldMutationMode::Remove("boot".to_string()))],
        true,
    ));

    // When
    apply_entity_updates(&mut index, &schema());

    // Then
    assert_eq!(
        index.effective_attribute(&id("REQ_001"), "tags"),
        Some(&AttributeValue::List(vec!["kernel".to_string()]))
    );
}

#[test]
fn test_clear_empties_an_attribute_entirely() {
    // Given
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_001"), requirement("open", &["boot"], &[]));
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_001",
        vec![("tags", FieldMutationMode::Clear)],
        true,
    ));

    // When
    apply_entity_updates(&mut index, &schema());

    // Then
    assert_eq!(index.effective_attribute(&id("REQ_001"), "tags"), None);
}

#[test]
fn test_append_or_remove_on_a_scalar_attribute_is_reported_and_ignored() {
    // Given
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_001"), requirement("open", &[], &[]));
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_001",
        vec![("status", FieldMutationMode::Append("closed".to_string()))],
        true,
    ));

    // When
    let diagnostics = apply_entity_updates(&mut index, &schema());

    // Then
    assert_eq!(
        codes(&diagnostics),
        vec![DiagnosticCode::EntityUpdateListOperationOnScalar]
    );
    assert_eq!(
        index.effective_attribute(&id("REQ_001"), "status"),
        Some(&AttributeValue::String("open".to_string()))
    );
}

#[test]
fn test_a_field_no_type_declares_is_reported_field_not_applicable() {
    // Given
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_001"), requirement("open", &[], &[]));
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_001",
        vec![("bogus", FieldMutationMode::Set("x".to_string()))],
        true,
    ));

    // When
    let diagnostics = apply_entity_updates(&mut index, &schema());

    // Then
    assert_eq!(
        codes(&diagnostics),
        vec![DiagnosticCode::EntityUpdateFieldNotApplicable]
    );
}

#[test]
fn test_an_out_of_range_enum_value_is_reported_invalid_value() {
    // Given
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_001"), requirement("open", &[], &[]));
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_001",
        vec![("status", FieldMutationMode::Set("pending".to_string()))],
        true,
    ));

    // When
    let diagnostics = apply_entity_updates(&mut index, &schema());

    // Then
    assert_eq!(
        codes(&diagnostics),
        vec![DiagnosticCode::EntityUpdateInvalidValue]
    );
}

#[test]
fn test_appending_an_item_outside_the_attributes_pattern_is_reported_and_not_applied() {
    // Given — the pattern is part of the attribute's type, so an update is
    // held to it exactly as the entity's own `:tags:` line was
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_001"), requirement("open", &["boot"], &[]));
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_001",
        vec![("tags", FieldMutationMode::Append("Kernel".to_string()))],
        true,
    ));

    // When
    let diagnostics = apply_entity_updates(&mut index, &schema());

    // Then
    assert_eq!(
        codes(&diagnostics),
        vec![DiagnosticCode::EntityUpdateInvalidValue]
    );
    assert_eq!(
        index.effective_attribute(&id("REQ_001"), "tags"),
        Some(&AttributeValue::List(vec!["boot".to_string()]))
    );
}

// ── field mutation modes, on a relation ─────────────────────────────────

#[test]
fn test_append_adds_a_relation_target() {
    // Given
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_001"), requirement("open", &[], &["SPEC_001"]));
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_001",
        vec![("links", FieldMutationMode::Append("SPEC_002".to_string()))],
        true,
    ));

    // When
    apply_entity_updates(&mut index, &schema());

    // Then
    assert_eq!(
        index.effective_relation_targets(&id("REQ_001"), "links"),
        [id("SPEC_001"), id("SPEC_002")]
    );
}

#[test]
fn test_remove_drops_a_relation_target() {
    // Given
    let mut index = ProjectIndex::default();
    index.entities.insert(
        id("REQ_001"),
        requirement("open", &[], &["SPEC_001", "SPEC_002"]),
    );
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_001",
        vec![("links", FieldMutationMode::Remove("SPEC_001".to_string()))],
        true,
    ));

    // When
    apply_entity_updates(&mut index, &schema());

    // Then
    assert_eq!(
        index.effective_relation_targets(&id("REQ_001"), "links"),
        [id("SPEC_002")]
    );
}

#[test]
fn test_a_relation_appended_beyond_its_cardinality_reuses_the_existing_diagnostic_code() {
    // Given — `links` was declared `multiple = true` in the schema above, so
    // use a non-multiple relation to exercise the cardinality check: reuse
    // `links` but load a schema variant where it takes exactly one.
    let single_target_schema = load_schema(
        r#"
        [[entity_type]]
        name = "req"
          [[entity_type.relation]]
          name = "links"
          to = ["spec"]
          multiple = false
        [[entity_type]]
        name = "spec"
        "#,
        &NoReservedNames,
    )
    .unwrap();
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_001"), requirement("open", &[], &["SPEC_001"]));
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_001",
        vec![("links", FieldMutationMode::Append("SPEC_002".to_string()))],
        true,
    ));

    // When
    let diagnostics = apply_entity_updates(&mut index, &single_target_schema);

    // Then
    assert_eq!(
        codes(&diagnostics),
        vec![DiagnosticCode::EntityMultipleRelationTargets]
    );
}

// ── deterministic ordering & sequential visibility ──────────────────────

#[test]
fn test_apply_entity_updates_applies_in_doc_path_then_span_order() {
    // Given — inserted out of order; alphabetical doc_path then line decides
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_001"), requirement("open", &[], &[]));
    index.entity_updates.push(update_record(
        "b.rst",
        1,
        "REQ_001",
        vec![("status", FieldMutationMode::Set("closed".to_string()))],
        true,
    ));
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_001",
        vec![("status", FieldMutationMode::Set("open".to_string()))],
        true,
    ));

    // When
    apply_entity_updates(&mut index, &schema());

    // Then — a.rst applies first (Set open), then b.rst (Set closed): the
    // final value is closed, and a.rst's Set is recorded as the earlier one
    assert_eq!(
        index.effective_attribute(&id("REQ_001"), "status"),
        Some(&AttributeValue::String("closed".to_string()))
    );
    assert_eq!(index.entity_updates[0].doc_path, "a.rst");
    assert_eq!(index.entity_updates[1].doc_path, "b.rst");
}

#[test]
fn test_a_later_updates_filter_sees_an_earlier_updates_effect_within_the_same_run() {
    // Given — the first update sets status to closed; the second, applied
    // after it, filters on status == "closed" and should see REQ_001
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_001"), requirement("open", &[], &[]));
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_001",
        vec![("status", FieldMutationMode::Set("closed".to_string()))],
        true,
    ));
    index.entity_updates.push(update_record(
        "a.rst",
        2,
        "status == \"closed\"",
        vec![("tags", FieldMutationMode::Append("reviewed".to_string()))],
        true,
    ));

    // When
    let diagnostics = apply_entity_updates(&mut index, &schema());

    // Then
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(
        index.effective_attribute(&id("REQ_001"), "tags"),
        Some(&AttributeValue::List(vec!["reviewed".to_string()]))
    );
}

// ── original/current snapshotting ────────────────────────────────────────

#[test]
fn test_original_is_snapshotted_once_and_survives_several_updates() {
    // Given
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_001"), requirement("open", &[], &[]));
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_001",
        vec![("status", FieldMutationMode::Set("closed".to_string()))],
        true,
    ));
    index.entity_updates.push(update_record(
        "a.rst",
        2,
        "REQ_001",
        vec![("status", FieldMutationMode::Set("open".to_string()))],
        true,
    ));

    // When
    apply_entity_updates(&mut index, &schema());

    // Then
    let history = &index.entity_update_history[&id("REQ_001")].attributes["status"];
    assert_eq!(
        history.original,
        Some(AttributeValue::String("open".to_string()))
    );
    assert_eq!(history.applied.len(), 2);
    assert_eq!(
        history.current,
        Some(AttributeValue::String("open".to_string()))
    );
    // The record itself is never mutated.
    assert_eq!(
        index.entities[&id("REQ_001")].attributes["status"],
        AttributeValue::String("open".to_string())
    );
}

// ── conflicting updates ──────────────────────────────────────────────────

#[test]
fn test_two_sets_to_different_values_from_different_documents_conflict() {
    // Given
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_001"), requirement("open", &[], &[]));
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_001",
        vec![("status", FieldMutationMode::Set("closed".to_string()))],
        true,
    ));
    index.entity_updates.push(update_record(
        "b.rst",
        1,
        "REQ_001",
        vec![("status", FieldMutationMode::Set("open".to_string()))],
        true,
    ));

    // When
    let diagnostics = apply_entity_updates(&mut index, &schema());

    // Then — reported once per involved directive, in its own file
    assert_eq!(
        codes(&diagnostics),
        vec![
            DiagnosticCode::EntityUpdateConflictingUpdate,
            DiagnosticCode::EntityUpdateConflictingUpdate
        ]
    );
    let by_path: Vec<&str> = diagnostics.iter().map(|d| d.source_path.as_str()).collect();
    assert!(by_path.contains(&"a.rst"));
    assert!(by_path.contains(&"b.rst"));

    let history = &index.entity_update_history[&id("REQ_001")].attributes["status"];
    assert_eq!(history.applied[1].conflicts_with, Some(0));
}

#[test]
fn test_two_sets_to_the_same_value_do_not_conflict() {
    // Given
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_001"), requirement("open", &[], &[]));
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_001",
        vec![("status", FieldMutationMode::Set("closed".to_string()))],
        true,
    ));
    index.entity_updates.push(update_record(
        "b.rst",
        1,
        "REQ_001",
        vec![("status", FieldMutationMode::Set("closed".to_string()))],
        true,
    ));

    // When
    let diagnostics = apply_entity_updates(&mut index, &schema());

    // Then — nothing to disagree about
    assert!(diagnostics.is_empty());
}

#[test]
fn test_append_then_remove_from_different_documents_never_conflict() {
    // Given — explicitly incremental operations, never a disagreement
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_001"), requirement("open", &["boot"], &[]));
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_001",
        vec![(
            "tags",
            FieldMutationMode::Append("safety-critical".to_string()),
        )],
        true,
    ));
    index.entity_updates.push(update_record(
        "b.rst",
        1,
        "REQ_001",
        vec![("tags", FieldMutationMode::Remove("boot".to_string()))],
        true,
    ));

    // When
    let diagnostics = apply_entity_updates(&mut index, &schema());

    // Then
    assert!(diagnostics.is_empty());
    assert_eq!(
        index.effective_attribute(&id("REQ_001"), "tags"),
        Some(&AttributeValue::List(vec!["safety-critical".to_string()]))
    );
}

// ── record immutability ──────────────────────────────────────────────────

#[test]
fn test_apply_entity_updates_never_mutates_entities_itself() {
    // Given
    let mut index = ProjectIndex::default();
    index
        .entities
        .insert(id("REQ_001"), requirement("open", &[], &[]));
    let before = index.entities.clone();
    index.entity_updates.push(update_record(
        "a.rst",
        1,
        "REQ_001",
        vec![("status", FieldMutationMode::Set("closed".to_string()))],
        true,
    ));

    // When
    apply_entity_updates(&mut index, &schema());

    // Then — the as-authored record is byte-for-byte unchanged
    assert_eq!(index.entities, before);
}
