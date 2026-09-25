//! Renders a line block as nested `<div class="line-block">`/`<div
//! class="line">` elements, matching docutils' HTML5 writer exactly: a
//! [`LineBlockItem::Nested`] run becomes a `<div class="line-block">` nested
//! directly inside its parent's, and an empty [`LineBlockItem::Line`] (a
//! bare `|` in the source) renders a bare `<br />` so it still occupies a
//! line of vertical space.

use std::fmt::Write as _;

use rinx_ast::LineBlockItem;

use crate::RenderCtx;
use crate::inline::render_inline;

pub(super) fn render_line_block(
    html: &mut String,
    items: &[LineBlockItem],
    ctx: &mut RenderCtx<'_>,
) {
    let _ = writeln!(html, "<div class=\"line-block\">");
    for item in items {
        match item {
            LineBlockItem::Line(inlines) => {
                let _ = write!(html, "<div class=\"line\">");
                if inlines.is_empty() {
                    let _ = write!(html, "<br />");
                } else {
                    for inline in inlines {
                        render_inline(html, inline, ctx);
                    }
                }
                let _ = writeln!(html, "</div>");
            }
            LineBlockItem::Nested(nested) => render_line_block(html, nested, ctx),
        }
    }
    let _ = writeln!(html, "</div>");
}

#[cfg(test)]
mod tests {
    use rinx_ast::{Document, InlineNode, Node};
    use rinx_index::ProjectIndex;

    use super::*;

    fn render_doc(doc: &Document) -> String {
        let index = ProjectIndex::default();
        crate::render(doc, &index, &doc.path).html
    }

    #[test]
    fn test_render_line_block_wraps_lines_in_a_line_block_div() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::LineBlock(vec![LineBlockItem::Line(vec![
                InlineNode::Text("One line.".to_string()),
            ])])],
        );

        // When
        let html = render_doc(&doc);

        // Then
        assert!(
            html.contains(
                "<div class=\"line-block\">\n<div class=\"line\">One line.</div>\n</div>"
            )
        );
    }

    #[test]
    fn test_render_line_block_renders_an_empty_line_as_a_line_break() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::LineBlock(vec![LineBlockItem::Line(vec![])])],
        );

        // When
        let html = render_doc(&doc);

        // Then
        assert!(html.contains("<div class=\"line\"><br /></div>"));
    }

    #[test]
    fn test_render_line_block_nests_a_nested_item_as_a_nested_div() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::LineBlock(vec![LineBlockItem::Nested(vec![
                LineBlockItem::Line(vec![InlineNode::Text("Nested.".to_string())]),
            ])])],
        );

        // When
        let html = render_doc(&doc);

        // Then — one line-block nested directly inside the other.
        let outer_start = html.find("<div class=\"line-block\">").expect("outer div");
        let inner_start = html[outer_start + 1..].find("<div class=\"line-block\">");
        assert!(inner_start.is_some());
    }

    #[test]
    fn test_render_line_block_renders_inline_markup_in_a_line() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::LineBlock(vec![LineBlockItem::Line(vec![
                InlineNode::Emphasis("stressed".to_string()),
            ])])],
        );

        // When
        let html = render_doc(&doc);

        // Then
        assert!(html.contains("<div class=\"line\"><em>stressed</em></div>"));
    }
}
