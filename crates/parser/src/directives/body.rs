//! Collection of the lines belonging to an explicit-markup block — a
//! directive's indented body, the further signature lines a domain-object
//! directive may declare, and the flattening of a body back to plain text
//! for [`rusty_sphinx_ast::Directive::Unknown`].
//!
//! Also reached from `crate::blocks::comment`, since an RST comment is the
//! same `.. ` explicit-markup construct and its body is delimited by exactly
//! the same rule.

use crate::indent::indent_width;

/// Collects the indented body belonging to a directive/comment whose own
/// intro line has `min_indent` leading whitespace characters.
///
/// Only lines indented *more* than `min_indent` (plus blank lines) are part
/// of the body; a line indented at or below `min_indent` ends it. This
/// distinguishes true nested body content from a sibling block at the same
/// indentation — e.g. a bodyless `.. index:: single: x` immediately followed
/// by a paragraph at the same indent level (common inside glossary entries,
/// list items, and other indented containers) must not swallow that
/// paragraph as if it were the directive's own body.
pub(crate) fn collect_directive_body<'a>(
    lines: &[&'a str],
    start_index: usize,
    min_indent: usize,
) -> (usize, Vec<&'a str>) {
    let mut body_lines = Vec::new();
    let mut current = start_index;
    while current < lines.len() {
        let next_line = lines[current].trim_end();
        if next_line.trim().is_empty() || indent_width(next_line) > min_indent {
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

/// Collects a domain object directive's *argument continuation lines*: the
/// further signatures a single definition directive may declare, each on its
/// own line immediately below the directive marker, e.g.
///
/// ```rst
/// .. data:: AF_UNIX
///           AF_INET
///           AF_INET6
/// ```
///
/// Unlike a directive body, continuation lines need no blank line to separate
/// them from the marker — so scanning stops at the first blank line, at any
/// line indented no more than `min_indent` (the marker's own indentation), or
/// at an option field such as `:type: int`, whichever comes first. Everything
/// after that belongs to [`collect_directive_body`] instead, which cannot
/// make this distinction itself: to it a continuation line and a docstring
/// line are both just "indented more than the marker".
///
/// Returns the number of lines consumed and each one's trimmed text.
pub(super) fn collect_argument_continuation_lines(
    lines: &[&str],
    start_index: usize,
    min_indent: usize,
) -> (usize, Vec<String>) {
    let mut continuations = Vec::new();
    let mut current = start_index;
    while current < lines.len() {
        let line = lines[current].trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() || indent_width(line) <= min_indent || trimmed.starts_with(':') {
            break;
        }
        continuations.push(trimmed.to_string());
        current += 1;
    }

    (current - start_index, continuations)
}

/// Flattens a directive body back to plain text, one line per source line
/// with each line's own leading whitespace dropped.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_argument_continuation_lines_collects_every_further_signature() {
        // Given — the shape `library/socket.rst` declares its address
        // families in: three aliases for one object, no blank line between.
        let lines = vec![
            ".. data:: AF_UNIX",
            "          AF_INET",
            "          AF_INET6",
        ];

        // When
        let (consumed, continuations) = collect_argument_continuation_lines(&lines, 1, 0);

        // Then
        assert_eq!(consumed, 2);
        assert_eq!(continuations, ["AF_INET", "AF_INET6"]);
    }

    #[test]
    fn test_collect_argument_continuation_lines_stops_at_a_blank_line() {
        // Given — everything past the blank line is docstring body.
        let lines = vec![
            ".. data:: AF_UNIX",
            "          AF_INET",
            "",
            "   The address families.",
        ];

        // When
        let (consumed, continuations) = collect_argument_continuation_lines(&lines, 1, 0);

        // Then
        assert_eq!(consumed, 1);
        assert_eq!(continuations, ["AF_INET"]);
    }

    #[test]
    fn test_collect_argument_continuation_lines_stops_at_an_option_field() {
        // Given — `:type:` opens the directive's option block, which ends the
        // argument text even though it is indented like a continuation.
        let lines = vec![
            ".. data:: DEFAULT_TIMEOUT",
            "   :type: int",
            "   :value: 30",
        ];

        // When
        let (consumed, continuations) = collect_argument_continuation_lines(&lines, 1, 0);

        // Then
        assert_eq!(consumed, 0);
        assert!(continuations.is_empty());
    }

    #[test]
    fn test_collect_argument_continuation_lines_stops_at_a_dedent() {
        // Given — a line indented no further than the marker is a sibling
        // block, not part of this directive at all.
        let lines = vec![".. data:: AF_UNIX", "Next paragraph."];

        // When
        let (consumed, continuations) = collect_argument_continuation_lines(&lines, 1, 0);

        // Then
        assert_eq!(consumed, 0);
        assert!(continuations.is_empty());
    }

    #[test]
    fn test_collect_argument_continuation_lines_respects_a_nested_markers_indentation() {
        // Given — a directive nested inside another block: its continuation
        // lines are indented past *its* marker, not past column zero.
        let lines = vec![
            "   .. data:: AF_UNIX",
            "             AF_INET",
            "   Sibling text.",
        ];

        // When
        let (consumed, continuations) = collect_argument_continuation_lines(&lines, 1, 3);

        // Then
        assert_eq!(consumed, 1);
        assert_eq!(continuations, ["AF_INET"]);
    }

    #[test]
    fn test_collect_argument_continuation_lines_returns_nothing_at_end_of_input() {
        // Given — a bodyless directive on the document's last line.
        let lines = vec![".. data:: AF_UNIX"];

        // When
        let (consumed, continuations) = collect_argument_continuation_lines(&lines, 1, 0);

        // Then
        assert_eq!(consumed, 0);
        assert!(continuations.is_empty());
    }

    // --- join_body_lines unit tests ---

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
        let (consumed, body) = collect_directive_body(&lines, 1, 0);
        // Then
        assert_eq!(consumed, 2);
        assert_eq!(body, vec!["   body1", "   body2"]);
    }

    #[test]
    fn test_collect_directive_body_stops_at_unindented_line() {
        // Given
        let lines = vec![".. note::", "   body1", "unindented", "   body2"];
        // When
        let (consumed, body) = collect_directive_body(&lines, 1, 0);
        // Then
        assert_eq!(consumed, 1);
        assert_eq!(body, vec!["   body1"]);
    }

    #[test]
    fn test_collect_directive_body_strips_leading_and_trailing_blank_lines() {
        // Given
        let lines = vec![".. note::", "  ", "   body1", "  ", "   body2", "   ", ""];
        // When
        let (consumed, body) = collect_directive_body(&lines, 1, 0);
        // Then
        assert_eq!(consumed, 6);
        assert_eq!(body, vec!["   body1", "", "   body2"]);
    }

    #[test]
    fn test_collect_directive_body_returns_empty_when_no_body() {
        // Given
        let lines = vec![".. note::", "unindented"];
        // When
        let (consumed, body) = collect_directive_body(&lines, 1, 0);
        // Then
        assert_eq!(consumed, 0);
        assert!(body.is_empty());
    }

    #[test]
    fn test_collect_directive_body_returns_correct_consumed_count() {
        // Given
        let lines = vec![".. note::", "   body", "  "];
        // When
        let (consumed, body) = collect_directive_body(&lines, 1, 0);
        // Then
        assert_eq!(consumed, 2);
        assert_eq!(body, vec!["   body"]);
    }

    #[test]
    fn test_collect_directive_body_stops_at_sibling_indented_at_same_level_as_directive() {
        // Given — a bodyless directive nested at 3-space indent (e.g. inside
        // a glossary entry), immediately followed by a sibling paragraph at
        // the SAME 3-space indent, not a deeper one. Real Sphinx docs do
        // this constantly (CPython's glossary.rst: `.. index:: pair: magic;
        // method` followed by a plain paragraph at the same indent).
        let lines = vec![
            "   .. index:: pair: magic; method",
            "",
            "   An informal synonym for something.",
        ];
        // When — min_indent is the directive's own 3-space indentation
        let (consumed, body) = collect_directive_body(&lines, 1, 3);
        // Then — the body is empty and only the blank line is consumed;
        // critically, the sibling paragraph itself is NOT swallowed, so the
        // caller will parse it as its own paragraph node afterwards.
        assert_eq!(consumed, 1);
        assert!(body.is_empty());
    }

    #[test]
    fn test_collect_directive_body_includes_lines_indented_deeper_than_directive() {
        // Given — true nested body content, indented deeper than the
        // directive's own 3-space indent
        let lines = vec!["   .. note::", "", "      Actual body.", "   Sibling."];
        // When
        let (consumed, body) = collect_directive_body(&lines, 1, 3);
        // Then — only the deeper-indented line is included
        assert_eq!(consumed, 2);
        assert_eq!(body, vec!["      Actual body."]);
    }
}
