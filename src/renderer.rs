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
            Node::Heading { level, text } => {
                let tag = format!("h{}", (*level).clamp(1, 6));
                let escaped_text = html_escape::encode_text(text);
                let _ = writeln!(html, "<{tag}>{escaped_text}</{tag}>");
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
            Node::Heading {
                level: 1,
                text: "Title".to_string(),
            },
            Node::Paragraph("Paragraph".to_string()),
            Node::Heading {
                level: 1,
                text: "Another Heading".to_string(),
            },
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
            Node::Heading {
                level: 1,
                text: "Title <script>".to_string(),
            },
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

    #[test]
    fn test_render_formats_heading_level_1_as_h1() {
        // Given
        let doc = Document::new(vec![Node::Heading {
            level: 1,
            text: "Top".to_string(),
        }]);
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index);

        // Then
        assert_eq!(result, "<h1>Top</h1>\n");
    }

    #[test]
    fn test_render_formats_heading_level_2_as_h2() {
        // Given
        let doc = Document::new(vec![Node::Heading {
            level: 2,
            text: "Sub".to_string(),
        }]);
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index);

        // Then
        assert_eq!(result, "<h2>Sub</h2>\n");
    }

    #[test]
    fn test_render_formats_heading_level_6_as_h6() {
        // Given
        let doc = Document::new(vec![Node::Heading {
            level: 6,
            text: "Deep".to_string(),
        }]);
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index);

        // Then
        assert_eq!(result, "<h6>Deep</h6>\n");
    }

    #[test]
    fn test_render_clamps_heading_level_above_6_to_h6() {
        // Given — level 7 exceeds the HTML maximum of 6
        let doc = Document::new(vec![Node::Heading {
            level: 7,
            text: "VeryDeep".to_string(),
        }]);
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index);

        // Then
        assert_eq!(result, "<h6>VeryDeep</h6>\n");
    }
}
