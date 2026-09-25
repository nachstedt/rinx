use rusty_sphinx_ast::{Directive, EntitySequence, EntitySequenceSource, ImageAlign, Node};
use rusty_sphinx_entity::{EntitySchema, NoReservedNames, load_schema};

use crate::context::ParseCtx;
use crate::parse_with_ctx;

/// A component sends messages, and a message is sent on to components — the
/// shape every sequence diagram walks.
fn schema() -> EntitySchema {
    load_schema(
        r#"
        [[entity_type]]
        name = "component"

          [[entity_type.attribute]]
          name = "status"
          type = "enum"
          values = ["open", "closed"]

          [[entity_type.relation]]
          name = "sends"
          to = ["message"]

        [[entity_type]]
        name = "message"

          [[entity_type.relation]]
          name = "sends"
          to = ["component"]

          [[entity_type.relation]]
          name = "replies"
          to = ["component"]
        "#,
        &NoReservedNames,
    )
    .expect("the test schema should load")
}

fn parse(rst: &str) -> rusty_sphinx_ast::Document {
    let schema = schema();
    let ctx = ParseCtx::with_domain(rusty_sphinx_ast::Domain::Py).with_schema(&schema);
    parse_with_ctx("arch/runtime", rst, &ctx)
}

/// The first sequence diagram in `rst`.
fn parse_sequence(rst: &str) -> EntitySequence {
    parse(rst)
        .nodes
        .iter()
        .find_map(|node| match node {
            Node::Directive(Directive::EntitySequence(sequence)) => Some((**sequence).clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no sequence diagram parsed from:\n{rst}"))
}

/// The diagnostic codes a parse reported, as their author-facing ids.
fn codes(rst: &str) -> Vec<String> {
    parse(rst)
        .diagnostics
        .iter()
        .map(|d| d.code.as_str().to_string())
        .collect()
}

/// Whether `rst` degraded to an error block quoting `name`.
fn is_malformed(rst: &str, name: &str) -> bool {
    parse(rst).nodes.iter().any(|node| {
        matches!(node, Node::Directive(Directive::Malformed { name: written, .. }) if written == name)
    })
}

#[test]
fn test_the_start_and_relations_are_read_in_the_order_written() {
    // Given
    let rst = ".. entity-sequence::\n   :start: COMP_UI, COMP_HAL\n   :relations: sends, replies\n";

    // When
    let sequence = parse_sequence(rst);

    // Then
    let start: Vec<&str> = sequence
        .start
        .as_slice()
        .iter()
        .map(rusty_sphinx_ast::EntityId::as_str)
        .collect();
    assert_eq!(start, ["COMP_UI", "COMP_HAL"]);
    assert_eq!(sequence.relations.as_slice(), ["sends", "replies"]);
    assert_eq!(sequence.source, EntitySequenceSource::EntitySequence);
    assert!(codes(rst).is_empty(), "{:?}", codes(rst));
}

#[test]
fn test_needsequence_is_the_same_directive_under_sphinx_needs_spelling() {
    // Given — the corpus's own shape: `;`-separated starts, `:link_types:`
    let rst =
        ".. needsequence:: Startup Sequence\n   :start: COMP_UI; COMP_HAL\n   :link_types: sends\n";

    // When
    let sequence = parse_sequence(rst);

    // Then
    assert_eq!(sequence.source, EntitySequenceSource::NeedSequence);
    assert_eq!(sequence.start.as_slice().len(), 2);
    assert_eq!(sequence.relations.as_slice(), ["sends"]);
}

#[test]
fn test_the_argument_is_the_caption_as_in_sphinx_needs() {
    // Given
    let rst = ".. needsequence:: Startup Sequence\n   :start: COMP_UI\n   :link_types: sends\n";

    // When
    let sequence = parse_sequence(rst);

    // Then
    assert_eq!(sequence.caption.as_deref(), Some("Startup Sequence"));
}

#[test]
fn test_a_caption_option_overrides_the_argument() {
    // Given
    let rst = ".. entity-sequence:: Startup\n   :start: COMP_UI\n   :relations: sends\n   :caption: Boot\n";

    // When
    let sequence = parse_sequence(rst);

    // Then
    assert_eq!(sequence.caption.as_deref(), Some("Boot"));
}

#[test]
fn test_presentation_options_are_read() {
    // Given
    let rst = ".. entity-sequence::\n   :start: COMP_UI\n   :relations: sends\n   :filter: status == \"open\"\n   :max-items: 4\n   :config: toptobottom\n   :debug:\n   :align: left\n   :width: 400px\n   :scale: 50\n   :class: wide tall\n   :name: boot-sequence\n";

    // When
    let sequence = parse_sequence(rst);

    // Then
    assert!(sequence.filter.is_some());
    assert_eq!(sequence.max_items.map(std::num::NonZeroU32::get), Some(4));
    assert_eq!(sequence.config.as_deref(), Some("toptobottom"));
    assert!(sequence.debug);
    assert_eq!(sequence.align, Some(ImageAlign::Left));
    assert_eq!(sequence.rendered_width().unwrap().to_string(), "200px");
    assert_eq!(sequence.classes, ["wide", "tall"]);
    assert_eq!(sequence.name.unwrap().as_str(), "boot-sequence");
    assert!(codes(rst).is_empty(), "{:?}", codes(rst));
}

#[test]
fn test_a_max_items_of_zero_means_no_limit() {
    // Given — sphinx-needs' documented spelling of "draw everything"
    let rst = ".. needsequence::\n   :start: COMP_UI\n   :link_types: sends\n   :max_items: 0\n";

    // When
    let sequence = parse_sequence(rst);

    // Then
    assert_eq!(sequence.max_items, None);
    assert!(codes(rst).is_empty());
}

#[test]
fn test_an_unreadable_max_items_is_reported_and_ignored() {
    // Given
    let rst =
        ".. entity-sequence::\n   :start: COMP_UI\n   :relations: sends\n   :max-items: many\n";

    // When
    let sequence = parse_sequence(rst);

    // Then
    assert_eq!(sequence.max_items, None);
    assert_eq!(codes(rst), ["entity-sequence.invalid-max-items"]);
}

#[test]
fn test_a_missing_start_degrades_to_an_error_block() {
    // Given
    let rst = ".. entity-sequence::\n   :relations: sends\n";

    // When / Then
    assert!(is_malformed(rst, "entity-sequence"));
    assert_eq!(codes(rst), ["entity-sequence.missing-start"]);
}

#[test]
fn test_an_empty_start_is_reported_as_missing() {
    // Given
    let rst = ".. entity-sequence::\n   :start:\n   :relations: sends\n";

    // When / Then
    assert!(is_malformed(rst, "entity-sequence"));
    assert_eq!(codes(rst), ["entity-sequence.missing-start"]);
}

#[test]
fn test_missing_relations_degrade_to_an_error_block_naming_the_declared_ones() {
    // Given — sphinx-needs would default to `links`; this build requires it
    let rst = ".. needsequence::\n   :start: COMP_UI\n";

    // When
    let doc = parse(rst);

    // Then
    assert!(is_malformed(rst, "needsequence"));
    assert_eq!(codes(rst), ["entity-sequence.missing-relations"]);
    let message = &doc.diagnostics[0].message;
    assert!(message.contains("replies, sends"), "{message}");
}

#[test]
fn test_both_missing_options_are_reported() {
    // Given
    let rst = ".. entity-sequence::\n";

    // When / Then
    assert_eq!(
        codes(rst),
        [
            "entity-sequence.missing-start",
            "entity-sequence.missing-relations"
        ]
    );
}

#[test]
fn test_an_unknown_relation_is_dropped_and_the_rest_kept() {
    // Given
    let rst = ".. entity-sequence::\n   :start: COMP_UI\n   :relations: sends, calls\n";

    // When
    let sequence = parse_sequence(rst);

    // Then
    assert_eq!(sequence.relations.as_slice(), ["sends"]);
    assert_eq!(codes(rst), ["entity-sequence.unknown-relation"]);
}

#[test]
fn test_only_unknown_relations_degrade_without_a_second_report() {
    // Given
    let rst = ".. entity-sequence::\n   :start: COMP_UI\n   :relations: calls\n";

    // When / Then — the entry was reported; "missing" would be untrue
    assert!(is_malformed(rst, "entity-sequence"));
    assert_eq!(codes(rst), ["entity-sequence.unknown-relation"]);
}

#[test]
fn test_a_malformed_start_id_is_dropped_and_the_rest_kept() {
    // Given
    let rst = ".. entity-sequence::\n   :start: COMP_UI, not an id\n   :relations: sends\n";

    // When
    let sequence = parse_sequence(rst);

    // Then
    assert_eq!(sequence.start.as_slice().len(), 1);
    assert_eq!(codes(rst), ["entity-sequence.invalid-start"]);
}

#[test]
fn test_a_filter_naming_an_unknown_field_is_reported() {
    // Given
    let rst = ".. entity-sequence::\n   :start: COMP_UI\n   :relations: sends\n   :filter: colour == \"red\"\n";

    // When / Then
    assert_eq!(codes(rst), ["entity-sequence.unknown-field"]);
}

#[test]
fn test_an_unparsable_filter_is_reported() {
    // Given
    let rst =
        ".. entity-sequence::\n   :start: COMP_UI\n   :relations: sends\n   :filter: status ==\n";

    // When / Then
    assert_eq!(codes(rst), ["entity-sequence.invalid-filter"]);
}

#[test]
fn test_a_sphinx_needs_option_this_build_does_not_draw_is_refused_by_name() {
    // Given
    let rst = ".. needsequence::\n   :start: COMP_UI\n   :link_types: sends\n   :show_legend:\n";

    // When
    let doc = parse(rst);

    // Then
    assert_eq!(codes(rst), ["entity-sequence.unsupported-option"]);
    assert!(
        doc.diagnostics[0].message.contains("no legend is drawn"),
        "{}",
        doc.diagnostics[0].message
    );
}

#[test]
fn test_an_option_nobody_knows_is_reported_as_unknown() {
    // Given
    let rst = ".. entity-sequence::\n   :start: COMP_UI\n   :relations: sends\n   :colour: red\n";

    // When / Then
    assert_eq!(codes(rst), ["directive.entity-sequence-unknown-option"]);
}

#[test]
fn test_body_content_is_reported() {
    // Given
    let rst = ".. entity-sequence::\n   :start: COMP_UI\n   :relations: sends\n\n   A -> B\n";

    // When / Then
    assert_eq!(codes(rst), ["directive.entity-sequence-unknown-option"]);
}

#[test]
fn test_a_scale_without_a_width_is_reported_on_its_own_line() {
    // Given
    let rst = ".. entity-sequence::\n   :start: COMP_UI\n   :relations: sends\n   :scale: 50\n";

    // When
    let doc = parse(rst);

    // Then
    assert_eq!(codes(rst), ["entity-sequence.unusable-scale"]);
    assert_eq!(doc.diagnostics[0].span.unwrap().start.line, 4);
}

#[test]
fn test_invalid_placement_options_are_reported() {
    // Given
    let rst = ".. entity-sequence::\n   :start: COMP_UI\n   :relations: sends\n   :align: middle\n   :width: wide\n   :scale: big\n";

    // When / Then
    assert_eq!(
        codes(rst),
        [
            "entity-sequence.invalid-align",
            "entity-sequence.invalid-width",
            "entity-sequence.invalid-scale"
        ]
    );
}

#[test]
fn test_an_empty_name_is_reported() {
    // Given
    let rst = ".. entity-sequence::\n   :start: COMP_UI\n   :relations: sends\n   :name:\n";

    // When / Then
    assert_eq!(codes(rst), ["entity-sequence.empty-option-value"]);
}

#[test]
fn test_a_sequence_inside_a_dropdown_reports_at_its_own_line() {
    // Given — a container parses its body under a rebased origin
    let rst = ".. dropdown:: Messages\n\n   .. entity-sequence::\n      :relations: sends\n";

    // When
    let doc = parse(rst);

    // Then
    assert_eq!(doc.diagnostics.len(), 1);
    assert_eq!(doc.diagnostics[0].span.unwrap().start.line, 3);
}
