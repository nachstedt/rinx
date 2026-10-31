//! Tests for the span a section title records: its text line, where an
//! editor's outline and a definition lead.

use super::*;
use rinx_ast::{Position, Span};

/// The lines `map` records for every heading in `nodes`, in document order.
fn heading_spans(nodes: &[Node], map: &rinx_ast::SourceMap) -> Vec<Option<Span>> {
    let mut spans = Vec::new();
    rinx_ast::walk_nodes(nodes, &mut |node| {
        if let Node::Heading { block, .. } = node {
            spans.push(block.and_then(|id| map.block_lines(id)));
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
    let (doc, map) = crate::parse_placed("test.rst", "Intro\n=====\n\ntext\n");

    // Then
    assert_eq!(heading_spans(&doc.nodes, &map), vec![Some(span_at(1, 5))]);
}

#[test]
fn test_parse_records_an_overlined_titles_text_line_not_its_overline() {
    // When
    let (doc, map) = crate::parse_placed("test.rst", "=======\n Intro\n=======\n\ntext\n");

    // Then
    assert_eq!(heading_spans(&doc.nodes, &map), vec![Some(span_at(2, 6))]);
}

#[test]
fn test_parse_records_each_titles_own_line() {
    // Given
    let input = "Intro\n=====\n\ntext\n\nUsage\n-----\n\nmore\n";

    // When
    let (doc, map) = crate::parse_placed("test.rst", input);

    // Then
    assert_eq!(
        heading_spans(&doc.nodes, &map),
        vec![Some(span_at(1, 5)), Some(span_at(6, 5))]
    );
}
