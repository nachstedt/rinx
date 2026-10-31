//! Tests for the span a section title records: its text line, where an
//! editor's outline and a definition lead.

use super::*;
use crate::parse_document;
use rinx_ast::{Position, Span};

/// The span of every heading in `nodes`, in document order.
fn heading_spans(nodes: &[Node]) -> Vec<Option<Span>> {
    let mut spans = Vec::new();
    rinx_ast::walk_nodes(nodes, &mut |node| {
        if let Node::Heading { span, .. } = node {
            spans.push(*span);
        }
    });
    spans
}

fn span_at(line: u32, columns: u32) -> Span {
    Span::new(Position::new(line, 1), Position::new(line, columns + 1))
}

#[test]
fn test_parse_records_an_underlined_titles_text_line() {
    // When
    let doc = parse_document("test.rst", "Intro\n=====\n\ntext\n");

    // Then
    assert_eq!(heading_spans(&doc.nodes), vec![Some(span_at(1, 5))]);
}

#[test]
fn test_parse_records_an_overlined_titles_text_line_not_its_overline() {
    // When
    let doc = parse_document("test.rst", "=======\n Intro\n=======\n\ntext\n");

    // Then
    assert_eq!(heading_spans(&doc.nodes), vec![Some(span_at(2, 6))]);
}

#[test]
fn test_parse_records_each_titles_own_line() {
    // Given
    let input = "Intro\n=====\n\ntext\n\nUsage\n-----\n\nmore\n";

    // When
    let doc = parse_document("test.rst", input);

    // Then
    assert_eq!(
        heading_spans(&doc.nodes),
        vec![Some(span_at(1, 5)), Some(span_at(6, 5))]
    );
}
