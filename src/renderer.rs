//! The renderer module converts the AST and `ProjectIndex` into HTML.

use crate::analyzer::ProjectIndex;
use crate::ast::{Directive, Document, Node};
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
            Node::Directive(directive) => match directive {
                Directive::Toctree { paths } => {
                    let _ = writeln!(html, "<ul>");
                    for path in paths {
                        let href = format!("{path}.html");
                        let escaped_text = html_escape::encode_text(path);
                        let _ = writeln!(html, "  <li><a href=\"{href}\">{escaped_text}</a></li>");
                    }
                    let _ = writeln!(html, "</ul>");
                }
                Directive::Unknown { .. } => {}
            },
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

    #[test]
    fn test_render_formats_toctree_as_html_list() {
        // Given
        let doc = Document::new(vec![Node::Directive(Directive::Toctree {
            paths: vec!["team_a/index".to_string(), "team_b/index".to_string()],
        })]);
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index);

        // Then
        assert_eq!(
            result,
            "<ul>\n  <li><a href=\"team_a/index.html\">team_a/index</a></li>\n  <li><a href=\"team_b/index.html\">team_b/index</a></li>\n</ul>\n"
        );
    }
}
