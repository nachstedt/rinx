use rinx_ast::{Diagnostic, DiagnosticCode, Directive, EntityUpdate, FieldMutationMode, Node};
use rinx_entity::{EntitySchema, NoReservedNames, load_schema};

use crate::context::ParseCtx;
use crate::parse_with_ctx;

/// The schema every test below parses against.
fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "req"
        argument = { fields = ["title"] }
        id = { prefix = "REQ_" }

          [[entity_type.attribute]]
          name = "title"
          type = "string"

          [[entity_type.attribute]]
          name = "status"
          type = "enum"
          values = ["open", "closed"]
          default = "open"

          [[entity_type.attribute]]
          name = "tags"
          type = "list<string>"

          [[entity_type.section]]
          name = "rationale"

          [[entity_type.relation]]
          name = "links"
          to = ["req"]
          multiple = true
        "#,
        &NoReservedNames,
    )
    .expect("the test schema should load")
}

/// Parses `rst` against the test schema, returning the whole document.
fn parse(rst: &str) -> rinx_ast::Document {
    let schema = schema();
    let ctx = ParseCtx::with_domain(rinx_ast::Domain::Py).with_schema(&schema);
    parse_with_ctx("specs/boot", rst, &ctx)
}

/// The directive's own node, plus the document's diagnostics.
fn parse_update(rst: &str) -> (Directive, Vec<Diagnostic>) {
    let doc = parse(rst);
    let directive = doc
        .nodes
        .iter()
        .find_map(|node| match node {
            Node::Directive(d @ Directive::EntityUpdate(_)) => Some(d.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no entity-update parsed from:\n{rst}"));
    (directive, doc.diagnostics)
}

fn update_of(directive: &Directive) -> &EntityUpdate {
    match directive {
        Directive::EntityUpdate(update) => update,
        other => panic!("expected an EntityUpdate, found {other:?}"),
    }
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<DiagnosticCode> {
    diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn test_a_bare_id_target_reads_as_a_candidate_id() {
    // Given
    let input = ".. entity-update:: REQ_001\n   :status: closed\n";

    // When
    let (directive, diagnostics) = parse_update(input);

    // Then
    let update = update_of(&directive);
    assert_eq!(
        update
            .target
            .candidate_id
            .as_ref()
            .map(rinx_ast::EntityId::as_str),
        Some("REQ_001")
    );
    assert!(diagnostics.is_empty());
}

#[test]
fn test_a_filter_target_is_read_with_no_candidate_id() {
    // Given
    let input = ".. entity-update:: type == \"req\"\n   :status: closed\n";

    // When
    let (directive, diagnostics) = parse_update(input);

    // Then
    let update = update_of(&directive);
    assert_eq!(update.target.candidate_id, None);
    assert!(update.target.filter.is_some());
    assert!(diagnostics.is_empty());
}

#[test]
fn test_an_empty_argument_becomes_malformed() {
    // Given
    let input = ".. entity-update::\n   :status: closed\n";

    // When
    let doc = parse(input);

    // Then
    assert!(matches!(
        doc.nodes.first(),
        Some(Node::Directive(Directive::Malformed { .. }))
    ));
}

#[test]
fn test_needextend_dispatches_to_the_same_node() {
    // Given
    let input = ".. needextend:: REQ_001\n   :status: closed\n";

    // When
    let (directive, diagnostics) = parse_update(input);

    // Then
    let update = update_of(&directive);
    assert_eq!(update.source, rinx_ast::EntityUpdateSource::NeedExtend);
    assert!(diagnostics.is_empty());
}

#[test]
fn test_a_plain_option_is_a_set_mutation() {
    // Given
    let input = ".. entity-update:: REQ_001\n   :status: closed\n";

    // When
    let (directive, _) = parse_update(input);

    // Then
    let update = update_of(&directive);
    assert_eq!(
        update.fields,
        vec![rinx_ast::FieldMutation {
            field: "status".to_string(),
            mode: FieldMutationMode::Set("closed".to_string()),
            span: update.fields[0].span,
        }]
    );
}

#[test]
fn test_a_plus_prefixed_option_is_an_append_mutation() {
    // Given
    let input = ".. entity-update:: REQ_001\n   :+tags: safety-critical\n";

    // When
    let (directive, _) = parse_update(input);

    // Then
    let update = update_of(&directive);
    assert_eq!(update.fields[0].field, "tags");
    assert_eq!(
        update.fields[0].mode,
        FieldMutationMode::Append("safety-critical".to_string())
    );
}

#[test]
fn test_a_minus_prefixed_option_with_a_value_is_a_remove_mutation() {
    // Given
    let input = ".. entity-update:: REQ_001\n   :-tags: boot\n";

    // When
    let (directive, _) = parse_update(input);

    // Then
    let update = update_of(&directive);
    assert_eq!(update.fields[0].field, "tags");
    assert_eq!(
        update.fields[0].mode,
        FieldMutationMode::Remove("boot".to_string())
    );
}

#[test]
fn test_a_minus_prefixed_option_with_no_value_is_a_clear_mutation() {
    // Given
    let input = ".. entity-update:: REQ_001\n   :-tags:\n";

    // When
    let (directive, _) = parse_update(input);

    // Then
    let update = update_of(&directive);
    assert_eq!(update.fields[0].field, "tags");
    assert_eq!(update.fields[0].mode, FieldMutationMode::Clear);
}

#[test]
fn test_strict_defaults_to_true() {
    // Given
    let input = ".. entity-update:: REQ_001\n   :status: closed\n";

    // When
    let (directive, _) = parse_update(input);

    // Then
    assert!(update_of(&directive).strict);
}

#[test]
fn test_strict_can_be_turned_off() {
    // Given
    let input = ".. entity-update:: type == \"nope\"\n   :strict: false\n";

    // When
    let (directive, diagnostics) = parse_update(input);

    // Then
    assert!(!update_of(&directive).strict);
    assert!(diagnostics.is_empty());
}

#[test]
fn test_an_invalid_strict_value_is_reported_and_defaults_to_true() {
    // Given
    let input = ".. entity-update:: REQ_001\n   :strict: maybe\n";

    // When
    let (directive, diagnostics) = parse_update(input);

    // Then — the safer default: still checked, rather than silently quiet
    assert!(update_of(&directive).strict);
    assert_eq!(
        codes(&diagnostics),
        vec![DiagnosticCode::EntityUpdateInvalidStrict]
    );
}

#[test]
fn test_a_protected_field_is_rejected() {
    // Given
    let input = ".. entity-update:: REQ_001\n   :id: REQ_999\n";

    // When
    let (directive, diagnostics) = parse_update(input);

    // Then
    assert!(update_of(&directive).fields.is_empty());
    assert_eq!(
        codes(&diagnostics),
        vec![DiagnosticCode::EntityUpdateProtectedField]
    );
}

#[test]
fn test_title_is_a_protected_field() {
    // Given — divergence from upstream, see docs/decisions/019-entity-update.md
    let input = ".. entity-update:: REQ_001\n   :title: New title\n";

    // When
    let (_, diagnostics) = parse_update(input);

    // Then
    assert_eq!(
        codes(&diagnostics),
        vec![DiagnosticCode::EntityUpdateProtectedField]
    );
}

#[test]
fn test_a_declared_section_is_rejected_with_its_own_code() {
    // Given
    let input = ".. entity-update:: REQ_001\n   :rationale: New text\n";

    // When
    let (directive, diagnostics) = parse_update(input);

    // Then
    assert!(update_of(&directive).fields.is_empty());
    assert_eq!(
        codes(&diagnostics),
        vec![DiagnosticCode::EntityUpdateSectionNotSupported]
    );
}

#[test]
fn test_an_unknown_field_is_rejected() {
    // Given
    let input = ".. entity-update:: REQ_001\n   :bogus-field: x\n";

    // When
    let (directive, diagnostics) = parse_update(input);

    // Then
    assert!(update_of(&directive).fields.is_empty());
    assert_eq!(
        codes(&diagnostics),
        vec![DiagnosticCode::EntityUpdateUnknownField]
    );
}

#[test]
fn test_the_body_parses_as_ordinary_block_content() {
    // Given
    let input = ".. entity-update:: REQ_001\n   :status: closed\n\n   Closed after review.\n";

    // When
    let (directive, diagnostics) = parse_update(input);

    // Then
    let update = update_of(&directive);
    assert_eq!(
        update.body,
        vec![Node::Paragraph(vec![rinx_ast::InlineNode::Text(
            "Closed after review.".to_string()
        )])]
    );
    assert!(diagnostics.is_empty());
}

#[test]
fn test_a_diagnostic_inside_the_body_points_at_the_real_line() {
    // Given — the body context must be rebased past the option block
    let input =
        ".. entity-update:: REQ_001\n   :status: closed\n\n   .. contents::\n      :nope: 1\n";

    // When
    let (_, diagnostics) = parse_update(input);

    // Then
    let span = diagnostics
        .iter()
        .find(|d| d.code == DiagnosticCode::DirectiveContentsUnknownOption)
        .and_then(|d| d.span)
        .expect("the nested option is reported with a span");
    assert_eq!(span.start.line, 5);
}

#[test]
fn test_the_directives_own_span_is_kept() {
    // Given
    let input = "Intro.\n\n.. entity-update:: REQ_001\n   :status: closed\n";

    // When
    let (directive, _) = parse_update(input);

    // Then
    assert_eq!(
        update_of(&directive)
            .span
            .expect("an entity-update records its span")
            .start
            .line,
        3
    );
}
