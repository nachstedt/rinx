//! docutils' bare *doctest block*: a text block opening with `>>> `, with no
//! directive around it.
//!
//! Distinct from the `sphinx.ext.doctest` directive family in
//! [`crate::directives::doctest`], which this shares no code with: that parses
//! `.. doctest::`/`.. testcode::`/… into a
//! [`rinx_ast::Directive::DocTest`], whereas this is a block-level
//! construct producing a [`Node::DoctestBlock`] and is reached from the
//! ordinary block dispatch chain. Both end up feeding the worker's doctest
//! plan, which is where the two forms finally meet.

use crate::indent::strip_common_indent;
use rinx_ast::Node;

/// Whether `line` opens a doctest block: `>>>` followed by a space or nothing,
/// ignoring indentation.
///
/// Requiring the space (or end of line) is what keeps this disjoint from a
/// transition, which needs four or more *identical* punctuation characters —
/// `>>>` is too short to be one and `>>>>` has no space, so neither construct
/// can be mistaken for the other.
fn opens_doctest_block(line: &str) -> bool {
    let trimmed = line.trim_start();
    match trimmed.strip_prefix(">>>") {
        Some(rest) => rest.is_empty() || rest.starts_with(' '),
        None => false,
    }
}

/// Parses a docutils *doctest block*: a text block beginning with `>>> ` and
/// running to the next blank line.
///
/// Everything up to that blank line belongs to the block — `...` continuation
/// lines and the expected output, which is why a doctest that wants to expect
/// an empty line has to write `<BLANKLINE>` rather than a real one.
///
/// This is reached only from the block-level dispatch chain, and only in
/// block-*start* position, so a `>>>` appearing partway through a paragraph
/// stays prose (as it does in docutils). Content introduced by `::` never
/// arrives here at all: `parse_paragraph` detects the trailing `::` and
/// [`super::literal_block::collect_literal_block_body`] consumes the whole
/// indented run, so a `>>>` inside a literal block is never re-examined —
/// which is what keeps those blocks non-executable.
pub(super) fn try_parse_doctest_block(lines: &[&str], i: usize) -> Option<(usize, Node)> {
    if !opens_doctest_block(lines[i]) {
        return None;
    }

    let end = lines[i..]
        .iter()
        .position(|line| line.trim().is_empty())
        .map_or(lines.len(), |offset| i + offset);

    let content = strip_common_indent(&lines[i..end]);
    Some((
        end - i,
        Node::DoctestBlock(rinx_ast::HashedContent::new(content)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opens_doctest_block_accepts_a_prompt_with_code() {
        // Given / When / Then
        assert!(opens_doctest_block(">>> 1 + 1"));
    }

    #[test]
    fn test_opens_doctest_block_accepts_an_indented_prompt() {
        // Given — essentially every real-world doctest block is indented,
        // sitting inside a block quote after a paragraph ending in `:`.
        assert!(opens_doctest_block("      >>> import re"));
    }

    #[test]
    fn test_opens_doctest_block_accepts_a_bare_prompt() {
        // Given / When / Then
        assert!(opens_doctest_block(">>>"));
    }

    #[test]
    fn test_opens_doctest_block_rejects_a_transition() {
        // Given — four or more repeated punctuation characters are a
        // transition; the two constructs are disjoint by spec, so neither
        // detector needs loosening to accommodate the other.
        assert!(!opens_doctest_block(">>>>"));
    }

    #[test]
    fn test_opens_doctest_block_rejects_a_prompt_without_separation() {
        // Given — `>>>x` is not the interactive prompt.
        assert!(!opens_doctest_block(">>>x = 1"));
    }

    #[test]
    fn test_opens_doctest_block_rejects_ordinary_prose() {
        // Given / When / Then
        assert!(!opens_doctest_block("Some prose about >>> prompts."));
    }

    #[test]
    fn test_try_parse_doctest_block_collects_until_a_blank_line() {
        // Given — continuation and expected-output lines belong to the block.
        let lines = vec![">>> f()", "... more", "result", "", "After."];

        // When
        let (consumed, node) = try_parse_doctest_block(&lines, 0).expect("should parse");

        // Then
        assert_eq!(consumed, 3);
        match node {
            Node::DoctestBlock(content) => {
                assert_eq!(content.body(), ">>> f()\n... more\nresult");
            }
            other => panic!("expected a doctest block, got {other:?}"),
        }
    }

    #[test]
    fn test_try_parse_doctest_block_strips_the_common_indent() {
        // Given
        let lines = vec!["      >>> import re", "      >>> re.escape('x')", ""];

        // When
        let (_, node) = try_parse_doctest_block(&lines, 0).expect("should parse");

        // Then
        match node {
            Node::DoctestBlock(content) => {
                assert_eq!(content.body(), ">>> import re\n>>> re.escape('x')");
            }
            other => panic!("expected a doctest block, got {other:?}"),
        }
    }

    #[test]
    fn test_try_parse_doctest_block_preserves_relative_indentation() {
        // Given — a continuation line indented under the prompt.
        let lines = vec!["   >>> def f():", "   ...     return 1", ""];

        // When
        let (_, node) = try_parse_doctest_block(&lines, 0).expect("should parse");

        // Then
        match node {
            Node::DoctestBlock(content) => {
                assert_eq!(content.body(), ">>> def f():\n...     return 1");
            }
            other => panic!("expected a doctest block, got {other:?}"),
        }
    }

    #[test]
    fn test_try_parse_doctest_block_runs_to_end_of_input() {
        // Given — no trailing blank line.
        let lines = vec![">>> 1 + 1", "2"];

        // When
        let (consumed, _) = try_parse_doctest_block(&lines, 0).expect("should parse");

        // Then
        assert_eq!(consumed, 2);
    }

    #[test]
    fn test_try_parse_doctest_block_declines_a_non_prompt_line() {
        // Given
        let lines = vec!["Just prose."];

        // When
        let result = try_parse_doctest_block(&lines, 0);

        // Then
        assert!(result.is_none());
    }
}
