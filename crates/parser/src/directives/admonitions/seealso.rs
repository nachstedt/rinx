//! `.. seealso::`.

use crate::diagnostics::Diagnostics;
use crate::directives::body::DirectiveContent;
use crate::directives::options::take_option_block;
use crate::headings::Adornment;
use rinx_ast::Directive;

/// Parses a `.. seealso::` from its content, which may have begun on the
/// marker line.
///
/// Sphinx gives it `:class:` and `:name:`, neither of which this build draws;
/// they are still taken off the content, so that they are not shown as text.
pub(super) fn parse_seealso(
    mut content: DirectiveContent<'_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    take_option_block(&mut content.lines);
    Directive::SeeAlso {
        body: content.parse(adornment_order, diagnostics),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ParseCtx;
    use crate::parse;
    use rinx_ast::{Domain, InlineNode, Node};

    fn only_seealso(input: &str) -> Vec<Node> {
        let doc = parse("test.rst", input);
        let [Node::Directive(Directive::SeeAlso { body })] = &doc.nodes[..] else {
            panic!("Expected one SeeAlso, got {:?}", doc.nodes);
        };
        body.clone()
    }

    #[test]
    fn test_parse_seealso_basic() {
        // Given
        let content = DirectiveContent {
            lines: vec!["See the other page.".to_string()],
            ctx: ParseCtx::with_domain(Domain::Py),
        };
        let mut diagnostics = Diagnostics::default();

        // When
        let directive = parse_seealso(content, &mut Vec::new(), &mut diagnostics);

        // Then
        let Directive::SeeAlso { body } = directive else {
            panic!("Expected SeeAlso directive");
        };
        assert!(matches!(body[..], [Node::Paragraph(_)]));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_seealso_takes_its_options_off_the_content() {
        // Given
        let input = ".. seealso::\n   :class: wide\n\n   Related.\n";

        // When
        let body = only_seealso(input);

        // Then
        assert_eq!(
            body,
            [Node::Paragraph(vec![InlineNode::Text(
                "Related.".to_string()
            )])]
        );
    }

    #[test]
    fn test_parse_creates_seealso_with_paragraph_body() {
        // Given
        let input = ".. seealso::\n\n   Related information here.";

        // When
        let body = only_seealso(input);

        // Then
        assert!(matches!(body[..], [Node::Paragraph(_)]));
    }

    #[test]
    fn test_parse_creates_seealso_with_empty_body() {
        // Given
        let input = ".. seealso::";

        // When
        let body = only_seealso(input);

        // Then
        assert!(body.is_empty());
    }

    #[test]
    fn test_parse_creates_seealso_with_bullet_list_body() {
        // Given
        let input = ".. seealso::\n\n   * Item A\n   * Item B";

        // When
        let body = only_seealso(input);

        // Then
        assert!(matches!(body[..], [Node::BulletList { .. }]));
    }

    #[test]
    fn test_parse_creates_seealso_with_definition_list_body() {
        // Given the CPython benchmark's `curses` seealso block: a definition
        // list where each term carries a :mod:/:ref: role.
        let input = concat!(
            ".. seealso::\n",
            "\n",
            "   Module :mod:`curses.ascii`\n",
            "      Utilities for working with ASCII characters, regardless of your locale settings.\n",
            "\n",
            "   Module :mod:`curses.panel`\n",
            "      A panel stack extension that adds depth to  curses windows.\n",
            "\n",
            "   :ref:`curses-howto`\n",
            "      Tutorial material on using curses with Python, by Andrew Kuchling and Eric\n",
            "      Raymond.",
        );

        // When
        let body = only_seealso(input);

        // Then the body is a single DefinitionList with three term/definition items
        let [Node::DefinitionList { items }] = &body[..] else {
            panic!("Expected DefinitionList, got {body:?}");
        };
        assert_eq!(items.len(), 3);
        assert!(matches!(
            items[0].term[1],
            InlineNode::DomainObjectReference { .. }
        ));
        assert!(matches!(items[2].term[0], InlineNode::Reference { .. }));
    }

    #[test]
    fn test_parse_seealso_keeps_plain_text_on_its_marker_line() {
        // Given
        let input = ".. seealso:: Plain text here\n";

        // When
        let body = only_seealso(input);

        // Then
        assert_eq!(
            body,
            [Node::Paragraph(vec![InlineNode::Text(
                "Plain text here".to_string()
            )])]
        );
    }

    #[test]
    fn test_parse_seealso_keeps_a_role_on_its_marker_line() {
        // Given the shape CPython's `Doc/c-api/typeobj.rst` writes
        let input = ".. seealso:: :pep:`634` -- Structural Pattern Matching: Specification\n";

        // When
        let body = only_seealso(input);

        // Then
        let [Node::Paragraph(inlines)] = &body[..] else {
            panic!("Expected one paragraph, got {body:?}");
        };
        assert!(matches!(inlines[0], InlineNode::PepReference { .. }));
    }

    #[test]
    fn test_parse_seealso_continues_marker_line_text_with_the_body() {
        // Given
        let input = ".. seealso:: First line\n   continued here\n";

        // When
        let body = only_seealso(input);

        // Then
        assert_eq!(
            body,
            [Node::Paragraph(vec![InlineNode::Text(
                "First line\ncontinued here".to_string()
            )])]
        );
    }

    #[test]
    fn test_parse_seealso_follows_marker_line_text_with_a_body_paragraph() {
        // Given
        let input = ".. seealso:: First\n\n   Second paragraph.\n";

        // When
        let body = only_seealso(input);

        // Then
        assert_eq!(body.len(), 2);
    }

    #[test]
    fn test_parse_seealso_places_a_body_diagnostic_on_its_line() {
        // Given an unknown directive two lines below marker-line content
        let input = ".. seealso:: First\n\n   .. bogus::\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        let lines: Vec<u32> = doc
            .diagnostics
            .iter()
            .filter_map(|d| d.span.map(|span| span.start.line))
            .collect();
        assert_eq!(lines, [3]);
    }
}
