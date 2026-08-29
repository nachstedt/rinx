use super::borders::is_equals_border;
use crate::bullet_list::{leading_whitespace_count, strip_indent};

/// Collects the table's physical lines, from the top border at `start_i`
/// through the bottom border, with the top border's indentation stripped off
/// every line. The returned vector has exactly one entry per consumed source
/// line, blank lines included — a simple table may contain them.
///
/// docutils' `isolate_simple_table` decides where the table ends: at the
/// *second* `=` border line, or at the first one that is followed by a blank
/// line or the end of input. That is the whole rule distinguishing an
/// interior header rule (never followed by a blank line) from the bottom
/// border.
pub(super) fn collect_simple_table_lines(
    lines: &[&str],
    start_i: usize,
    diagnostics: &mut Vec<String>,
) -> Option<Vec<String>> {
    let top = lines[start_i].trim_end();
    let indent = leading_whitespace_count(top);
    let top_border = strip_indent(top, indent);
    let top_width = top_border.chars().count();

    let mut rows = vec![top_border.to_string()];
    let mut borders_found = 0usize;

    for (offset, &raw_line) in lines.iter().enumerate().skip(start_i + 1) {
        let line = raw_line.trim_end();
        if line.trim().is_empty() {
            rows.push(String::new());
            continue;
        }

        let this_indent = leading_whitespace_count(line);
        if this_indent < indent {
            diagnostics.push(format!(
                "simple table: line {} is indented less than the top border (expected at least {indent}, got {this_indent})",
                offset + 1,
            ));
            return None;
        }
        let content = strip_indent(line, indent).to_string();

        if is_equals_border(&content) {
            if content.chars().count() != top_width {
                diagnostics.push(format!(
                    "simple table: the border on line {} does not match the top border's width ({top_width})",
                    offset + 1,
                ));
                return None;
            }
            borders_found += 1;
            let ends_table = borders_found == 2
                || lines
                    .get(offset + 1)
                    .is_none_or(|next| next.trim().is_empty());
            rows.push(content);
            if ends_table {
                return Some(rows);
            }
            continue;
        }

        rows.push(content);
    }

    diagnostics.push(if borders_found > 0 {
        "simple table: no bottom border found, or no blank line after the table's bottom border"
            .to_string()
    } else {
        "simple table: no bottom border found".to_string()
    });
    None
}

/// Where a table's header/body rule sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HeadBodyRule {
    /// The table has no rule, so every row is a body row.
    Absent,
    /// The rule is the table line at this index.
    At(usize),
    /// The table has more than one rule, which is malformed.
    Ambiguous,
}

/// Rewrites the top and bottom borders to `-` (docutils' `setup`) and then
/// locates the header/body rule among what remains (its `find_head_body_sep`),
/// rewriting that to `-` too.
///
/// Because the two borders are converted *first*, the rule can never be the
/// table's first or last line — the error docutils raises for that case is
/// unreachable here rather than omitted.
pub(super) fn find_head_body_rule(
    rows: &mut [String],
    start_i: usize,
    diagnostics: &mut Vec<String>,
) -> HeadBodyRule {
    let last = rows.len() - 1;
    rows[0] = rows[0].replace('=', "-");
    rows[last] = rows[last].replace('=', "-");

    let mut rule = HeadBodyRule::Absent;
    for (idx, row) in rows.iter_mut().enumerate() {
        if !is_equals_border(row) {
            continue;
        }
        if let HeadBodyRule::At(previous) = rule {
            diagnostics.push(format!(
                "simple table: multiple head/body row separators (lines {} and {}); only one allowed",
                start_i + previous + 1,
                start_i + idx + 1,
            ));
            return HeadBodyRule::Ambiguous;
        }
        rule = HeadBodyRule::At(idx);
        *row = row.replace('=', "-");
    }

    rule
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- collect_simple_table_lines ---

    #[test]
    fn test_collect_simple_table_lines_takes_the_whole_table() {
        // Given a headerless table followed by a paragraph
        let lines = vec!["=====  =====", "1      2", "=====  =====", "", "after"];
        let mut diagnostics = Vec::new();

        // When
        let rows = collect_simple_table_lines(&lines, 0, &mut diagnostics).expect("should collect");

        // Then — the table stops at its bottom border
        assert_eq!(rows, vec!["=====  =====", "1      2", "=====  ====="]);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_collect_simple_table_lines_continues_past_a_header_rule() {
        // Given a table whose first border is a header rule (no blank line
        // follows it), so collection must run on to the second border
        let lines = vec![
            "=====  =====",
            "A      B",
            "=====  =====",
            "1      2",
            "=====  =====",
        ];
        let mut diagnostics = Vec::new();

        // When
        let rows = collect_simple_table_lines(&lines, 0, &mut diagnostics).expect("should collect");

        // Then
        assert_eq!(rows.len(), 5);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_collect_simple_table_lines_keeps_interior_blank_lines() {
        // Given a table with a blank line separating two rows
        let lines = vec!["=====  =====", "1      a", "", "2      b", "=====  ====="];
        let mut diagnostics = Vec::new();

        // When
        let rows = collect_simple_table_lines(&lines, 0, &mut diagnostics).expect("should collect");

        // Then — one entry per source line, so the caller's line count is right
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[2], "");
    }

    #[test]
    fn test_collect_simple_table_lines_strips_the_common_indent() {
        // Given an indented table whose second column's text is further in
        let lines = vec![
            "  =====  =====",
            "  1      a",
            "         continued",
            "  =====  =====",
        ];
        let mut diagnostics = Vec::new();

        // When
        let rows = collect_simple_table_lines(&lines, 0, &mut diagnostics).expect("should collect");

        // Then — only the table's own indent goes; cell indentation survives
        assert_eq!(rows[0], "=====  =====");
        assert_eq!(rows[2], "       continued");
    }

    #[test]
    fn test_collect_simple_table_lines_rejects_a_mismatched_border() {
        // Given a bottom border narrower than the top one
        let lines = vec!["=====  =====", "1      2", "=====  ===", ""];
        let mut diagnostics = Vec::new();

        // When
        let rows = collect_simple_table_lines(&lines, 0, &mut diagnostics);

        // Then
        assert!(rows.is_none());
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("does not match the top border's width"));
    }

    #[test]
    fn test_collect_simple_table_lines_rejects_an_unterminated_table() {
        // Given a table with no bottom border at all
        let lines = vec!["=====  =====", "1      2"];
        let mut diagnostics = Vec::new();

        // When
        let rows = collect_simple_table_lines(&lines, 0, &mut diagnostics);

        // Then
        assert!(rows.is_none());
        assert_eq!(diagnostics, vec!["simple table: no bottom border found"]);
    }

    #[test]
    fn test_collect_simple_table_lines_reports_a_missing_blank_line_after_the_bottom() {
        // Given a header rule but no closing border, so the table runs off the
        // end of the input
        let lines = vec!["=====  =====", "A      B", "=====  =====", "1      2"];
        let mut diagnostics = Vec::new();

        // When
        let rows = collect_simple_table_lines(&lines, 0, &mut diagnostics);

        // Then
        assert!(rows.is_none());
        assert!(diagnostics[0].contains("no blank line after"));
    }

    #[test]
    fn test_collect_simple_table_lines_rejects_an_under_indented_line() {
        // Given an indented table with a line that escapes its indentation
        let lines = vec!["  =====  =====", "1      2", "  =====  =====", ""];
        let mut diagnostics = Vec::new();

        // When
        let rows = collect_simple_table_lines(&lines, 0, &mut diagnostics);

        // Then
        assert!(rows.is_none());
        assert!(diagnostics[0].contains("indented less than the top border"));
    }

    // --- find_head_body_rule ---

    #[test]
    fn test_find_head_body_rule_finds_the_interior_rule() {
        // Given a table with a header rule at index 2
        let mut rows = vec![
            "=====  =====".to_string(),
            "A      B".to_string(),
            "=====  =====".to_string(),
            "1      2".to_string(),
            "=====  =====".to_string(),
        ];
        let mut diagnostics = Vec::new();

        // When
        let rule = find_head_body_rule(&mut rows, 0, &mut diagnostics);

        // Then — the rule is found and every border is now a span line
        assert_eq!(rule, HeadBodyRule::At(2));
        assert_eq!(rows[0], "-----  -----");
        assert_eq!(rows[2], "-----  -----");
        assert_eq!(rows[4], "-----  -----");
    }

    #[test]
    fn test_find_head_body_rule_reports_absent_without_a_rule() {
        // Given a headerless table
        let mut rows = vec![
            "=====  =====".to_string(),
            "1      2".to_string(),
            "=====  =====".to_string(),
        ];
        let mut diagnostics = Vec::new();

        // When
        let rule = find_head_body_rule(&mut rows, 0, &mut diagnostics);

        // Then
        assert_eq!(rule, HeadBodyRule::Absent);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_find_head_body_rule_rejects_two_rules() {
        // Given a table with two interior `=` rules
        let mut rows = vec![
            "=====  =====".to_string(),
            "A      B".to_string(),
            "=====  =====".to_string(),
            "1      2".to_string(),
            "=====  =====".to_string(),
            "3      4".to_string(),
            "=====  =====".to_string(),
        ];
        let mut diagnostics = Vec::new();

        // When
        let rule = find_head_body_rule(&mut rows, 0, &mut diagnostics);

        // Then
        assert_eq!(rule, HeadBodyRule::Ambiguous);
        assert!(diagnostics[0].contains("multiple head/body row separators"));
    }
}
