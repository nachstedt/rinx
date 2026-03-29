//! The parser module converts RST text into an Abstract Syntax Tree (Document).

use crate::ast::{Document, Node};

/// Parses an RST-formatted string into a Document.
///
/// Current logic:
/// - A heading is defined as a line of text followed by a line composed purely
///   of punctuation character(s) (e.g. `===` or `---`) that is at least as long
///   as the text line above it.
/// - Otherwise, consecutive non-blank lines are grouped into a Paragraph.
#[must_use]
pub fn parse(input: &str) -> Document {
    let lines: Vec<&str> = input.lines().collect();
    let mut nodes = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim_end();

        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        // Check if next line is a heading marker
        if i + 1 < lines.len() {
            let next_line = lines[i + 1].trim();
            if !next_line.is_empty()
                && next_line.chars().all(|c| c.is_ascii_punctuation())
                && next_line.len() >= line.trim().len()
            {
                nodes.push(Node::Heading(line.trim().to_string()));
                i += 2;
                continue;
            }
        }

        // Parse as a paragraph
        let mut paragraph_text = String::new();
        while i < lines.len() {
            let current = lines[i].trim_end();
            if current.trim().is_empty() {
                break;
            }

            // Peek at next line to ensure we don't consume a heading's text line
            // as part of the current paragraph
            if i + 1 < lines.len() {
                let peek_next = lines[i + 1].trim();
                let is_heading = !peek_next.is_empty()
                    && peek_next.chars().all(|c| c.is_ascii_punctuation())
                    && peek_next.len() >= current.trim().len();
                if is_heading && !paragraph_text.is_empty() {
                    break;
                }
            }

            if !paragraph_text.is_empty() {
                paragraph_text.push('\n');
            }
            paragraph_text.push_str(current.trim());
            i += 1;
        }

        if !paragraph_text.is_empty() {
            nodes.push(Node::Paragraph(paragraph_text));
        }
    }

    Document::new(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty() {
        let doc = parse("");
        assert_eq!(doc.nodes.len(), 0);
    }

    #[test]
    fn test_parse_heading_only() {
        let doc = parse("Heading\n=======");
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(doc.nodes[0], Node::Heading("Heading".to_string()));
    }

    #[test]
    fn test_parse_paragraph_only() {
        let doc = parse("Just some\ntext");
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(doc.nodes[0], Node::Paragraph("Just some\ntext".to_string()));
    }

    #[test]
    fn test_parse_mixed() {
        let doc = parse("Title\n=====\n\nText.");
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(doc.nodes[0], Node::Heading("Title".to_string()));
        assert_eq!(doc.nodes[1], Node::Paragraph("Text.".to_string()));
    }
}
