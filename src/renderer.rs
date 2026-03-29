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
                let _ = writeln!(html, "<h1>{text}</h1>");
            }
            Node::Paragraph(text) => {
                let _ = writeln!(html, "<p>{text}</p>");
            }
        }
    }

    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_empty() {
        let doc = Document::new(vec![]);
        let index = ProjectIndex {};
        let result = render(&doc, &index);
        assert_eq!(result, "");
    }

    #[test]
    fn test_render_nodes() {
        let doc = Document::new(vec![
            Node::Heading("Title".to_string()),
            Node::Paragraph("Paragraph".to_string()),
        ]);
        let index = ProjectIndex {};
        let result = render(&doc, &index);
        assert_eq!(result, "<h1>Title</h1>\n<p>Paragraph</p>\n");
    }
}
