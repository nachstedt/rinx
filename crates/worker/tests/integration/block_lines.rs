//! A block's lines are the editor's to know, never the build's: every block
//! an editor's preview places holds a `BlockId` into the parse's source map
//! rather than a line, so moving lines leaves the `.ast` a Bazel build writes
//! byte for byte as it was, and no page re-renders.

use rinx_ast::{Node, SourceMap, walk_nodes};

/// One document holding every kind of block that is placed, and nothing a
/// build phase reports at (no reference, entity or code block).
const BLOCKS: &str = "\
Guide
=====

A paragraph,
over two lines.

- a bullet item
- another

1. an enumerated item

term
   Its definition.

-v  Talk more.

+---+---+
| a | b |
+---+---+

=== ===
x   y
=== ===

An example::

   literal

>>> 1 + 1
2

   A block quote.

| A line block.

----

.. note::

   Inside a note.

.. _label:

Closing.
";

/// `BLOCKS` with every block moved down by blank lines above the first —
/// not between blocks, where a second blank line after a `::` would be
/// part of the literal block's text.
fn shifted() -> String {
    format!("\n\n\n{BLOCKS}")
}

/// The `.ast` the build's parse action writes for `source`.
fn written(source: &str) -> String {
    let document =
        rinx_parser::parse("guide.rst", source, &rinx_parser::ParseCtx::default()).document;
    serde_json::to_string(&document).expect("serializes")
}

#[test]
fn test_moving_lines_above_blocks_leaves_the_written_ast_unchanged() {
    // When / Then — where each block was written is the source map's alone
    assert_eq!(written(BLOCKS), written(&shifted()));
}

/// The first line of every placed node in `nodes`, nested ones included, in
/// document order — `None` for one placed nowhere.
fn first_lines(nodes: &[Node], map: &SourceMap) -> Vec<Option<u32>> {
    let mut lines = Vec::new();
    walk_nodes(nodes, &mut |node| {
        let mut node = node.clone();
        if node.block_mut().is_some() {
            lines.push(
                node.block()
                    .and_then(|id| map.block_lines(id))
                    .map(|span| span.start.line),
            );
        }
    });
    lines
}

#[test]
fn test_every_block_is_placed_at_its_first_line_and_moves_with_it() {
    // Given
    let parse =
        |source: &str| rinx_parser::parse("guide.rst", source, &rinx_parser::ParseCtx::default());
    let (original, moved) = (parse(BLOCKS), parse(&shifted()));

    // When
    let before = first_lines(&original.document.nodes, &original.source_map);
    let after = first_lines(&moved.document.nodes, &moved.source_map);

    // Then — every block placed, each further down once moved
    assert!(before.len() >= 15, "{before:?}");
    assert!(before.iter().all(Option::is_some), "{before:?}");
    assert_eq!(before.len(), after.len());
    for (old, new) in before.iter().zip(&after) {
        assert!(new > old, "a block stayed at line {old:?}");
    }
}
