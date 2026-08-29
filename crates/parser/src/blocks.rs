use super::bullet_list::try_parse_bullet_list;
use super::definition_list::try_parse_definition_list;
use super::directives::try_parse_directive;
use super::enumerated_list::try_parse_enumerated_list;
use super::headings::{Adornment, detect_adornment, try_parse_heading};
use super::inline::parse_inline_text;
use super::simple_table::try_parse_simple_table;
use super::table::try_parse_grid_table;
use rusty_sphinx_ast::{Document, Domain, Node};

mod comment;
mod directive_body;
mod doctest_prompt;
mod transition;

use comment::try_parse_comment;
use doctest_prompt::{collect_literal_block_body, try_parse_doctest_block, try_parse_target};
use transition::try_parse_transition;

// Re-exported so `crate::blocks::{indent_width, collect_argument_continuation_lines,
// collect_directive_body, join_body_lines, strip_common_indent}` keeps resolving for
// every module that reaches these shared body-collection helpers through `blocks`
// rather than `blocks::directive_body` directly.
pub(crate) use directive_body::{
    collect_argument_continuation_lines, collect_directive_body, indent_width, join_body_lines,
    strip_common_indent,
};

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
    parse_with_domain(path, input, Domain::Py)
}

/// Parses an RST-formatted string into a `Document`, resolving any bare
/// (unprefixed) domain directive or role — e.g. `.. function::` or `:func:` —
/// via `default_domain`. `parse` is a thin wrapper defaulting to `Domain::Py`.
///
/// # Panics
///
/// The internal implementation uses `expect()` on an iterator that is guaranteed
/// to be non-empty by preceding checks.
#[must_use]
pub fn parse_with_domain(path: &str, input: &str, default_domain: Domain) -> Document {
    let lines: Vec<&str> = input.lines().collect();
    let mut adornment_order: Vec<Adornment> = Vec::new();
    let mut diagnostics = Vec::new();

    let mut nodes = parse_blocks(
        &lines,
        &mut adornment_order,
        &mut diagnostics,
        default_domain,
    );

    let mut index_id_counter = 0;
    super::index_ids::assign_index_ids(&mut nodes, &mut index_id_counter);

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
pub(super) fn parse_blocks(
    lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim_end();

        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        if let Some((consumed, node)) =
            try_parse_directive(lines, i, adornment_order, diagnostics, default_domain)
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

        if let Some((consumed, node)) = try_parse_comment(lines, i) {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) = try_parse_transition(lines, i, &nodes, diagnostics) {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) = try_parse_heading(lines, i, adornment_order, default_domain)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) =
            try_parse_grid_table(lines, i, adornment_order, diagnostics, default_domain)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) =
            try_parse_simple_table(lines, i, adornment_order, diagnostics, default_domain)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) =
            try_parse_bullet_list(lines, i, adornment_order, diagnostics, default_domain)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        // Before the definition list: `detect_definition_term` matches any line
        // followed by a more-indented one, which every multi-line enumerated
        // item also is. docutils resolves this the same way, trying its
        // `enumerator` transition before falling through to text.
        if let Some((consumed, node)) =
            try_parse_enumerated_list(lines, i, adornment_order, diagnostics, default_domain)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) =
            try_parse_definition_list(lines, i, adornment_order, diagnostics, default_domain)
        {
            nodes.push(node);
            i += consumed;
            continue;
        }

        if let Some((consumed, node)) = try_parse_doctest_block(lines, i) {
            nodes.push(node);
            i += consumed;
            continue;
        }

        let (consumed, new_nodes) = parse_paragraph(lines, i, default_domain);
        nodes.extend(new_nodes);
        i += consumed;
    }
    nodes
}
fn parse_paragraph(lines: &[&str], i: usize, default_domain: Domain) -> (usize, Vec<Node>) {
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

    let inlines = parse_inline_text(&paragraph_text, default_domain);

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
        let inlines = parse_inline_text(&stripped, default_domain);
        return (total_consumed, vec![Node::Paragraph(inlines), literal_node]);
    }

    (current_pos_line - i, vec![Node::Paragraph(inlines)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::InlineNode;

    #[test]
    fn test_parse_blocks_empty_input() {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();
        let nodes = parse_blocks(&[], &mut adornment_order, &mut diagnostics, Domain::Py);
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_parse_blocks_simple_paragraph() {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();
        let lines = vec!["Hello world"];
        let nodes = parse_blocks(&lines, &mut adornment_order, &mut diagnostics, Domain::Py);
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
        let nodes1 = parse_blocks(&lines1, &mut adornment_order, &mut diagnostics, Domain::Py);
        assert_eq!(nodes1.len(), 1);

        let lines2 = vec!["Title 2", "-------"];
        let nodes2 = parse_blocks(&lines2, &mut adornment_order, &mut diagnostics, Domain::Py);
        assert_eq!(nodes2.len(), 1);

        assert_eq!(adornment_order.len(), 2);
    }

    #[test]
    fn test_parse_blocks_collects_diagnostics() {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // This will trigger a diagnostic because of the unknown option
        let lines = vec![".. toctree::", "   :unknown_option: value"];
        let nodes = parse_blocks(&lines, &mut adornment_order, &mut diagnostics, Domain::Py);

        assert_eq!(nodes.len(), 1);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].contains("Invalid or non-standard Sphinx toctree option"));
    }

    // --- try_parse_comment unit tests ---
}

#[cfg(test)]
mod integration_tests {
    use crate::parse;
    use rusty_sphinx_ast::TargetName;
    use rusty_sphinx_ast::{InlineNode, Node};

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
                text: vec![InlineNode::Text("Title".to_string())]
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
                text: vec![InlineNode::Text("Heading".to_string())]
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
                text: vec![InlineNode::Text("Heading".to_string())]
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
                text: vec![InlineNode::Text("Heading".to_string())]
            }
        );
    }

    #[test]
    fn test_parse_keeps_a_literal_block_containing_prompts_literal() {
        // Given — THE case this feature must not break. A `::`-introduced
        // block is a literal block in docutils, never a doctest block, and
        // Sphinx does not execute it. In the CPython corpus 1711 nodes have
        // this shape against 448 real doctest blocks, so a detector that fired
        // here would make a great deal of illustrative code suddenly runnable.
        let input = "Example::\n\n    >>> 1 + 1\n    2\n";

        // When
        let doc = parse("test.rst", input);

        // Then — paragraph plus LiteralBlock; no DoctestBlock anywhere.
        assert_eq!(doc.nodes.len(), 2);
        assert!(matches!(doc.nodes[1], Node::LiteralBlock { .. }));
        assert!(!doc.nodes.iter().any(|n| matches!(n, Node::DoctestBlock(_))));
    }

    #[test]
    fn test_parse_creates_a_doctest_block_after_a_single_colon() {
        // Given — the real-world shape: one colon, so a block quote rather
        // than a literal block, and the indented run is a doctest block.
        let input =
            "Use search rather than match:\n\n   >>> import re\n   >>> re.search('a', 'ba')\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        match &doc.nodes[1] {
            Node::DoctestBlock(content) => {
                assert_eq!(content.body(), ">>> import re\n>>> re.search('a', 'ba')");
            }
            other => panic!("expected a doctest block, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_creates_a_doctest_block_at_the_left_margin() {
        // Given
        let input = ">>> 1 + 1\n2\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(matches!(doc.nodes[0], Node::DoctestBlock(_)));
    }

    #[test]
    fn test_parse_leaves_a_mid_paragraph_prompt_as_prose() {
        // Given — a doctest block has to *start* a text block, in docutils and
        // here alike; 3 CPython paragraphs rely on this.
        let input = "Some prose.\n>>> f()\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(matches!(doc.nodes[0], Node::Paragraph(_)));
    }

    #[test]
    fn test_parse_keeps_a_transition_a_transition() {
        // Given — `>>>>` is four repeated punctuation characters.
        let input = "Before.\n\n>>>>\n\nAfter.\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert!(doc.nodes.iter().any(|n| matches!(n, Node::Transition)));
        assert!(!doc.nodes.iter().any(|n| matches!(n, Node::DoctestBlock(_))));
    }

    #[test]
    fn test_parse_finds_a_doctest_block_nested_in_an_admonition() {
        // Given — directive bodies are parsed recursively, so bare blocks
        // inside them are found too (and `walk_nodes` reaches them later).
        let input = ".. note::\n\n   Try it:\n\n   >>> 1 + 1\n   2\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        let mut found = false;
        rusty_sphinx_ast::walk_nodes(&doc.nodes, &mut |node| {
            if matches!(node, Node::DoctestBlock(_)) {
                found = true;
            }
        });
        assert!(found, "expected a doctest block inside the admonition");
    }

    #[test]
    fn test_parse_separates_two_doctest_blocks_by_a_blank_line() {
        // Given
        let input = ">>> a = 1\n\n>>> b = 2\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert!(doc.nodes.iter().all(|n| matches!(n, Node::DoctestBlock(_))));
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

    #[test]
    fn test_parse_bodyless_index_directive_inside_glossary_does_not_swallow_following_paragraph() {
        // Given — mirrors real CPython usage (Doc/glossary.rst): a bodyless
        // `.. index::` directive nested inside a glossary term's definition,
        // immediately followed by a plain paragraph at the SAME indentation
        // (not a deeper one). Before the indentation-depth fix, the
        // paragraph was swallowed into the directive's body and mis-parsed
        // as bogus index entries.
        let input = "\
.. glossary::

   magic method
      .. index:: pair: magic; method

      An informal synonym for something.
";
        // When
        let doc = parse("test.rst", input);
        // Then
        assert_eq!(doc.nodes.len(), 1);
        let Node::Directive(rusty_sphinx_ast::Directive::Glossary { entries, .. }) = &doc.nodes[0]
        else {
            panic!("Expected Glossary directive, got {:?}", doc.nodes[0]);
        };
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].terms, vec!["magic method".to_string()]);
        // The definition must contain the Index directive AND the paragraph
        // as two separate sibling nodes — not one node with the paragraph's
        // text corrupted into bogus index entries.
        assert_eq!(entries[0].definition.len(), 2);
        assert!(matches!(
            entries[0].definition[0],
            Node::Directive(rusty_sphinx_ast::Directive::Index { .. })
        ));
        assert_eq!(
            entries[0].definition[1],
            Node::Paragraph(vec![InlineNode::Text(
                "An informal synonym for something.".to_string()
            )])
        );
        // No diagnostics about invalid/unknown index entries should be emitted.
        assert!(
            !doc.diagnostics
                .iter()
                .any(|d| d.contains(".. index::") || d.contains("index:"))
        );
    }
}
