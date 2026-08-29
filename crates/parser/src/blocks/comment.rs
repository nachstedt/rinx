use crate::directives::body::collect_directive_body;
use crate::indent::indent_width;
use rusty_sphinx_ast::Node;

/// Tries to parse an RST comment starting at line `i`.
///
/// A comment is any RST explicit markup block that is not a directive, target, or
/// substitution definition. The **explicit markup start** is strictly either:
///
/// - Exactly `..` (bare, nothing after), or
/// - `.. ` (two dots followed by at least one space, then optional inline text)
///
/// This means `...` or `....` or `... text` are **not** explicit markup starts and
/// must not be matched — they are ordinary text (e.g. heading text or paragraph text).
///
/// Three comment forms are recognised:
///
/// - `.. inline text` — optional indented continuation body may follow
/// - `..` (bare) followed by an indented body
/// - `..` (bare) with no body at all
///
/// The comment body is consumed but discarded; only [`Node::Comment`] is returned.
pub(super) fn try_parse_comment(lines: &[&str], i: usize) -> Option<(usize, Node)> {
    let line = lines[i].trim_end();
    let trimmed = line.trim();

    // RST explicit markup start: exactly ".." or ".. " (dot dot space).
    // Three or more dots (e.g. "...") are NOT an explicit markup start.
    if trimmed != ".." && !trimmed.starts_with(".. ") {
        return None;
    }
    // Directives require "::" somewhere on the intro line.
    if line.contains("::") {
        return None;
    }
    // Targets start with ".. _".
    if trimmed.starts_with(".. _") {
        return None;
    }

    // Consume the intro line and any indented body that follows.
    let (body_consumed, _body) = collect_directive_body(lines, i + 1, indent_width(line));
    let consumed = 1 + body_consumed;

    Some((consumed, Node::Comment))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- try_parse_comment unit tests ---

    #[test]
    fn test_try_parse_comment_returns_none_for_plain_text() {
        // Given
        let lines = vec!["Hello world"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_comment_single_line() {
        // Given
        let lines = vec![".. a comment"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert_eq!(result, Some((1, Node::Comment)));
    }

    #[test]
    fn test_try_parse_comment_bare_dots() {
        // Given
        let lines = vec![".."];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert_eq!(result, Some((1, Node::Comment)));
    }

    #[test]
    fn test_try_parse_comment_with_inline_text_and_body() {
        // Given
        let lines = vec![".. comment text", "   continuation line"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert_eq!(result, Some((2, Node::Comment)));
    }

    #[test]
    fn test_try_parse_comment_bare_dots_with_body() {
        // Given
        let lines = vec!["..", "   indented body"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert_eq!(result, Some((2, Node::Comment)));
    }

    #[test]
    fn test_try_parse_comment_returns_none_for_three_dots() {
        // Given — "..." is not an RST explicit markup start
        let lines = vec!["..."];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_comment_returns_none_for_ellipsis_text() {
        // Given — "... some text" starts with two dots but the third char is not a space
        let lines = vec!["... some text"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_comment_returns_none_for_ellipsis_heading_text() {
        // Given — the exact reported regression: heading text beginning with "..."
        let lines = vec!["... install scientific Python packages?"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_comment_does_not_match_directive() {
        // Given — directive lines contain "::" and are handled by try_parse_directive
        let lines = vec![".. note::"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_comment_does_not_match_named_target() {
        // Given — named targets start with ".. _"
        let lines = vec![".. _label:"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_comment_does_not_match_anonymous_target() {
        // Given — anonymous targets start with ".. __:"
        let lines = vec![".. __: https://example.com"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_comment_multiline_body_consumes_all_indented_lines() {
        // Given
        let lines = vec!["..", "   line one", "   line two", "Not part of comment"];
        // When
        let result = try_parse_comment(&lines, 0);
        // Then
        assert_eq!(result, Some((3, Node::Comment)));
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::parse;
    use rusty_sphinx_ast::{InlineNode, Node};

    // --- Comment integration tests ---

    #[test]
    fn test_parse_comment_produces_single_comment_node() {
        // Given
        let input = ".. This is a comment";
        // When
        let doc = parse("test.rst", input);
        // Then — the document contains exactly one Comment node
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(doc.nodes[0], Node::Comment);
    }

    #[test]
    fn test_parse_bare_comment_marker_produces_comment_node() {
        // Given — bare `..` with no following text or body
        let input = "..";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(doc.nodes[0], Node::Comment);
    }

    #[test]
    fn test_parse_multiline_comment_produces_single_comment_node() {
        // Given — bare `..` followed by indented body
        let input = "..\n\n   This is a multi-line\n   comment body.";
        // When
        let doc = parse("test.rst", input);
        // Then — still just one Comment node; body is discarded
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(doc.nodes[0], Node::Comment);
    }

    #[test]
    fn test_parse_comment_before_paragraph_yields_only_paragraph() {
        // Given
        let input = ".. A comment\n\nA paragraph.";
        // When
        let doc = parse("test.rst", input);
        // Then — comment is discarded; only the paragraph survives
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(doc.nodes[0], Node::Comment);
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![InlineNode::Text("A paragraph.".to_string())])
        );
    }

    #[test]
    fn test_parse_comment_between_heading_and_paragraph() {
        // Given
        let input = "Title\n=====\n\n.. A comment\n\nSome text.";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 3);
        assert_eq!(
            doc.nodes[0],
            Node::Heading {
                level: 1,
                text: vec![InlineNode::Text("Title".to_string())]
            }
        );
        assert_eq!(doc.nodes[1], Node::Comment);
        assert_eq!(
            doc.nodes[2],
            Node::Paragraph(vec![InlineNode::Text("Some text.".to_string())])
        );
    }

    #[test]
    fn test_parse_comment_with_double_colon_in_body_is_not_a_directive() {
        // Given — the `::` is only in the indented body, not on the `..` intro line
        let input = ".. some text\n   contains:: stuff";
        // When
        let doc = parse("test.rst", input);
        // Then — still parsed as a comment (body discarded)
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(doc.nodes[0], Node::Comment);
    }

    #[test]
    fn test_parse_heading_starting_with_ellipsis_is_not_a_comment() {
        // Given — heading text that begins with "..." (three dots)
        let input =
            "... install scientific Python packages?\n---------------------------------------";
        // When
        let doc = parse("test.rst", input);
        // Then — must be a Heading, not a Comment
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Heading {
                level: 1,
                text: vec![InlineNode::Text(
                    "\u{2026} install scientific Python packages?".to_string()
                )]
            }
        );
    }

    #[test]
    fn test_parse_paragraph_starting_with_ellipsis_is_not_a_comment() {
        // Given — a plain paragraph whose text begins with "..."
        let input = "...continued from above.";
        // When
        let doc = parse("test.rst", input);
        // Then — must be a Paragraph, not a Comment
        assert_eq!(doc.nodes.len(), 1);
        assert_eq!(
            doc.nodes[0],
            Node::Paragraph(vec![InlineNode::Text(
                "\u{2026}continued from above.".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_directive_is_not_misidentified_as_comment() {
        // Given — a real directive must not be swallowed by try_parse_comment
        let input = ".. note::\n\n   Body text.";
        // When
        let doc = parse("test.rst", input);
        // Then — directive is parsed, not a Comment
        assert_eq!(doc.nodes.len(), 1);
        assert!(
            matches!(&doc.nodes[0], Node::Directive(_)),
            "Expected Directive, got {:?}",
            doc.nodes[0]
        );
    }

    #[test]
    fn test_parse_target_is_not_misidentified_as_comment() {
        // Given — a named target must not be swallowed by try_parse_comment
        let input = ".. _my-target:";
        // When
        let doc = parse("test.rst", input);
        // Then — target is parsed, not a Comment
        assert_eq!(doc.nodes.len(), 1);
        assert!(
            matches!(&doc.nodes[0], Node::Target { .. }),
            "Expected Target, got {:?}",
            doc.nodes[0]
        );
    }
}
