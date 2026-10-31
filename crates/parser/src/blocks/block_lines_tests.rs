//! Where each block was written: the lines the parse's source map records
//! for every node an editor's preview places, by the `BlockId` it carries.

use rinx_ast::{Directive, Node, Position, SourceMap, Span};

/// The lines `map` records for `node`, as `((line, column), (line, column))`.
fn lines_of(node: &Node, map: &SourceMap) -> Option<((u32, u32), (u32, u32))> {
    let span = map.block_lines(node.block()?)?;
    Some((
        (span.start.line, span.start.column),
        (span.end.line, span.end.column),
    ))
}

#[test]
fn test_a_paragraph_records_its_lines() {
    // Given
    let input = "First line\nsecond line.\n\nNext.\n";

    // When
    let (doc, map) = crate::parse_placed("test.rst", input);

    // Then
    assert_eq!(lines_of(&doc.nodes[0], &map), Some(((1, 1), (2, 13))));
    assert_eq!(lines_of(&doc.nodes[1], &map), Some(((4, 1), (4, 6))));
}

#[test]
fn test_a_paragraph_and_the_literal_block_its_colons_introduce_record_their_own_lines() {
    // Given
    let input = "Example::\n\n   code\n   more\n";

    // When
    let (doc, map) = crate::parse_placed("test.rst", input);

    // Then — the literal block from the `::` line, as it folds
    assert!(matches!(doc.nodes[1], Node::LiteralBlock { .. }));
    assert_eq!(lines_of(&doc.nodes[0], &map), Some(((1, 1), (1, 10))));
    assert_eq!(lines_of(&doc.nodes[1], &map), Some(((1, 1), (4, 8))));
}

#[test]
fn test_a_standalone_literal_block_records_its_lines() {
    // Given
    let input = "Text.\n\n::\n\n   code\n";

    // When
    let (doc, map) = crate::parse_placed("test.rst", input);

    // Then
    assert!(matches!(doc.nodes[1], Node::LiteralBlock { .. }));
    assert_eq!(lines_of(&doc.nodes[1], &map), Some(((3, 1), (5, 8))));
}

#[test]
fn test_each_bullet_list_item_records_its_lines_and_its_content_its_own() {
    // Given
    let input = "- one\n  continued\n- two\n";

    // When
    let (doc, map) = crate::parse_placed("test.rst", input);

    // Then
    let Node::BulletList { items, .. } = &doc.nodes[0] else {
        panic!("expected a bullet list, found {:?}", doc.nodes[0]);
    };
    let item_lines: Vec<_> = items
        .iter()
        .map(|item| item.block.and_then(|id| map.block_lines(id)))
        .collect();
    assert_eq!(
        item_lines,
        [
            Some(Span::new(Position::new(1, 1), Position::new(2, 12))),
            Some(Span::new(Position::new(3, 1), Position::new(3, 6))),
        ]
    );
    assert_eq!(lines_of(&items[0].nodes[0], &map), Some(((1, 3), (2, 12))));
    assert_eq!(doc.nodes[0].block(), None);
}

#[test]
fn test_each_enumerated_list_item_records_its_lines() {
    // Given
    let input = "1. one\n2. two\n   more\n";

    // When
    let (doc, map) = crate::parse_placed("test.rst", input);

    // Then
    let Node::EnumeratedList { items, .. } = &doc.nodes[0] else {
        panic!("expected an enumerated list, found {:?}", doc.nodes[0]);
    };
    let starts: Vec<_> = items
        .iter()
        .map(|item| item.block.and_then(|id| map.block_lines(id)))
        .map(|span| span.map(|span| (span.start.line, span.end.line)))
        .collect();
    assert_eq!(starts, [Some((1, 1)), Some((2, 3))]);
}

#[test]
fn test_each_definition_list_item_records_its_lines_from_its_term() {
    // Given
    let input = "term\n   Its definition.\nother\n   Another.\n";

    // When
    let (doc, map) = crate::parse_placed("test.rst", input);

    // Then
    let Node::DefinitionList { items } = &doc.nodes[0] else {
        panic!("expected a definition list, found {:?}", doc.nodes[0]);
    };
    let lines: Vec<_> = items
        .iter()
        .map(|item| item.block.and_then(|id| map.block_lines(id)))
        .collect();
    assert_eq!(
        lines,
        [
            Some(Span::new(Position::new(1, 1), Position::new(2, 19))),
            Some(Span::new(Position::new(3, 1), Position::new(4, 12))),
        ]
    );
}

#[test]
fn test_each_option_list_item_records_its_lines() {
    // Given
    let input = "-a        Output all.\n--verbose\n          Talk more.\n";

    // When
    let (doc, map) = crate::parse_placed("test.rst", input);

    // Then
    let Node::OptionList { items } = &doc.nodes[0] else {
        panic!("expected an option list, found {:?}", doc.nodes[0]);
    };
    let lines: Vec<_> = items
        .iter()
        .map(|item| item.block.and_then(|id| map.block_lines(id)))
        .map(|span| span.map(|span| (span.start.line, span.end.line)))
        .collect();
    assert_eq!(lines, [Some((1, 1)), Some((2, 3))]);
}

#[test]
fn test_a_table_a_block_quote_a_line_block_and_a_transition_record_their_lines() {
    // Given
    let input = "\
Text.

+---+
| a |
+---+

   Quoted.

| A line.
| Another.

----

More.
";

    // When
    let (doc, map) = crate::parse_placed("test.rst", input);

    // Then
    let starts: Vec<_> = doc
        .nodes
        .iter()
        .map(|node| lines_of(node, &map).map(|(start, end)| (start.0, end.0)))
        .collect();
    assert_eq!(
        starts,
        [
            Some((1, 1)),
            Some((3, 5)),
            Some((7, 7)),
            Some((9, 10)),
            Some((12, 12)),
            Some((14, 14)),
        ]
    );
}

#[test]
fn test_a_doctest_block_records_its_lines() {
    // Given
    let input = "Text.\n\n>>> 1 + 1\n2\n";

    // When
    let (doc, map) = crate::parse_placed("test.rst", input);

    // Then
    assert!(matches!(doc.nodes[1], Node::DoctestBlock { .. }));
    assert_eq!(lines_of(&doc.nodes[1], &map), Some(((3, 1), (4, 2))));
}

#[test]
fn test_a_directive_records_its_marker_line_and_its_body_its_own() {
    // Given
    let input = ".. note::\n\n   Inside.\n";

    // When
    let (doc, map) = crate::parse_placed("test.rst", input);

    // Then
    let Node::Directive {
        directive: Directive::Admonition { body, .. },
        ..
    } = &doc.nodes[0]
    else {
        panic!("expected a note, found {:?}", doc.nodes[0]);
    };
    assert_eq!(lines_of(&doc.nodes[0], &map), Some(((1, 1), (1, 10))));
    assert_eq!(lines_of(&body[0], &map), Some(((3, 4), (3, 11))));
}

#[test]
fn test_a_directive_yielding_nothing_records_nothing() {
    // Given — a `.. role::` yields no node
    let input = ".. role:: custom\n\nText.\n";

    // When
    let (doc, map) = crate::parse_placed("test.rst", input);

    // Then — the paragraph holds its own line
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(lines_of(&doc.nodes[0], &map), Some(((3, 1), (3, 6))));
}

#[test]
fn test_a_comment_records_no_lines() {
    // Given
    let input = ".. a comment\n";

    // When
    let (mut doc, _map) = crate::parse_placed("test.rst", input);

    // Then
    assert_eq!(doc.nodes[0], Node::Comment);
    assert_eq!(doc.nodes[0].block_mut(), None);
}
