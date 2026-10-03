//! Tests for section titles where none may stand — docutils' "Unexpected
//! section title" — and for the places that do allow them.

use super::test_support::diagnostic_codes;
use super::*;
use crate::parse;

/// The levels of every heading in `nodes`, however deeply nested.
fn heading_levels(nodes: &[Node]) -> Vec<u8> {
    let mut levels = Vec::new();
    rinx_ast::walk_nodes(nodes, &mut |node| {
        if let Node::Heading { level, .. } = node {
            levels.push(*level);
        }
    });
    levels
}

/// Asserts that `input` keeps its one title as a heading and reports it as
/// standing where it may not.
fn assert_unexpected_title(input: &str) {
    let doc = parse("test.rst", input);
    assert_eq!(
        diagnostic_codes(&doc),
        vec![DiagnosticCode::HeadingUnexpected],
        "{:?}",
        doc.nodes
    );
    assert_eq!(heading_levels(&doc.nodes).len(), 1, "{:?}", doc.nodes);
}

#[test]
fn test_parse_reports_a_title_in_a_block_quote() {
    assert_unexpected_title("Para\n\n   Quoted Title\n   ============\n\n   text\n");
}

#[test]
fn test_parse_reports_a_title_in_a_bullet_item() {
    assert_unexpected_title("- Item\n\n  Item Title\n  ==========\n\n  text\n");
}

#[test]
fn test_parse_reports_a_title_in_an_enumerated_item() {
    // Given — the underline is indented, so the first line is a list item
    // rather than a top-level title
    assert_unexpected_title("1. Item Title\n   ==========\n\n   text\n");
}

#[test]
fn test_parse_reports_a_title_in_a_definition() {
    assert_unexpected_title("Term\n   Definition Title\n   ================\n");
}

#[test]
fn test_parse_reports_a_title_in_a_grid_table_cell() {
    assert_unexpected_title("+------------+\n| Cell Title |\n| ========== |\n+------------+\n");
}

#[test]
fn test_parse_reports_a_title_in_an_admonition() {
    assert_unexpected_title(".. note::\n\n   Note Title\n   ==========\n\n   text\n");
}

#[test]
fn test_parse_reports_a_title_in_a_dropdown() {
    assert_unexpected_title(".. dropdown:: More\n\n   Inner\n   =====\n");
}

#[test]
fn test_parse_allows_a_title_in_an_object_description() {
    // Given — Sphinx parses an object's content with section headings allowed
    let input = ".. py:class:: Widget\n\n   Methods\n   =======\n\n   text\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(diagnostic_codes(&doc), Vec::new());
    assert_eq!(heading_levels(&doc.nodes), vec![1]);
}

#[test]
fn test_parse_reports_a_title_in_an_object_description_inside_a_note() {
    assert_unexpected_title(
        ".. note::\n\n   .. py:class:: Widget\n\n      Methods\n      =======\n",
    );
}

#[test]
fn test_parse_allows_a_title_in_a_top_level_if_builder() {
    // Given — the block is spliced into the document, titles and all
    let input = ".. if-builder:: html\n\n   Spliced\n   =======\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(diagnostic_codes(&doc), Vec::new());
    assert_eq!(heading_levels(&doc.nodes), vec![1]);
}

#[test]
fn test_parse_reports_a_title_in_an_if_builder_inside_a_note() {
    assert_unexpected_title(
        ".. note::\n\n   .. if-builder:: html\n\n      Spliced\n      =======\n",
    );
}

#[test]
fn test_parse_does_not_let_a_nested_title_claim_a_level() {
    // Given — the note's `-` title comes first, but may not set a level
    let input = "Top\n===\n\n.. note::\n\n   Inner\n   -----\n\nNext\n~~~~\n";

    // When
    let doc = parse("test.rst", input);

    // Then — `Next` is the document's second level, not its third
    assert_eq!(heading_levels(&doc.nodes), vec![1, 2, 2]);
    assert_eq!(
        diagnostic_codes(&doc),
        vec![DiagnosticCode::HeadingUnexpected]
    );
}

#[test]
fn test_parse_reports_a_malformed_nested_title_twice_over() {
    // Given — wrong in shape and in place
    let input = ".. note::\n\n   =======\n   Title\n   ==========\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(
        diagnostic_codes(&doc),
        vec![
            DiagnosticCode::HeadingOverlineMismatch,
            DiagnosticCode::HeadingUnexpected
        ]
    );
}

#[test]
fn test_parse_records_a_noqa_for_an_unexpected_title() {
    // Given
    let input = ".. noqa: heading.unexpected\n\n.. note::\n\n   Inner\n   =====\n";

    // When
    let doc = parse("test.rst", input);

    // Then
    assert_eq!(doc.diagnostics.len(), 1);
    assert!(
        doc.suppressions
            .iter()
            .any(|suppression| suppression
                .suppresses(doc.diagnostics[0].code, doc.diagnostics[0].span)),
        "{:?}",
        doc.suppressions
    );
}
