//! Assigning equation numbers to the labeled `.. math::` blocks of one
//! document.
//!
//! Kept as its own document-order pass rather than an arm inside
//! [`super::document_index::index_nodes`] for two reasons. Numbering is
//! inherently sequential, so folding it in would mean threading a mutable
//! counter through that recursive matcher purely for this one variant; and
//! `index_nodes` ends in a catch-all `_ => {}`, so a `Directive` arm there is
//! the one kind of registration the compiler cannot check you remembered to
//! write.
//!
//! Only labeled equations are numbered, and the count restarts in every
//! document — Sphinx's behaviour with `math_number_all` and `math_numfig`
//! both off, which are the only settings rusty-sphinx supports.

use rusty_sphinx_ast::{Directive, Document, Node, TargetName, walk_nodes};

/// Numbers the labeled `.. math::` blocks of `doc`, in document order,
/// starting at 1.
///
/// Unlabeled equations are skipped entirely: they neither receive a number nor
/// advance the counter, so `(1)` and `(2)` stay adjacent to the reader even
/// when an unnumbered equation sits between them.
///
/// A `:nowrap:` equation is skipped for the same reason it renders without a
/// number: the author has taken over the LaTeX environment, so any numbering
/// is theirs to write. Real Sphinx returns before numbering for exactly this
/// case.
///
/// A label used twice in one document yields two entries; the caller's map
/// keeps the last, matching how every other duplicate target is resolved.
pub(super) fn number_equations(doc: &Document) -> Vec<(TargetName, usize)> {
    let mut numbered = Vec::new();
    let mut counter = 0;
    walk_nodes(&doc.nodes, &mut |node| {
        if let Node::Directive(Directive::Math {
            label: Some(label),
            nowrap: false,
            ..
        }) = node
        {
            counter += 1;
            numbered.push((label.clone(), counter));
        }
    });
    numbered
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{AdmonitionKind, InlineNode};

    fn math(label: Option<&str>, nowrap: bool) -> Node {
        Node::Directive(Directive::Math {
            parts: vec!["x = y".to_string()],
            label: label.map(TargetName::new),
            nowrap,
            classes: Vec::new(),
            span: None,
        })
    }

    #[test]
    fn test_number_equations_numbers_labeled_equations_in_order() {
        // Given two labeled equations
        let doc = Document::new(
            "math.rst".to_string(),
            vec![math(Some("first"), false), math(Some("second"), false)],
        );

        // When
        let numbered = number_equations(&doc);

        // Then they are numbered from 1 in document order
        assert_eq!(
            numbered,
            vec![
                (TargetName::new("first"), 1),
                (TargetName::new("second"), 2),
            ]
        );
    }

    #[test]
    fn test_number_equations_skips_unlabeled_equations_without_advancing() {
        // Given a labeled equation on either side of an unlabeled one
        let doc = Document::new(
            "math.rst".to_string(),
            vec![
                math(Some("first"), false),
                math(None, false),
                math(Some("second"), false),
            ],
        );

        // When
        let numbered = number_equations(&doc);

        // Then the numbers stay adjacent — the unlabeled equation is invisible
        assert_eq!(
            numbered,
            vec![
                (TargetName::new("first"), 1),
                (TargetName::new("second"), 2),
            ]
        );
    }

    #[test]
    fn test_number_equations_skips_a_nowrap_equation() {
        // Given a labeled `:nowrap:` equation before an ordinary one
        let doc = Document::new(
            "math.rst".to_string(),
            vec![math(Some("raw"), true), math(Some("numbered"), false)],
        );

        // When
        let numbered = number_equations(&doc);

        // Then only the ordinary one is numbered, and it is (1)
        assert_eq!(numbered, vec![(TargetName::new("numbered"), 1)]);
    }

    #[test]
    fn test_number_equations_counts_an_equation_nested_in_a_directive() {
        // Given an equation inside an admonition, between two top-level ones
        let doc = Document::new(
            "math.rst".to_string(),
            vec![
                math(Some("before"), false),
                Node::Directive(Directive::Admonition {
                    kind: AdmonitionKind::Note,
                    title: None,
                    collapsible: None,
                    body: vec![math(Some("nested"), false)],
                }),
                math(Some("after"), false),
            ],
        );

        // When
        let numbered = number_equations(&doc);

        // Then nesting doesn't exempt it — numbering follows document order
        assert_eq!(
            numbered,
            vec![
                (TargetName::new("before"), 1),
                (TargetName::new("nested"), 2),
                (TargetName::new("after"), 3),
            ]
        );
    }

    #[test]
    fn test_number_equations_returns_nothing_for_a_document_without_math() {
        // Given a document with no equations
        let doc = Document::new(
            "prose.rst".to_string(),
            vec![Node::Paragraph(vec![InlineNode::Text("Hi".to_string())])],
        );

        // When
        let numbered = number_equations(&doc);

        // Then
        assert!(numbered.is_empty());
    }
}
