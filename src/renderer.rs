//! The renderer module converts the AST and `ProjectIndex` into HTML.

use crate::analyzer::ProjectIndex;
use crate::ast::{Document, Node};
use std::fmt::Write as _;

/// Renders a Document into an HTML string.
#[must_use]
pub fn render(doc: &Document, _index: &ProjectIndex) -> String {
    let mut html = String::new();

    for node in &doc.nodes {
        match node {
            Node::Heading(text) => {
                let escaped_text = html_escape::encode_text(text);
                let _ = writeln!(html, "<h1>{escaped_text}</h1>");
            }
            Node::Paragraph(text) => {
                let escaped_text = html_escape::encode_text(text);
                let _ = writeln!(html, "<p>{escaped_text}</p>");
            }
        }
    }

    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_returns_empty_string_for_empty_document() {
        // Given
        let doc = Document::new(vec![]);
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index);

        // Then
        assert_eq!(result, "");
    }

    #[test]
    fn test_render_formats_heading_and_paragraph_nodes() {
        // Given
        let doc = Document::new(vec![
            Node::Heading("Title".to_string()),
            Node::Paragraph("Paragraph".to_string()),
            Node::Heading("Another Heading".to_string()),
        ]);
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index);

        // Then
        assert_eq!(
            result,
            "<h1>Title</h1>\n<p>Paragraph</p>\n<h1>Another Heading</h1>\n"
        );
    }

    #[test]
    fn test_render_escapes_html_special_characters() {
        // Given
        let doc = Document::new(vec![
            Node::Heading("Title <script>".to_string()),
            Node::Paragraph("A & B > C".to_string()),
        ]);
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index);

        // Then
        assert_eq!(
            result,
            "<h1>Title &lt;script&gt;</h1>\n<p>A &amp; B &gt; C</p>\n"
        );
    }
}
