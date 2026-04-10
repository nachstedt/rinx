//! The parser module converts RST text into an Abstract Syntax Tree (Document).

use crate::ast::{Directive, Document, Node};

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

        // Check if line is a directive
        if line.trim().starts_with(".. ") && line.contains("::") {
            let trimmed = line.trim();
            if let Some((name_part, arg_part)) = trimmed.split_once("::") {
                if let Some(name_inner) = name_part.strip_prefix(".. ") {
                    let name = name_inner.trim().to_string();
                    let argument = arg_part.trim().to_string();

                    let mut body_lines = Vec::new();
                    i += 1;
                    while i < lines.len() {
                        let next_line = lines[i].trim_end();
                        if next_line.trim().is_empty()
                            || next_line.starts_with(' ')
                            || next_line.starts_with('\t')
                        {
                            body_lines.push(next_line);
                        } else {
                            break;
                        }
                        i += 1;
                    }

                    // Remove trailing empty lines
                    while body_lines.last().is_some_and(|l| l.trim().is_empty()) {
                        body_lines.pop();
                    }

                    // Remove leading empty lines
                    let mut start = 0;
                    while start < body_lines.len() && body_lines[start].trim().is_empty() {
                        start += 1;
                    }

                    let directive = if name == "toctree" {
                        let paths = body_lines[start..]
                            .iter()
                            .map(|l| l.trim_start().to_string())
                            .filter(|l| !l.is_empty())
                            .collect();
                        Directive::Toctree { paths }
                    } else {
                        let mut body = String::new();
                        for l in &body_lines[start..] {
                            if !body.is_empty() {
                                body.push('\n');
                            }
                            body.push_str(l.trim_start());
                        }
                        Directive::Unknown {
                            name,
                            argument,
                            body,
                        }
                    };

                    nodes.push(Node::Directive(directive));
                    continue;
                }
            }
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
    fn test_parse_returns_empty_document_for_empty_input() {
        // Given
        let input = "";

        // When
        let doc = parse(input);

        // Then
        assert_eq!(doc.nodes.len(), 0);
    }

    #[test]
    fn test_parse_creates_heading_node() {
        // Given
        let input = "Heading\n=======";

        // When
        let doc = parse(input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(doc.nodes[0], Node::Heading("Heading".to_string()));
    }

    #[test]
    fn test_parse_creates_paragraph_node() {
        // Given
        let input = "Just some\ntext";

        // When
        let doc = parse(input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(doc.nodes[0], Node::Paragraph("Just some\ntext".to_string()));
    }

    #[test]
    fn test_parse_creates_mixed_nodes_for_heading_and_paragraph() {
        // Given
        let input = "Title\n=====\n\nText.";

        // When
        let doc = parse(input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(doc.nodes[0], Node::Heading("Title".to_string()));
        assert_eq!(doc.nodes[1], Node::Paragraph("Text.".to_string()));
    }

    #[test]
    fn test_parse_creates_paragraph_for_shorter_underline() {
        // Given
        let input = "Long Heading\n===";

        // When
        let doc = parse(input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph("Long Heading\n===".to_string())
        );
    }

    #[test]
    fn test_parse_creates_heading_for_alternate_punctuation() {
        // Given
        let input = "Sub Title\n---------";

        // When
        let doc = parse(input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(doc.nodes[0], Node::Heading("Sub Title".to_string()));
    }

    #[test]
    fn test_parse_ignores_surrounding_whitespace_for_heading() {
        // Given
        let input = "Heading  \n  =======  \n\nNext";

        // When
        let doc = parse(input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(doc.nodes[0], Node::Heading("Heading".to_string()));
        assert_eq!(doc.nodes[1], Node::Paragraph("Next".to_string()));
    }

    #[test]
    fn test_parse_creates_multiple_paragraphs_ignoring_blank_lines() {
        // Given
        let input = "Para 1\n\n\nPara 2\n\nPara 3";

        // When
        let doc = parse(input);

        // Then
        assert_eq!(doc.nodes.len(), 3);
        assert_eq!(doc.nodes[0], Node::Paragraph("Para 1".to_string()));
        assert_eq!(doc.nodes[1], Node::Paragraph("Para 2".to_string()));
        assert_eq!(doc.nodes[2], Node::Paragraph("Para 3".to_string()));
    }

    #[test]
    fn test_parse_handles_carriage_returns_gracefully() {
        // Given
        let input = "Heading\r\n=======\r\n\r\nPara\r\nline 2";

        // When
        let doc = parse(input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(doc.nodes[0], Node::Heading("Heading".to_string()));
        assert_eq!(doc.nodes[1], Node::Paragraph("Para\nline 2".to_string()));
    }

    #[test]
    fn test_parse_creates_heading_from_punctuation_lines() {
        // Given
        let input = "===\n---";

        // When
        let doc = parse(input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(doc.nodes[0], Node::Heading("===".to_string()));
    }

    #[test]
    fn test_parse_creates_directive() {
        // Given
        let input = ".. toctree::\n   \n   team_a/index\n   team_b/index\n\nNext Para";

        // When
        let doc = parse(input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Directive(Directive::Toctree {
                paths: vec!["team_a/index".to_string(), "team_b/index".to_string()],
            })
        );
        assert_eq!(doc.nodes[1], Node::Paragraph("Next Para".to_string()));
    }

    #[test]
    fn test_parse_directive_with_argument_and_trailing_indents() {
        // Given
        let input = ".. code-block:: rust\n\n   let x = 1;\n   \n   let y = 2;\n\n";

        // When
        let doc = parse(input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Directive(Directive::Unknown {
                name: "code-block".to_string(),
                argument: "rust".to_string(),
                body: "let x = 1;\n\nlet y = 2;".to_string(),
            })
        );
    }
}
