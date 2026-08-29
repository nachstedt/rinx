//! Simple-table border/span-line recognition and column-span parsing —
//! shared low-level primitives for `super::simple_table`.

/// One column's `[start, end)` extent, in character offsets into the table's
/// indentation-stripped lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ColumnSpan {
    pub(super) start: usize,
    pub(super) end: usize,
}

/// Matches docutils' `simple_table_border_pat` / `head_body_separator_pat`
/// (`=+[ =]*$`): a line built only from runs of `=` separated by spaces.
/// Recognizes the top border, the header/body rule and the bottom border
/// alike — which of the three a given line is depends on where it sits.
pub(super) fn is_equals_border(line: &str) -> bool {
    line.starts_with('=') && line.chars().all(|c| c == '=' || c == ' ')
}

/// Matches docutils' `span_pat` (`-[ -]*$`): a column-span underline.
pub(super) fn is_span_line(line: &str) -> bool {
    line.starts_with('-') && line.chars().all(|c| c == '-' || c == ' ')
}

/// Matches docutils' `simple_table_top_pat` (`=+( +=+)+ *$`).
///
/// Requiring *two or more* `=` runs is what keeps this syntax disjoint from a
/// section adornment and from a transition, both of which are a single run of
/// one repeated character. Because the two languages cannot overlap, the
/// order in which `parse_blocks` tries them is irrelevant — a single-column
/// `=====` block stays a heading, exactly as in docutils.
pub(super) fn is_simple_table_top(line: &str) -> bool {
    is_equals_border(line) && parse_column_spans(line).len() >= 2
}

/// docutils' `parse_columns`: the `[start, end)` char extents of every run of
/// non-space characters in a border or span line.
pub(super) fn parse_column_spans(line: &str) -> Vec<ColumnSpan> {
    let mut spans = Vec::new();
    let mut open: Option<usize> = None;
    let mut width = 0usize;
    for (idx, c) in line.chars().enumerate() {
        width = idx + 1;
        if c == ' ' {
            if let Some(start) = open.take() {
                spans.push(ColumnSpan { start, end: idx });
            }
        } else if open.is_none() {
            open = Some(idx);
        }
    }
    if let Some(start) = open {
        spans.push(ColumnSpan { start, end: width });
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_equals_border_accepts_multi_column_border() {
        // Given / When / Then
        assert!(is_equals_border("=====  ====="));
    }

    #[test]
    fn test_is_equals_border_accepts_single_run() {
        // Given / When / Then — a single run is a valid *border*; only the
        // top-border check additionally demands two columns.
        assert!(is_equals_border("====="));
    }

    #[test]
    fn test_is_equals_border_rejects_dashes() {
        // Given / When / Then
        assert!(!is_equals_border("-----  -----"));
    }

    #[test]
    fn test_is_equals_border_rejects_leading_space() {
        // Given / When / Then — an over-indented border is content, not a
        // border, exactly as docutils' anchored regex decides.
        assert!(!is_equals_border(" ====="));
    }

    #[test]
    fn test_is_equals_border_rejects_other_characters() {
        // Given / When / Then
        assert!(!is_equals_border("==x=="));
    }

    #[test]
    fn test_is_equals_border_rejects_empty_line() {
        // Given / When / Then
        assert!(!is_equals_border(""));
    }

    // --- is_span_line ---

    #[test]
    fn test_is_span_line_accepts_dash_runs() {
        // Given / When / Then
        assert!(is_span_line("-----  -----"));
    }

    #[test]
    fn test_is_span_line_accepts_single_run() {
        // Given / When / Then
        assert!(is_span_line("------------"));
    }

    #[test]
    fn test_is_span_line_rejects_equals_border() {
        // Given / When / Then
        assert!(!is_span_line("=====  ====="));
    }

    // --- is_simple_table_top ---

    #[test]
    fn test_is_simple_table_top_accepts_two_columns() {
        // Given / When / Then
        assert!(is_simple_table_top("=====  ====="));
    }

    #[test]
    fn test_is_simple_table_top_accepts_three_columns() {
        // Given / When / Then
        assert!(is_simple_table_top("=====  =====  ======"));
    }

    #[test]
    fn test_is_simple_table_top_rejects_single_column() {
        // Given / When / Then — this is a section adornment, and keeping the
        // two syntaxes disjoint is what makes dispatch order irrelevant.
        assert!(!is_simple_table_top("====="));
    }

    #[test]
    fn test_is_simple_table_top_rejects_span_line() {
        // Given / When / Then
        assert!(!is_simple_table_top("-----  -----"));
    }

    // --- parse_column_spans ---

    #[test]
    fn test_parse_column_spans_finds_each_run() {
        // Given a two-column border
        let line = "=====  =====";

        // When
        let spans = parse_column_spans(line);

        // Then
        assert_eq!(
            spans,
            vec![
                ColumnSpan { start: 0, end: 5 },
                ColumnSpan { start: 7, end: 12 },
            ]
        );
    }

    #[test]
    fn test_parse_column_spans_handles_a_leading_gap() {
        // Given a span underline that starts partway across the table
        let line = "       ------------";

        // When
        let spans = parse_column_spans(line);

        // Then
        assert_eq!(spans, vec![ColumnSpan { start: 7, end: 19 }]);
    }

    #[test]
    fn test_parse_column_spans_returns_nothing_for_a_blank_line() {
        // Given / When / Then
        assert!(parse_column_spans("").is_empty());
    }
}
