//! Generic leading-whitespace helpers, used wherever a construct needs to
//! measure or strip indentation — enumerated lists deciding whether a line
//! continues an item, doctest blocks dedenting their content, directive
//! bodies deciding what belongs to them. Deliberately knows nothing about
//! any particular construct.

/// Counts the leading whitespace characters on a line, char-based (not
/// byte-based) so it doesn't panic on multi-byte characters near the
/// indentation boundary. Chars are the unit every indentation-sensitive
/// construct measures in.
pub(crate) fn indent_width(line: &str) -> usize {
    line.chars().take_while(|c| c.is_whitespace()).count()
}

/// Drops the first `indent_chars` *characters* of `s`, yielding `""` when the
/// line is shorter than that. Char-based for the same reason
/// [`indent_width`] is.
pub(crate) fn strip_indent(s: &str, indent_chars: usize) -> &str {
    let mut indices = s.char_indices();
    if let Some((idx, _)) = indices.nth(indent_chars) {
        &s[idx..]
    } else {
        ""
    }
}

/// Strips the common leading indentation off a directive's body lines, taking
/// the indent from the first non-blank line. Returns an empty vec if the body
/// has no non-blank line.
///
/// The line-wise counterpart to [`strip_common_indent`], which returns one
/// joined string instead: this is what directive body parsers want, since they
/// hand the result back to the block parser as lines.
pub(crate) fn unindent_body_lines(body_lines: &[&str]) -> Vec<String> {
    let Some(first) = body_lines.iter().find(|l| !l.trim().is_empty()) else {
        return Vec::new();
    };
    let indent = indent_width(first);
    body_lines
        .iter()
        .map(|l| {
            if l.chars().count() >= indent {
                strip_indent(l, indent).to_string()
            } else {
                l.trim().to_string()
            }
        })
        .collect()
}

/// Removes the deepest indentation shared by every non-blank line, preserving
/// relative indentation within the block (so nested Python source, for
/// instance, survives intact). Blank lines are normalized to empty strings
/// rather than being left as runs of stray spaces, and never contribute to
/// the computed minimum.
pub(crate) fn strip_common_indent(body_lines: &[&str]) -> String {
    let min_indent = body_lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.chars().take_while(|c| c.is_whitespace()).count())
        .min()
        .unwrap_or(0);

    body_lines
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                String::new()
            } else {
                l.chars().skip(min_indent).collect()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_common_indent_removes_the_shared_leading_whitespace() {
        // Given
        let lines = vec!["    first", "    second"];

        // When
        let content = strip_common_indent(&lines);

        // Then
        assert_eq!(content, "first\nsecond");
    }

    #[test]
    fn test_strip_common_indent_preserves_relative_indentation() {
        // Given — the property that makes this usable for Python source.
        let lines = vec!["    def f():", "        return 1"];

        // When
        let content = strip_common_indent(&lines);

        // Then — only the shared four spaces go; the inner four remain.
        assert_eq!(content, "def f():\n    return 1");
    }

    #[test]
    fn test_strip_common_indent_normalizes_blank_lines_to_empty_strings() {
        // Given — a blank line that is shorter than the common indent.
        let lines = vec!["    first", "", "    second"];

        // When
        let content = strip_common_indent(&lines);

        // Then — the blank line must not influence the minimum, and must not
        // become a run of stray spaces.
        assert_eq!(content, "first\n\nsecond");
    }

    #[test]
    fn test_strip_common_indent_uses_the_least_indented_line_as_the_baseline() {
        // Given — the first line is deeper than a later one.
        let lines = vec!["        deep", "    shallow"];

        // When
        let content = strip_common_indent(&lines);

        // Then
        assert_eq!(content, "    deep\nshallow");
    }

    #[test]
    fn test_strip_common_indent_leaves_unindented_lines_untouched() {
        // Given
        let lines = vec!["no indent", "still none"];

        // When
        let content = strip_common_indent(&lines);

        // Then
        assert_eq!(content, "no indent\nstill none");
    }

    #[test]
    fn test_strip_common_indent_returns_empty_string_for_no_lines() {
        // Given
        let lines: Vec<&str> = Vec::new();

        // When
        let content = strip_common_indent(&lines);

        // Then
        assert_eq!(content, "");
    }

    #[test]
    fn test_strip_common_indent_handles_only_blank_lines() {
        // Given — no non-blank line to derive a minimum indent from.
        let lines = vec!["", "   "];

        // When
        let content = strip_common_indent(&lines);

        // Then
        assert_eq!(content, "\n");
    }

    #[test]
    fn test_indent_width_counts_leading_whitespace() {
        // Given / When / Then
        assert_eq!(indent_width("   text"), 3);
        assert_eq!(indent_width("text"), 0);
        assert_eq!(indent_width("  "), 2);
    }

    #[test]
    fn test_indent_width_does_not_panic_on_multibyte_char_after_indent() {
        // Given — a multi-byte character immediately after the indentation,
        // exercising the char-based (not byte-based) counting
        let line = "  éfoo";
        // When
        let width = indent_width(line);
        // Then
        assert_eq!(width, 2);
    }

    #[test]
    fn test_unindent_body_lines_strips_common_indentation() {
        // Given a body whose lines share a 4-space common indent
        let body_lines = ["    First line.", "    Second line.", "", "    Third line."];

        // When unindenting the body
        let result = unindent_body_lines(&body_lines);

        // Then the common indent is stripped from every line
        assert_eq!(
            result,
            vec!["First line.", "Second line.", "", "Third line."]
        );
    }

    #[test]
    fn test_unindent_body_lines_trims_a_line_shorter_than_the_indent() {
        // Given a body whose first line sets a 4-char indent but a later line
        // has fewer characters in total than that
        let body_lines = ["    First line.", "  "];

        // When unindenting the body
        let result = unindent_body_lines(&body_lines);

        // Then the short line is fully trimmed instead of indent-stripped
        assert_eq!(result, vec!["First line.", ""]);
    }

    #[test]
    fn test_unindent_body_lines_returns_empty_vec_when_no_non_blank_line() {
        // Given a body containing only blank lines
        let body_lines = ["", "   ", ""];

        // When unindenting the body
        let result = unindent_body_lines(&body_lines);

        // Then no lines are returned
        assert!(result.is_empty());
    }

    #[test]
    fn test_unindent_body_lines_does_not_panic_on_multi_byte_char_within_the_indent() {
        // Given a first line with a 3-char indent and a second line long
        // enough to be indent-stripped whose 3rd character is a multi-byte
        // character straddling the byte offset the old byte-index slicing
        // (using a char count as a byte index) would have panicked on
        let body_lines = ["   First line normal indent.", "  éfoo"];

        // When unindenting the body
        let result = unindent_body_lines(&body_lines);

        // Then it does not panic, and strips the first 3 characters same as
        // any other line long enough to reach the common indent
        assert_eq!(result, vec!["First line normal indent.", "foo"]);
    }
}
