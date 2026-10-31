//! Finding the cross-reference under the cursor.
//!
//! Every reference role keeps the span it was written at (ADR-003), so the
//! reference at a position is the one whose span contains it — found by
//! walking the parsed document's inline content, without re-reading the
//! text. Hover and go-to-definition both ask this first. A `.. toctree::`
//! entry keeps the span of its line the same way, so the entry at a position
//! is found alike.

use rinx_ast::{
    Child, Directive, Document, InlineNode, Node, Position, Span, TocEntry, for_each_child,
    walk_nodes,
};

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
                found = inlines
                    .iter()
                    .find(|inline| inline.span().is_some_and(|span| holds(span, position)));
            }
        });
    });
    found
}

/// The `.. toctree::` entry whose line holds `position` in `document`, or
/// `None` when the cursor is on no entry — the directive's own line and its
/// options included.
///
/// Only entries written in the document's own text count, as for
/// [`reference_at`].
#[must_use]
pub fn toc_entry_at(document: &Document, position: Position) -> Option<&TocEntry> {
    let mut found = None;
    walk_nodes(&document.nodes, &mut |node| {
        if let Node::Directive(Directive::Toctree(toctree)) = node
            && found.is_none()
        {
            found = toctree
                .entries
                .iter()
                .find(|entry| entry.span().is_some_and(|span| holds(span, position)));
        }
    });
    found
}

/// Whether `span`, written in the document's own text, holds `position`.
fn holds(span: Span, position: Position) -> bool {
    span.file.is_none() && span.start <= position && position < span.end
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The target written by the reference at line `line`, column `column`.
    fn target_at(text: &str, line: u32, column: u32) -> Option<String> {
        let document =
            rinx_parser::parse("index.rst", text, &rinx_parser::ParseCtx::default()).document;
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

    /// What the toctree entry at line `line`, column `column` names.
    fn entry_at(text: &str, line: u32, column: u32) -> Option<String> {
        let document =
            rinx_parser::parse("index.rst", text, &rinx_parser::ParseCtx::default()).document;
        toc_entry_at(&document, Position::new(line, column)).map(|entry| match entry {
            TocEntry::Document { docname, .. } => docname.clone(),
            other => format!("{other:?}"),
        })
    }

    const TOCTREE: &str = ".. toctree::\n   :maxdepth: 2\n\n   intro\n   Getting started <setup>\n";

    #[test]
    fn test_toc_entry_at_finds_the_entry_on_the_line() {
        // When / Then
        assert_eq!(entry_at(TOCTREE, 4, 4), Some("intro".to_string()));
        assert_eq!(entry_at(TOCTREE, 5, 4), Some("setup".to_string()));
        assert_eq!(entry_at(TOCTREE, 5, 22), Some("setup".to_string()));
    }

    #[test]
    fn test_toc_entry_at_finds_nothing_on_the_directive_or_an_option() {
        // When / Then
        assert_eq!(entry_at(TOCTREE, 1, 4), None);
        assert_eq!(entry_at(TOCTREE, 2, 6), None);
        assert_eq!(entry_at(TOCTREE, 3, 1), None);
    }

    #[test]
    fn test_toc_entry_at_finds_an_entry_nested_in_a_directive_body() {
        // Given
        let text = ".. note::\n\n   .. toctree::\n\n      intro\n";

        // When / Then
        assert_eq!(entry_at(text, 5, 8), Some("intro".to_string()));
    }

    #[test]
    fn test_toc_entry_at_ignores_an_entry_spliced_in_from_another_file() {
        // Given — an entry's span counting lines in an included fragment
        let mut document = rinx_parser::parse(
            "index.rst",
            ".. toctree::\n\n   intro\n",
            &rinx_parser::ParseCtx::default(),
        )
        .document;
        let Node::Directive(Directive::Toctree(toctree)) = &mut document.nodes[0] else {
            unreachable!()
        };
        let TocEntry::Document { span, .. } = &mut toctree.entries[0] else {
            unreachable!()
        };
        *span = span.map(|span| span.with_file(Some(rinx_ast::FileId::new(0))));

        // When / Then
        assert_eq!(toc_entry_at(&document, Position::new(3, 5)), None);
    }

    #[test]
    fn test_reference_at_ignores_a_reference_spliced_in_from_another_file() {
        // Given — a span counting lines in an included fragment
        let mut document = rinx_parser::parse(
            "index.rst",
            "See :ref:`install`.\n",
            &rinx_parser::ParseCtx::default(),
        )
        .document;
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
