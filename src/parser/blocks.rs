use super::bullet_list::try_parse_bullet_list;
use super::directives::try_parse_directive;
use super::headings::{Adornment, detect_adornment, try_parse_heading};
use super::inline::parse_inline_text;
use crate::ast::{Document, Node, TargetName};

/// Parses an RST-formatted string into a Document.
///
/// Current logic:
/// - A heading is defined as a line of text followed by a line composed purely
///   of punctuation character(s) (e.g. `===` or `---`) that is at least as long
///   as the text line above it.
/// - Heading levels are determined by the order in which each underline character
///   is first encountered in the document: the first character seen becomes level
///   1, the second distinct character level 2, etc.
/// - Otherwise, consecutive non-blank lines are grouped into a Paragraph.
///
/// # Panics
///
/// The internal implementation uses `expect()` on an iterator that is guaranteed
/// to be non-empty by preceding checks.
#[must_use]
pub fn parse(path: &str, input: &str) -> Document {
    let lines: Vec<&str> = input.lines().collect();
    let mut adornment_order: Vec<Adornment> = Vec::new();
    let mut diagnostics = Vec::new();

    let nodes = parse_blocks(&lines, &mut adornment_order, &mut diagnostics);

    if !diagnostics.is_empty() {
        eprintln!("Diagnostics for '{path}':");
        for diag in &diagnostics {
            eprintln!("  - {diag}");
        }
    }

    let mut doc = Document::new(path.to_string(), nodes);
    doc.diagnostics = diagnostics;
    doc
}

pub(super) fn try_parse_target(lines: &[&str], i: usize) -> Option<(usize, Node)> {
    let line = lines[i].trim();
    if let Some(rest) = line.strip_prefix(".. __:") {
        let mut uri = rest.trim().to_string();
        let mut consumed = 1;

        if uri.is_empty() && i + 1 < lines.len() {
            let next_line = lines[i + 1];
            if next_line.starts_with(' ') || next_line.starts_with('\t') {
                uri = next_line.trim().to_string();
                consumed = 2;
            }
        }

        if !uri.is_empty() {
            return Some((consumed, Node::AnonymousTarget { uri }));
        }
    }

    if !line.starts_with(".. _") {
        return None;
    }

    // Try to find the colon that ends the target name
    if let Some(colon_pos) = line[4..].find(':') {
        let absolute_colon_pos = 4 + colon_pos;
        let name_str = &line[4..absolute_colon_pos].trim();
        if name_str.is_empty() {
            return None;
        }

        let mut uri = line[absolute_colon_pos + 1..].trim().to_string();
        let mut consumed = 1;

        // If URI is empty on the same line, check the next line for an indented block
        if uri.is_empty() && i + 1 < lines.len() {
            let next_line = lines[i + 1];
            if next_line.starts_with(' ') || next_line.starts_with('\t') {
                uri = next_line.trim().to_string();
                consumed = 2;
            }
        }

        let uri_opt = if uri.is_empty() { None } else { Some(uri) };

        return Some((
            consumed,
            Node::Target {
                name: TargetName::new(name_str),
                uri: uri_opt,
            },
        ));
    }

    None
}

pub(super) fn parse_blocks(
    lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim_end();

        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        if let Some((consumed, node)) = try_parse_directive(lines, i, adornment_order, diagnostics)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) = try_parse_target(lines, i) {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) = try_parse_heading(lines, i, adornment_order) {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) =
            try_parse_bullet_list(lines, i, adornment_order, diagnostics)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        let (consumed, new_nodes) = parse_paragraph(lines, i);
        nodes.extend(new_nodes);
        i += consumed;
    }
    nodes
}

pub(super) fn collect_directive_body<'a>(
    lines: &[&'a str],
    start_index: usize,
) -> (usize, Vec<&'a str>) {
    let mut body_lines = Vec::new();
    let mut current = start_index;
    while current < lines.len() {
        let next_line = lines[current].trim_end();
        if next_line.trim().is_empty() || next_line.starts_with(' ') || next_line.starts_with('\t')
        {
            body_lines.push(next_line);
        } else {
            break;
        }
        current += 1;
    }

    let consumed = current - start_index;

    // Remove trailing empty lines
    while body_lines.last().is_some_and(|l| l.trim().is_empty()) {
        body_lines.pop();
    }

    let mut start = 0;
    while start < body_lines.len() && body_lines[start].trim().is_empty() {
        start += 1;
    }

    (consumed, body_lines[start..].to_vec())
}

pub(super) fn join_body_lines(body_lines: &[&str]) -> String {
    let mut body = String::new();
    for l in body_lines {
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(l.trim_start());
    }
    body
}

/// Collects a literal block body starting at `start_index`.
///
/// - Skips a leading blank line (mandatory after `::`).
/// - Collects contiguous lines until indentation drops to (or below) the base level.
/// - Strips the *minimum* common indentation from all non-blank lines, preserving relative
///   indentation within the block (RST spec behaviour).
///
/// Returns `(lines_consumed, verbatim_content)`.
pub(super) fn collect_literal_block_body(lines: &[&str], start_index: usize) -> (usize, String) {
    let mut current = start_index;

    // Skip the mandatory blank line after `::`
    if current < lines.len() && lines[current].trim().is_empty() {
        current += 1;
    }

    // Find the first non-blank line to establish the base indentation level
    let Some(first_non_blank) = lines[current..].iter().find(|l| !l.trim().is_empty()) else {
        return (current - start_index, String::new());
    };
    let base_indent = first_non_blank
        .chars()
        .take_while(|c| c.is_whitespace())
        .count();

    if base_indent == 0 {
        // No indented block follows
        return (current - start_index, String::new());
    }

    // Collect lines that belong to the block; blank lines are kept as separators
    let mut body_lines: Vec<&str> = Vec::new();
    while current < lines.len() {
        let line = lines[current];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            body_lines.push("");
            current += 1;
            continue;
        }
        let indent = line.chars().take_while(|c| c.is_whitespace()).count();
        if indent < base_indent {
            break;
        }
        body_lines.push(line);
        current += 1;
    }

    // Remove trailing blank lines
    while body_lines.last().is_some_and(|l| l.trim().is_empty()) {
        body_lines.pop();
    }

    // Compute the minimum indentation of all non-blank lines
    let min_indent = body_lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.chars().take_while(|c| c.is_whitespace()).count())
        .min()
        .unwrap_or(0);

    // Strip the common indent; leave blank lines as empty strings
    let content = body_lines
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                String::new()
            } else {
                l.chars().skip(min_indent).collect()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    (current - start_index, content)
}

fn parse_paragraph(lines: &[&str], i: usize) -> (usize, Vec<Node>) {
    let mut paragraph_text = String::new();
    let mut current_pos_line = i;

    while current_pos_line < lines.len() {
        let line = lines[current_pos_line].trim_end();
        if line.trim().is_empty() {
            break;
        }

        if !paragraph_text.is_empty() {
            paragraph_text.push('\n');
        }
        paragraph_text.push_str(line.trim());
        current_pos_line += 1;

        // Peek at next line to ensure we don't consume a heading's text line or overline
        if current_pos_line < lines.len() && detect_adornment(lines, current_pos_line).is_some() {
            break;
        }
    }

    let inlines = parse_inline_text(&paragraph_text);

    // Detect trailing "::" to introduce a literal block
    let trailing_double_colon = paragraph_text.trim_end().ends_with("::");
    if trailing_double_colon {
        let trimmed = paragraph_text.trim_end();
        // Determine whether the whole paragraph is just "::" (standalone introducer)
        let only_colon = trimmed.trim() == "::";
        let lit_start = current_pos_line;
        let (lit_consumed, content) = collect_literal_block_body(lines, lit_start);
        let total_consumed = current_pos_line - i + lit_consumed;
        let literal_node = Node::LiteralBlock {
            language: None,
            content,
        };
        if only_colon {
            // The "::" line itself is suppressed; emit only the literal block
            return (total_consumed, vec![literal_node]);
        }
        // Strip trailing "::" → ":" and emit paragraph + literal block
        let stripped = trimmed[..trimmed.len() - 1].trim_end().to_string();
        let inlines = parse_inline_text(&stripped);
        return (total_consumed, vec![Node::Paragraph(inlines), literal_node]);
    }

    (current_pos_line - i, vec![Node::Paragraph(inlines)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::InlineNode;

    #[test]
    fn test_parse_blocks_empty_input() {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();
        let nodes = parse_blocks(&[], &mut adornment_order, &mut diagnostics);
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_parse_blocks_simple_paragraph() {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();
        let lines = vec!["Hello world"];
        let nodes = parse_blocks(&lines, &mut adornment_order, &mut diagnostics);
        assert_eq!(nodes.len(), 1);
        match &nodes[0] {
            Node::Paragraph(inlines) => {
                assert_eq!(inlines.len(), 1);
                assert_eq!(inlines[0], InlineNode::Text("Hello world".to_string()));
            }
            _ => panic!("Expected paragraph"),
        }
    }

    #[test]
    fn test_parse_blocks_maintains_adornment_order() {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        let lines1 = vec!["Title 1", "======="];
        let nodes1 = parse_blocks(&lines1, &mut adornment_order, &mut diagnostics);
        assert_eq!(nodes1.len(), 1);

        let lines2 = vec!["Title 2", "-------"];
        let nodes2 = parse_blocks(&lines2, &mut adornment_order, &mut diagnostics);
        assert_eq!(nodes2.len(), 1);

        assert_eq!(adornment_order.len(), 2);
    }

    #[test]
    fn test_parse_blocks_collects_diagnostics() {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // This will trigger a diagnostic because of the unknown option
        let lines = vec![".. toctree::", "   :unknown_option: value"];
        let nodes = parse_blocks(&lines, &mut adornment_order, &mut diagnostics);

        assert_eq!(nodes.len(), 1);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].contains("Invalid or non-standard Sphinx toctree option"));
    }

    #[test]
    fn test_join_body_lines_with_empty_input() {
        // Given
        let input: &[&str] = &[];
        // When
        let result = join_body_lines(input);
        // Then
        assert_eq!(result, "");
    }

    #[test]
    fn test_join_body_lines_with_single_line() {
        // Given
        let input = vec!["   hello"];
        // When
        let result = join_body_lines(&input);
        // Then
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_join_body_lines_with_multiple_lines() {
        // Given
        let input = vec!["   line1", "  line2", "line3"];
        // When
        let result = join_body_lines(&input);
        // Then
        assert_eq!(result, "line1\nline2\nline3");
    }

    #[test]
    fn test_collect_directive_body_collects_indented_lines() {
        // Given
        let lines = vec![".. note::", "   body1", "   body2"];
        // When
        let (consumed, body) = collect_directive_body(&lines, 1);
        // Then
        assert_eq!(consumed, 2);
        assert_eq!(body, vec!["   body1", "   body2"]);
    }

    #[test]
    fn test_collect_directive_body_stops_at_unindented_line() {
        // Given
        let lines = vec![".. note::", "   body1", "unindented", "   body2"];
        // When
        let (consumed, body) = collect_directive_body(&lines, 1);
        // Then
        assert_eq!(consumed, 1);
        assert_eq!(body, vec!["   body1"]);
    }

    #[test]
    fn test_collect_directive_body_strips_leading_and_trailing_blank_lines() {
        // Given
        let lines = vec![".. note::", "  ", "   body1", "  ", "   body2", "   ", ""];
        // When
        let (consumed, body) = collect_directive_body(&lines, 1);
        // Then
        assert_eq!(consumed, 6);
        assert_eq!(body, vec!["   body1", "", "   body2"]);
    }

    #[test]
    fn test_collect_directive_body_returns_empty_when_no_body() {
        // Given
        let lines = vec![".. note::", "unindented"];
        // When
        let (consumed, body) = collect_directive_body(&lines, 1);
        // Then
        assert_eq!(consumed, 0);
        assert!(body.is_empty());
    }

    #[test]
    fn test_collect_directive_body_returns_correct_consumed_count() {
        // Given
        let lines = vec![".. note::", "   body", "  "];
        // When
        let (consumed, body) = collect_directive_body(&lines, 1);
        // Then
        assert_eq!(consumed, 2);
        assert_eq!(body, vec!["   body"]);
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::ast::TargetName;
    use crate::ast::{InlineNode, Node};
    use crate::parser::parse;

    #[test]
    fn test_parse_returns_empty_document_for_empty_input() {
        // Given
        let input = "";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 0);
    }

    #[test]
    fn test_parse_creates_paragraph_node() {
        // Given
        let input = "Just some\ntext";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![InlineNode::Text("Just some\ntext".to_string())])
        );
    }

    #[test]
    fn test_parse_creates_mixed_nodes_for_heading_and_paragraph() {
        // Given
        let input = "Title\n=====\n\nText.";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Heading {
                level: 1,
                text: "Title".to_string()
            }
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![InlineNode::Text("Text.".to_string())])
        );
    }

    #[test]
    fn test_parse_creates_paragraph_for_shorter_underline() {
        // Given
        let input = "Long Heading\n===";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![InlineNode::Text("Long Heading\n===".to_string())])
        );
    }

    #[test]
    fn test_parse_ignores_surrounding_whitespace_for_heading() {
        // Given
        let input = "Heading  \n  =======  \n\nNext";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Heading {
                level: 1,
                text: "Heading".to_string()
            }
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![InlineNode::Text("Next".to_string())])
        );
    }

    #[test]
    fn test_parse_creates_multiple_paragraphs_ignoring_blank_lines() {
        // Given
        let input = "Para 1\n\n\nPara 2\n\nPara 3";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 3);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![InlineNode::Text("Para 1".to_string())])
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![InlineNode::Text("Para 2".to_string())])
        );
        assert_eq!(
            doc.nodes[2],
            Node::Paragraph(vec![InlineNode::Text("Para 3".to_string())])
        );
    }

    #[test]
    fn test_parse_handles_carriage_returns_gracefully() {
        // Given
        let input = "Heading\r\n=======\r\n\r\nPara\r\nline 2";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Heading {
                level: 1,
                text: "Heading".to_string()
            }
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![InlineNode::Text("Para\nline 2".to_string())])
        );
    }

    #[test]
    fn test_parse_creates_target_node_for_explicit_target() {
        // Given
        let input = ".. _my-target:\n\nSome text.";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Target {
                name: TargetName::new("my-target"),
                uri: None
            }
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![InlineNode::Text("Some text.".to_string())])
        );
    }

    #[test]
    fn test_parse_creates_external_target_node() {
        // Given
        let input = ".. _my-link: https://example.com\n\nSome text.";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Target {
                name: TargetName::new("my-link"),
                uri: Some("https://example.com".to_string())
            }
        );
    }

    #[test]
    fn test_parse_creates_indented_external_target_node() {
        // Given
        let input = ".. _my-link:\n   https://example.com\n\nSome text.";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Target {
                name: TargetName::new("my-link"),
                uri: Some("https://example.com".to_string())
            }
        );
    }

    #[test]
    fn test_parse_paragraph_breaks_at_overline() {
        // Given
        let input = "Para text.\n#######\nHeading\n#######";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![InlineNode::Text("Para text.".to_string())])
        );
        assert_eq!(
            doc.nodes[1],
            Node::Heading {
                level: 1,
                text: "Heading".to_string()
            }
        );
    }

    #[test]
    fn test_parse_double_colon_paragraph_emits_literal_block() {
        // Given: a paragraph ending with :: followed by an indented block
        let input = "Here is some code::\n\n    def hello():\n        pass\n";
        // When
        let doc = parse("test.rst", input);
        // Then: two nodes — paragraph (with :: reduced to :) and LiteralBlock
        assert_eq!(doc.nodes.len(), 2);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            let text = match &inlines[0] {
                InlineNode::Text(t) => t.as_str(),
                other => panic!("Expected Text inline, got {other:?}"),
            };
            assert_eq!(text, "Here is some code:");
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
        if let Node::LiteralBlock { language, content } = &doc.nodes[1] {
            assert!(language.is_none());
            assert_eq!(content, "def hello():\n    pass");
        } else {
            panic!("Expected LiteralBlock, got {:?}", doc.nodes[1]);
        }
    }

    #[test]
    fn test_parse_standalone_double_colon_suppresses_paragraph() {
        // Given: a line of only "::" introduces a literal block with no visible paragraph
        let input = "::\n\n    verbatim content\n";
        // When
        let doc = parse("test.rst", input);
        // Then: only the LiteralBlock is emitted (no paragraph)
        assert_eq!(doc.nodes.len(), 1);
        if let Node::LiteralBlock { language, content } = &doc.nodes[0] {
            assert!(language.is_none());
            assert_eq!(content, "verbatim content");
        } else {
            panic!("Expected LiteralBlock, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_double_colon_strips_to_single_colon() {
        // Given: text followed by "::" — the "::" becomes ":"
        let input = "Example::\n\n    content\n";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 2);
        if let Node::Paragraph(inlines) = &doc.nodes[0] {
            let text = match &inlines[0] {
                InlineNode::Text(t) => t.as_str(),
                other => panic!("Expected Text, got {other:?}"),
            };
            assert_eq!(text, "Example:");
        } else {
            panic!("Expected Paragraph, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_literal_block_preserves_internal_blank_lines() {
        // Given: blank lines inside the block must be kept
        let input = "Example::\n\n    line one\n\n    line three\n";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 2);
        if let Node::LiteralBlock { content, .. } = &doc.nodes[1] {
            assert_eq!(content, "line one\n\nline three");
        } else {
            panic!("Expected LiteralBlock, got {:?}", doc.nodes[1]);
        }
    }

    #[test]
    fn test_parse_literal_block_strips_common_indentation() {
        // Given: all lines indented 4 spaces; inner block adds 4 more
        let input = "Example::\n\n    outer\n        inner\n    outer again\n";
        // When
        let doc = parse("test.rst", input);
        // Then: 4 spaces stripped from all lines; inner keeps its extra 4
        assert_eq!(doc.nodes.len(), 2);
        if let Node::LiteralBlock { content, .. } = &doc.nodes[1] {
            assert_eq!(content, "outer\n    inner\nouter again");
        } else {
            panic!("Expected LiteralBlock, got {:?}", doc.nodes[1]);
        }
    }
}
