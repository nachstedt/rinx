//! Finding the cross-reference under the cursor.
//!
//! Every reference role keeps the span it was written at (ADR-003), so the
//! reference at a position is the one whose span contains it — found by
//! walking the parsed document's inline content, without re-reading the
//! text. Hover and go-to-definition both ask this first.

use rinx_ast::{Child, Document, InlineNode, Position, for_each_child, walk_nodes};

/// The inline node carrying a span that contains `position` in `document` —
/// a reference, or one of the few other nodes with a span (`:math:`,
/// `:code:`). `None` when the cursor is on plain text.
///
/// Only spans in the document's own text count: one in an `.. include::`d
/// fragment counts lines in that file, not in this one.
#[must_use]
pub fn reference_at(document: &Document, position: Position) -> Option<&InlineNode> {
    let mut found = None;
    walk_nodes(&document.nodes, &mut |node| {
        for_each_child(node, &mut |child| {
            if let Child::Inline(inlines) = child
                && found.is_none()
            {
                found = inlines.iter().find(|inline| {
                    inline.span().is_some_and(|span| {
                        span.file.is_none() && span.start <= position && position < span.end
                    })
                });
            }
        });
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The target written by the reference at line `line`, column `column`.
    fn target_at(text: &str, line: u32, column: u32) -> Option<String> {
        let document = rinx_parser::parse("index.rst", text);
        reference_at(&document, Position::new(line, column)).map(|inline| match inline {
            InlineNode::Reference { target, .. } | InlineNode::DocReference { target, .. } => {
                target.clone()
            }
            other => format!("{other:?}"),
        })
    }

    #[test]
    fn test_reference_at_finds_the_reference_whose_span_holds_the_position() {
        // Given — `:ref:` starts at column 5 and `:doc:` at column 25
        let text = "See :ref:`install` and :doc:`setup`.\n";

        // When / Then
        assert_eq!(target_at(text, 1, 5), Some("install".to_string()));
        assert_eq!(target_at(text, 1, 18), Some("install".to_string()));
        assert_eq!(target_at(text, 1, 26), Some("setup".to_string()));
    }

    #[test]
    fn test_reference_at_finds_nothing_on_plain_text_or_just_after_a_reference() {
        // Given — the span ends after the closing backtick, exclusively
        let text = "See :ref:`install` now.\n";

        // When / Then
        assert_eq!(target_at(text, 1, 2), None);
        assert_eq!(target_at(text, 1, 19), None);
    }

    #[test]
    fn test_reference_at_finds_a_reference_nested_in_a_directive_body() {
        // Given
        let text = "Title\n=====\n\n.. note::\n\n   See :ref:`install`.\n";

        // When / Then
        assert_eq!(target_at(text, 6, 12), Some("install".to_string()));
    }

    #[test]
    fn test_reference_at_ignores_a_reference_spliced_in_from_another_file() {
        // Given — a span counting lines in an included fragment
        let mut document = rinx_parser::parse("index.rst", "See :ref:`install`.\n");
        let rinx_ast::Node::Paragraph(inlines) = &mut document.nodes[0] else {
            unreachable!()
        };
        let span = inlines[1].span().unwrap();
        inlines[1] = inlines[1]
            .clone()
            .with_span(Some(span.with_file(Some(rinx_ast::FileId::new(0)))));

        // When / Then
        assert_eq!(reference_at(&document, Position::new(1, 6)), None);
    }
}
