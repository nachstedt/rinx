//! Renders a block quote as `<blockquote>`, matching docutils' HTML5 writer
//! exactly: the quoted content, then — when present — its attribution as
//! `<p class="attribution">`, prefixed with a literal em dash (docutils'
//! default `attribution` setting, `"dash"`).

use std::fmt::Write as _;

use rusty_sphinx_ast::{InlineNode, Node};

use crate::RenderCtx;
use crate::inline::render_inline;

pub(super) fn render_block_quote(
    html: &mut String,
    content: &[Node],
    attribution: Option<&[InlineNode]>,
    ctx: &mut RenderCtx<'_>,
) {
    let _ = writeln!(html, "<blockquote>");
    super::render_nodes(html, content, ctx);
    if let Some(attribution) = attribution {
        let _ = write!(html, "<p class=\"attribution\">\u{2014}");
        for inline in attribution {
            render_inline(html, inline, ctx);
        }
        let _ = writeln!(html, "</p>");
    }
    let _ = writeln!(html, "</blockquote>");
}

#[cfg(test)]
mod tests {
    use rusty_sphinx_ast::Document;
    use rusty_sphinx_index::ProjectIndex;

    use super::*;

    fn render_doc(doc: &Document) -> String {
        let index = ProjectIndex::default();
        crate::render(doc, &index, &doc.path).html
    }

    #[test]
    fn test_render_block_quote_wraps_content_in_a_blockquote_element() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::BlockQuote {
                content: vec![Node::Paragraph(vec![InlineNode::Text(
                    "Quoted text.".to_string(),
                )])],
                attribution: None,
            }],
        );

        // When
        let html = render_doc(&doc);

        // Then
        assert!(html.contains("<blockquote>\n<p>Quoted text.</p>\n</blockquote>"));
        assert!(!html.contains("class=\"attribution\""));
    }

    #[test]
    fn test_render_block_quote_renders_an_attribution_with_an_em_dash_prefix() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::BlockQuote {
                content: vec![Node::Paragraph(vec![InlineNode::Text(
                    "Quoted text.".to_string(),
                )])],
                attribution: Some(vec![InlineNode::Text("Sherlock Holmes".to_string())]),
            }],
        );

        // When
        let html = render_doc(&doc);

        // Then
        assert!(html.contains("<p class=\"attribution\">\u{2014}Sherlock Holmes</p>"));
    }

    #[test]
    fn test_render_block_quote_renders_nested_quotes() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::BlockQuote {
                content: vec![Node::BlockQuote {
                    content: vec![Node::Paragraph(vec![InlineNode::Text(
                        "Inner.".to_string(),
                    )])],
                    attribution: None,
                }],
                attribution: None,
            }],
        );

        // When
        let html = render_doc(&doc);

        // Then — one blockquote nested directly inside the other.
        let outer_start = html.find("<blockquote>").expect("expected a blockquote");
        let inner_start = html[outer_start + 1..].find("<blockquote>");
        assert!(inner_start.is_some());
    }
}
