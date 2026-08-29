use rusty_sphinx_ast::Node;

/// Tries to parse an RST transition (horizontal rule) starting at line `i`.
///
/// A transition is a single line of 4 or more repeated identical ASCII punctuation
/// characters (e.g. `----` or `====`), with a blank line (or document boundary)
/// immediately before and after it.
///
/// The RST spec also says a transition should not begin or end the document, nor
/// immediately follow another transition; violations are reported as diagnostics
/// rather than rejected, since rusty-sphinx's parser is error-resilient.
pub(super) fn try_parse_transition(
    lines: &[&str],
    i: usize,
    nodes: &[Node],
    diagnostics: &mut Vec<String>,
) -> Option<(usize, Node)> {
    let line = lines[i].trim();
    let mut chars = line.chars();
    let first = chars.next()?;
    if line.len() < 4 || !first.is_ascii_punctuation() || !chars.all(|c| c == first) {
        return None;
    }

    let preceded_by_blank = i == 0 || lines[i - 1].trim().is_empty();
    let followed_by_blank = i + 1 >= lines.len() || lines[i + 1].trim().is_empty();
    if !preceded_by_blank || !followed_by_blank {
        return None;
    }

    if nodes.is_empty() {
        diagnostics.push("transition (horizontal rule) may not begin the document".to_string());
    } else if matches!(nodes.last(), Some(Node::Transition)) {
        diagnostics.push(
            "transition (horizontal rule) may not immediately follow another transition"
                .to_string(),
        );
    }
    if lines[i + 1..].iter().all(|l| l.trim().is_empty()) {
        diagnostics.push("transition (horizontal rule) may not end the document".to_string());
    }

    Some((1, Node::Transition))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_try_parse_transition_matches_hyphens() {
        // Given
        let lines = vec!["", "----", ""];
        let nodes = Vec::new();
        let mut diagnostics = Vec::new();
        // When
        let result = try_parse_transition(&lines, 1, &nodes, &mut diagnostics);
        // Then
        assert_eq!(result, Some((1, Node::Transition)));
    }

    #[test]
    fn test_try_parse_transition_matches_other_punctuation_characters() {
        // Given
        let lines = vec!["", "====", ""];
        let nodes = Vec::new();
        let mut diagnostics = Vec::new();
        // When
        let result = try_parse_transition(&lines, 1, &nodes, &mut diagnostics);
        // Then
        assert_eq!(result, Some((1, Node::Transition)));
    }

    #[test]
    fn test_try_parse_transition_returns_none_for_fewer_than_four_characters() {
        // Given
        let lines = vec!["", "---", ""];
        let nodes = Vec::new();
        let mut diagnostics = Vec::new();
        // When
        let result = try_parse_transition(&lines, 1, &nodes, &mut diagnostics);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_transition_returns_none_for_mixed_characters() {
        // Given
        let lines = vec!["", "--==", ""];
        let nodes = Vec::new();
        let mut diagnostics = Vec::new();
        // When
        let result = try_parse_transition(&lines, 1, &nodes, &mut diagnostics);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_transition_returns_none_without_preceding_blank_line() {
        // Given
        let lines = vec!["Some text", "----", ""];
        let nodes = Vec::new();
        let mut diagnostics = Vec::new();
        // When
        let result = try_parse_transition(&lines, 1, &nodes, &mut diagnostics);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_transition_returns_none_without_following_blank_line() {
        // Given
        let lines = vec!["", "----", "Some text"];
        let nodes = Vec::new();
        let mut diagnostics = Vec::new();
        // When
        let result = try_parse_transition(&lines, 1, &nodes, &mut diagnostics);
        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_transition_matches_at_start_of_document() {
        // Given
        let lines = vec!["----", ""];
        let nodes = Vec::new();
        let mut diagnostics = Vec::new();
        // When
        let result = try_parse_transition(&lines, 0, &nodes, &mut diagnostics);
        // Then
        assert_eq!(result, Some((1, Node::Transition)));
    }

    #[test]
    fn test_try_parse_transition_matches_at_end_of_document() {
        // Given
        let lines = vec!["", "----"];
        let nodes = Vec::new();
        let mut diagnostics = Vec::new();
        // When
        let result = try_parse_transition(&lines, 1, &nodes, &mut diagnostics);
        // Then
        assert_eq!(result, Some((1, Node::Transition)));
    }

    #[test]
    fn test_try_parse_transition_emits_diagnostic_when_beginning_document() {
        // Given — no nodes parsed yet
        let lines = vec!["----", "", "More text"];
        let nodes = Vec::new();
        let mut diagnostics = Vec::new();
        // When
        try_parse_transition(&lines, 0, &nodes, &mut diagnostics);
        // Then
        assert!(diagnostics.iter().any(|d| d.contains("begin the document")));
    }

    #[test]
    fn test_try_parse_transition_emits_diagnostic_when_ending_document() {
        // Given — only blank lines remain afterwards
        let lines = vec!["Some text", "", "----", "", "  "];
        let nodes = vec![Node::Paragraph(vec![])];
        let mut diagnostics = Vec::new();
        // When
        try_parse_transition(&lines, 2, &nodes, &mut diagnostics);
        // Then
        assert!(diagnostics.iter().any(|d| d.contains("end the document")));
    }

    #[test]
    fn test_try_parse_transition_emits_diagnostic_when_immediately_adjacent() {
        // Given — the previously parsed node is also a transition
        let lines = vec!["", "----", "", "Some text"];
        let nodes = vec![Node::Transition];
        let mut diagnostics = Vec::new();
        // When
        try_parse_transition(&lines, 1, &nodes, &mut diagnostics);
        // Then
        assert!(
            diagnostics
                .iter()
                .any(|d| d.contains("immediately follow another transition"))
        );
    }

    #[test]
    fn test_try_parse_transition_no_diagnostic_for_well_formed_transition() {
        // Given — a transition with content both before and after
        let lines = vec!["Some text", "", "----", "", "More text"];
        let nodes = vec![Node::Paragraph(vec![])];
        let mut diagnostics = Vec::new();
        // When
        try_parse_transition(&lines, 2, &nodes, &mut diagnostics);
        // Then
        assert!(diagnostics.is_empty());
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::parse;
    use rusty_sphinx_ast::{InlineNode, Node};

    // --- Transition integration tests ---

    #[test]
    fn test_parse_transition_between_paragraphs_produces_no_diagnostics() {
        // Given
        let input = "First paragraph.\n\n----\n\nSecond paragraph.";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(
            doc.nodes,
            vec![
                Node::Paragraph(vec![InlineNode::Text("First paragraph.".to_string())]),
                Node::Transition,
                Node::Paragraph(vec![InlineNode::Text("Second paragraph.".to_string())]),
            ]
        );
        assert!(doc.diagnostics.is_empty());
    }

    #[test]
    fn test_parse_real_heading_is_unaffected_by_transition_dispatch() {
        // Given — a normal underline-only heading, adjacent to the transition dispatch
        let input = "Heading\n=======\n\nSome text.";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(
            doc.nodes,
            vec![
                Node::Heading {
                    level: 1,
                    text: vec![InlineNode::Text("Heading".to_string())]
                },
                Node::Paragraph(vec![InlineNode::Text("Some text.".to_string())]),
            ]
        );
    }

    #[test]
    fn test_parse_transition_at_document_start_emits_diagnostic() {
        // Given
        let input = "----\n\nSome text.";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes[0], Node::Transition);
        assert!(
            doc.diagnostics
                .iter()
                .any(|d| d.contains("begin the document"))
        );
    }
}
