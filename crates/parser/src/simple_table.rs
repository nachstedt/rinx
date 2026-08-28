//! Parsing for RST *simple tables* — the whitespace-column `=====  =====`
//! syntax, as opposed to the `+---+---+` ASCII art handled by
//! [`super::table`]. Both produce the same [`Node::Table`].
//!
//! The structure follows docutils' `SimpleTableParser`, including its central
//! trick: the top border, the header/body rule and the bottom border are all
//! rewritten from `=` to `-` up front, so every structural line in the table
//! is uniformly a *column-span underline* and one code path handles all of
//! them. A row's column layout always comes from the line that terminates it.
//!
//! Simple tables cannot express row spans, so every cell here has
//! `rowspan: 1`.

use super::blocks::parse_blocks;
use super::bullet_list::{leading_whitespace_count, strip_indent};
use super::headings::Adornment;
use super::table::normalize_cell_lines;
use rusty_sphinx_ast::{Domain, Node, TableCell, TableRow};

/// One column's `[start, end)` extent, in character offsets into the table's
/// indentation-stripped lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ColumnSpan {
    start: usize,
    end: usize,
}

/// Matches docutils' `simple_table_border_pat` / `head_body_separator_pat`
/// (`=+[ =]*$`): a line built only from runs of `=` separated by spaces.
/// Recognizes the top border, the header/body rule and the bottom border
/// alike — which of the three a given line is depends on where it sits.
fn is_equals_border(line: &str) -> bool {
    line.starts_with('=') && line.chars().all(|c| c == '=' || c == ' ')
}

/// Matches docutils' `span_pat` (`-[ -]*$`): a column-span underline.
fn is_span_line(line: &str) -> bool {
    line.starts_with('-') && line.chars().all(|c| c == '-' || c == ' ')
}

/// Matches docutils' `simple_table_top_pat` (`=+( +=+)+ *$`).
///
/// Requiring *two or more* `=` runs is what keeps this syntax disjoint from a
/// section adornment and from a transition, both of which are a single run of
/// one repeated character. Because the two languages cannot overlap, the
/// order in which `parse_blocks` tries them is irrelevant — a single-column
/// `=====` block stays a heading, exactly as in docutils.
fn is_simple_table_top(line: &str) -> bool {
    is_equals_border(line) && parse_column_spans(line).len() >= 2
}

/// docutils' `parse_columns`: the `[start, end)` char extents of every run of
/// non-space characters in a border or span line.
fn parse_column_spans(line: &str) -> Vec<ColumnSpan> {
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
fn collect_simple_table_lines(
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
enum HeadBodyRule {
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
fn find_head_body_rule(
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

/// The immutable table geometry plus the mutable parse state that recursive
/// cell-content parsing needs. Mirrors [`super::table`]'s `GridCtx`.
struct SimpleTableCtx<'a> {
    /// The character grid of the table's lines, indentation already stripped.
    grid: &'a [Vec<char>],
    /// The table's own column layout, taken from the top border.
    columns: &'a [ColumnSpan],
    /// Document line index of the table's top border, so diagnostics can name
    /// real source lines rather than table-relative ones.
    start_i: usize,
    adornment_order: &'a mut Vec<Adornment>,
    diagnostics: &'a mut Vec<String>,
    default_domain: Domain,
}

impl SimpleTableCtx<'_> {
    /// The 1-based document line number of the table line at `offset`.
    fn line_number(&self, offset: usize) -> usize {
        self.start_i + offset + 1
    }

    /// The `end` offset of the table's rightmost column (docutils'
    /// `border_end`), which every span line must reach exactly.
    fn border_end(&self) -> usize {
        self.columns
            .last()
            .expect("a simple table top border has at least two columns")
            .end
    }

    /// Slices `[from, to)` out of the table line at `offset`, clamped to that
    /// line's length — table lines are ragged, since trailing whitespace is
    /// stripped.
    fn slice(&self, offset: usize, from: usize, to: usize) -> String {
        let row = &self.grid[offset];
        let from = from.min(row.len());
        let to = to.min(row.len()).max(from);
        row[from..to].iter().collect()
    }
}

/// Derives a row's column layout from the span line that terminates it
/// (docutils' `parse_columns` for a non-top line).
///
/// The last run must reach the table's right edge; otherwise the underline
/// leaves part of the row unaccounted for.
///
/// Deviation from docutils: it *widens* its rightmost column when a cell
/// overflows the border and reports that width in the table's `colspec`s. We
/// emit no column widths at all, and instead always slice the rightmost
/// column to the end of the line (see [`extract_cell_lines`]), which yields
/// the same cell text without the bookkeeping.
fn row_columns_from_span(ctx: &mut SimpleTableCtx<'_>, offset: usize) -> Option<Vec<ColumnSpan>> {
    let columns = parse_column_spans(&ctx.grid[offset].iter().collect::<String>());
    let last = columns
        .last()
        .expect("a span line starts with `-`, so it has at least one run");
    if last.end != ctx.border_end() {
        ctx.diagnostics.push(format!(
            "simple table: column span incomplete on line {} — it does not reach the table's right edge",
            ctx.line_number(offset),
        ));
        return None;
    }
    Some(columns)
}

/// docutils' `check_columns`: anything but whitespace in the gap between one
/// column's end and the next column's start is text that has drifted out of
/// its cell and would silently vanish when the row is sliced.
///
/// The rightmost column is deliberately unchecked: text there may overflow
/// past the border, which is legal and is picked up by [`extract_cell_lines`].
fn check_column_margins(
    ctx: &mut SimpleTableCtx<'_>,
    start: usize,
    end: usize,
    columns: &[ColumnSpan],
) -> Option<()> {
    for (index, column) in columns.iter().enumerate() {
        let Some(next) = columns.get(index + 1) else {
            continue;
        };
        for offset in start..end {
            if !ctx.slice(offset, column.end, next.start).trim().is_empty() {
                ctx.diagnostics.push(format!(
                    "simple table: text in the column margin on line {}",
                    ctx.line_number(offset),
                ));
                return None;
            }
        }
    }
    Some(())
}

/// docutils' `init_row`: maps each of the row's column extents onto the
/// table's own columns, yielding that cell's `colspan`. An extent that starts
/// or ends anywhere other than on a table column boundary means the span
/// underline is misaligned with the table.
fn resolve_colspans(
    ctx: &mut SimpleTableCtx<'_>,
    columns: &[ColumnSpan],
    offset: usize,
) -> Option<Vec<usize>> {
    let mut colspans = Vec::with_capacity(columns.len());
    let mut index = 0usize;

    for column in columns {
        if ctx.columns.get(index).map(|c| c.start) != Some(column.start) {
            ctx.diagnostics.push(format!(
                "simple table: column span on line {} is not aligned with the table's columns",
                ctx.line_number(offset),
            ));
            return None;
        }
        let mut colspan = 1usize;
        loop {
            match ctx.columns.get(index) {
                None => {
                    ctx.diagnostics.push(format!(
                        "simple table: column span on line {} is not aligned with the table's columns",
                        ctx.line_number(offset),
                    ));
                    return None;
                }
                Some(table_column) if table_column.end == column.end => break,
                Some(_) => {
                    index += 1;
                    colspan += 1;
                }
            }
        }
        colspans.push(colspan);
        index += 1;
    }

    Some(colspans)
}

/// Slices one cell's character rectangle out of the row's lines and
/// normalizes it. `unbounded` marks the row's rightmost cell, whose text may
/// run past the table's right edge.
fn extract_cell_lines(
    ctx: &SimpleTableCtx<'_>,
    start: usize,
    end: usize,
    column: ColumnSpan,
    unbounded: bool,
) -> Vec<String> {
    let lines = (start..end)
        .map(|offset| {
            let to = if unbounded {
                ctx.grid[offset].len()
            } else {
                column.end
            };
            ctx.slice(offset, column.start, to)
        })
        .collect();
    normalize_cell_lines(lines)
}

/// docutils' `parse_row`: turns the accumulated lines `[start, end)` into a
/// [`TableRow`], using the column layout of the span line that terminated the
/// row when there was one, and the table's own columns otherwise.
fn build_row(
    ctx: &mut SimpleTableCtx<'_>,
    start: usize,
    end: usize,
    span_line: Option<usize>,
) -> Option<TableRow> {
    let columns = match span_line {
        Some(offset) => row_columns_from_span(ctx, offset)?,
        None => ctx.columns.to_vec(),
    };
    check_column_margins(ctx, start, end, &columns)?;
    let colspans = resolve_colspans(ctx, &columns, span_line.unwrap_or(start))?;

    let last_index = columns.len() - 1;
    let mut cells = Vec::with_capacity(columns.len());
    for (index, (column, colspan)) in columns.iter().zip(colspans).enumerate() {
        let content_lines = extract_cell_lines(ctx, start, end, *column, index == last_index);
        let content_refs: Vec<&str> = content_lines.iter().map(String::as_str).collect();
        let content = parse_blocks(
            &content_refs,
            ctx.adornment_order,
            ctx.diagnostics,
            ctx.default_domain,
        );
        cells.push(TableCell {
            colspan,
            rowspan: 1,
            content,
        });
    }

    Some(TableRow { cells })
}

/// Walks the table's lines and flushes one row at a time (docutils'
/// `parse_table`). A row ends either at a span line — an underline, the
/// header rule, or the bottom border — or at the next line that has text in
/// the first column. Returns each row paired with the table-line offset it
/// started at, which is what the header/body split is decided on.
fn build_rows(ctx: &mut SimpleTableCtx<'_>, row_count: usize) -> Option<Vec<(usize, TableRow)>> {
    let first_column = ctx.columns[0];
    let mut rows = Vec::new();
    let mut start = 1usize;
    let mut text_found = false;

    for offset in 1..row_count {
        let line: String = ctx.grid[offset].iter().collect();
        if is_span_line(&line) {
            rows.push((start, build_row(ctx, start, offset, Some(offset))?));
            start = offset + 1;
            text_found = false;
        } else if !ctx
            .slice(offset, first_column.start, first_column.end)
            .trim()
            .is_empty()
        {
            if text_found && offset != start {
                rows.push((start, build_row(ctx, start, offset, None)?));
            }
            start = offset;
            text_found = true;
        } else if !text_found {
            // A line whose first-column cell is blank cannot open a row, so
            // docutils drops it silently. Warn instead: the author's text is
            // being discarded, and the fix (an escaped space in the first
            // cell) is not guessable from the rendered output.
            if !line.trim().is_empty() {
                ctx.diagnostics.push(format!(
                    "simple table: line {} has an empty first-column cell, so its content is dropped — use `\\ ` (an escaped space) to start a row with an empty first cell",
                    ctx.line_number(offset),
                ));
            }
            start = offset + 1;
        }
    }

    Some(rows)
}

/// Tries to parse an RST simple table (the whitespace-column `=====  =====`
/// syntax) starting at line `start_i`.
///
/// On any structural inconsistency — an unterminated table, a border that
/// disagrees with the top border, more than one header/body rule, a
/// misaligned or incomplete column-span underline, or text sitting in a
/// column margin — pushes a diagnostic and returns `None`, so `parse_blocks`
/// falls back to treating the lines as an ordinary paragraph. This matches
/// both the grid table parser and docutils, which renders a malformed table
/// as a literal block alongside an error.
pub(super) fn try_parse_simple_table(
    lines: &[&str],
    start_i: usize,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> Option<(usize, Node)> {
    let first_line = lines[start_i].trim_end();
    let indent = leading_whitespace_count(first_line);
    if !is_simple_table_top(strip_indent(first_line, indent)) {
        return None;
    }

    let mut raw_rows = collect_simple_table_lines(lines, start_i, diagnostics)?;
    let consumed = raw_rows.len();
    let head_body_rule = match find_head_body_rule(&mut raw_rows, start_i, diagnostics) {
        HeadBodyRule::Ambiguous => return None,
        HeadBodyRule::Absent => None,
        HeadBodyRule::At(index) => Some(index),
    };

    let grid: Vec<Vec<char>> = raw_rows.iter().map(|row| row.chars().collect()).collect();
    let columns = parse_column_spans(&raw_rows[0]);

    let mut ctx = SimpleTableCtx {
        grid: &grid,
        columns: &columns,
        start_i,
        adornment_order,
        diagnostics,
        default_domain,
    };
    let rows = build_rows(&mut ctx, raw_rows.len())?;

    // The bottom border always terminates a row, so there is always at least
    // one. Rows starting after the header rule are body rows; if the rule has
    // no row after it, every row stays a body row (docutils degrades the same
    // way), which is why the body can never come out empty.
    let first_body_row = rows
        .iter()
        .position(|(start, _)| head_body_rule.is_some_and(|rule| *start > rule))
        .unwrap_or(0);
    let mut all_rows: Vec<TableRow> = rows.into_iter().map(|(_, row)| row).collect();
    let body_rows = all_rows.split_off(first_body_row);

    Some((
        consumed,
        Node::Table {
            header_rows: all_rows,
            body_rows,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::InlineNode;

    /// Extracts the plain text of a cell whose content is a single paragraph.
    fn cell_text(cell: &TableCell) -> String {
        match &cell.content[..] {
            [Node::Paragraph(inlines)] => inlines
                .iter()
                .map(|inline| match inline {
                    InlineNode::Text(text) => text.clone(),
                    other => panic!("Expected Text, got {other:?}"),
                })
                .collect(),
            [] => String::new(),
            other => panic!("Expected a single paragraph, got {other:?}"),
        }
    }

    /// Unwraps the single table parsed from `input`.
    fn parse_table(input: &str) -> (Vec<TableRow>, Vec<TableRow>) {
        let doc = parse("test.rst", input);
        match doc.nodes.into_iter().next() {
            Some(Node::Table {
                header_rows,
                body_rows,
            }) => (header_rows, body_rows),
            other => panic!("Expected Table, got {other:?}"),
        }
    }

    // --- is_equals_border ---

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

    // --- row-level helpers, through a built context ---

    /// Builds a context over `rows` for exercising the row-level helpers.
    fn with_ctx<T>(
        rows: &[&str],
        body: impl FnOnce(&mut SimpleTableCtx<'_>) -> T,
    ) -> (T, Vec<String>) {
        let grid: Vec<Vec<char>> = rows.iter().map(|row| row.chars().collect()).collect();
        let columns = parse_column_spans(rows[0]);
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();
        let result = {
            let mut ctx = SimpleTableCtx {
                grid: &grid,
                columns: &columns,
                start_i: 0,
                adornment_order: &mut adornment_order,
                diagnostics: &mut diagnostics,
                default_domain: Domain::Py,
            };
            body(&mut ctx)
        };
        (result, diagnostics)
    }

    #[test]
    fn test_border_end_is_the_right_edge_of_the_last_column() {
        // Given a table whose columns are 5 and 6 wide
        let (border_end, _) = with_ctx(&["-----  ------"], |ctx| ctx.border_end());

        // Then
        assert_eq!(border_end, 13);
    }

    #[test]
    fn test_line_number_is_one_based_and_relative_to_the_document() {
        // Given a table starting on the eleventh line of a document
        let (number, _) = with_ctx(&["-----  -----", "1      2"], |ctx| {
            ctx.start_i = 10;
            ctx.line_number(1)
        });

        // Then — diagnostics name the real source line, not a table offset
        assert_eq!(number, 12);
    }

    #[test]
    fn test_slice_clamps_to_a_short_line() {
        // Given a table line shorter than the column being sliced — trailing
        // whitespace is stripped, so table lines are ragged
        let (text, _) = with_ctx(&["-----  -----", "1"], |ctx| ctx.slice(1, 7, 12));

        // Then
        assert_eq!(text, "");
    }

    #[test]
    fn test_row_columns_from_span_accepts_an_underline_reaching_the_right_edge() {
        // Given an underline covering the full table width
        let (columns, diagnostics) = with_ctx(&["-----  -----", "------------"], |ctx| {
            row_columns_from_span(ctx, 1)
        });

        // Then
        assert_eq!(columns, Some(vec![ColumnSpan { start: 0, end: 12 }]));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_row_columns_from_span_rejects_an_underline_stopping_short() {
        // Given an underline that ends before the table's right edge
        let (columns, diagnostics) = with_ctx(&["-----  -----", "-------"], |ctx| {
            row_columns_from_span(ctx, 1)
        });

        // Then
        assert!(columns.is_none());
        assert!(diagnostics[0].contains("column span incomplete"));
    }

    #[test]
    fn test_resolve_colspans_maps_matching_columns_to_single_cells() {
        // Given a row whose columns are the table's own
        let (colspans, diagnostics) = with_ctx(&["-----  -----", "1      2"], |ctx| {
            let columns = ctx.columns.to_vec();
            resolve_colspans(ctx, &columns, 1)
        });

        // Then
        assert_eq!(colspans, Some(vec![1, 1]));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_resolve_colspans_merges_a_spanning_underline() {
        // Given an underline covering both of the table's columns
        let (colspans, diagnostics) = with_ctx(&["-----  -----", "------------"], |ctx| {
            let columns = parse_column_spans("------------");
            resolve_colspans(ctx, &columns, 1)
        });

        // Then — one cell spanning two columns
        assert_eq!(colspans, Some(vec![2]));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_resolve_colspans_rejects_a_misaligned_start() {
        // Given an underline starting inside a column rather than on its edge
        let (colspans, diagnostics) = with_ctx(&["-----  -----", " -----------"], |ctx| {
            let columns = parse_column_spans(" -----------");
            resolve_colspans(ctx, &columns, 1)
        });

        // Then
        assert!(colspans.is_none());
        assert!(diagnostics[0].contains("not aligned with the table's columns"));
    }

    #[test]
    fn test_resolve_colspans_rejects_an_end_matching_no_column() {
        // Given an underline that stops inside the last column
        let (colspans, diagnostics) = with_ctx(&["-----  -----", "----------"], |ctx| {
            let columns = parse_column_spans("----------");
            resolve_colspans(ctx, &columns, 1)
        });

        // Then
        assert!(colspans.is_none());
        assert!(diagnostics[0].contains("not aligned with the table's columns"));
    }

    #[test]
    fn test_check_column_margins_accepts_a_clean_row() {
        // Given a row whose text stays inside its columns
        let (result, diagnostics) = with_ctx(&["-----  -----", "1      2"], |ctx| {
            let columns = ctx.columns.to_vec();
            check_column_margins(ctx, 1, 2, &columns)
        });

        // Then
        assert_eq!(result, Some(()));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_check_column_margins_rejects_text_between_columns() {
        // Given a row with text sitting in the gap between two columns
        let (result, diagnostics) = with_ctx(&["-----  -----", "1     x2"], |ctx| {
            let columns = ctx.columns.to_vec();
            check_column_margins(ctx, 1, 2, &columns)
        });

        // Then
        assert!(result.is_none());
        assert!(diagnostics[0].contains("text in the column margin on line 2"));
    }

    #[test]
    fn test_check_column_margins_allows_overflow_past_the_last_column() {
        // Given a row whose last cell runs past the table's right edge
        let (result, diagnostics) = with_ctx(&["-----  -----", "1      overflowing"], |ctx| {
            let columns = ctx.columns.to_vec();
            check_column_margins(ctx, 1, 2, &columns)
        });

        // Then — legal, and picked up when the cell is sliced
        assert_eq!(result, Some(()));
        assert!(diagnostics.is_empty());
    }

    // --- extract_cell_lines ---

    #[test]
    fn test_extract_cell_lines_slices_and_normalizes_a_bounded_cell() {
        // Given a two-line cell in the first column
        let (lines, _) = with_ctx(&["-----  -----", "1      a", "2      b"], |ctx| {
            extract_cell_lines(ctx, 1, 3, ColumnSpan { start: 0, end: 5 }, false)
        });

        // Then
        assert_eq!(lines, vec!["1".to_string(), "2".to_string()]);
    }

    #[test]
    fn test_extract_cell_lines_reads_an_unbounded_cell_to_end_of_line() {
        // Given a last-column cell whose text overflows the border
        let (lines, _) = with_ctx(&["-----  -----", "1      overflowing text"], |ctx| {
            extract_cell_lines(ctx, 1, 2, ColumnSpan { start: 7, end: 12 }, true)
        });

        // Then
        assert_eq!(lines, vec!["overflowing text".to_string()]);
    }

    // --- try_parse_simple_table, through `parse` ---

    #[test]
    fn test_parse_simple_table_with_a_header() {
        // Given the spec's introductory example
        let input = "\
=====  =====
col 1  col 2
=====  =====
1      Second column of row 1.
2      Second column of row 2.
=====  =====";

        // When
        let (header_rows, body_rows) = parse_table(input);

        // Then
        assert_eq!(header_rows.len(), 1);
        assert_eq!(cell_text(&header_rows[0].cells[0]), "col 1");
        assert_eq!(cell_text(&header_rows[0].cells[1]), "col 2");
        assert_eq!(body_rows.len(), 2);
        assert_eq!(cell_text(&body_rows[0].cells[0]), "1");
        assert_eq!(cell_text(&body_rows[1].cells[1]), "Second column of row 2.");
    }

    #[test]
    fn test_parse_simple_table_without_a_header() {
        // Given a table with no interior rule
        let input = "\
=====  =====
1      a
2      b
=====  =====";

        // When
        let (header_rows, body_rows) = parse_table(input);

        // Then — every row is a body row
        assert!(header_rows.is_empty());
        assert_eq!(body_rows.len(), 2);
    }

    #[test]
    fn test_parse_simple_table_joins_a_multi_line_cell() {
        // Given a row continued on the next line
        let input = "\
=====  =========================
1      Second column of row 1.
       Second line of paragraph.
=====  =========================";

        // When
        let (_, body_rows) = parse_table(input);

        // Then — the continuation joins the same cell's paragraph
        assert_eq!(body_rows.len(), 1);
        assert_eq!(
            cell_text(&body_rows[0].cells[1]),
            "Second column of row 1.\nSecond line of paragraph."
        );
    }

    #[test]
    fn test_parse_simple_table_parses_a_cell_as_a_miniature_document() {
        // Given a cell holding a bullet list separated by a blank line
        let input = "\
=====  ==========================
3      - Second column of row 3.

       - Second item in bullet
         list (row 3, column 2).
=====  ==========================";

        // When
        let (_, body_rows) = parse_table(input);

        // Then
        assert_eq!(body_rows.len(), 1);
        let Node::BulletList { items, .. } = &body_rows[0].cells[1].content[0] else {
            panic!(
                "Expected BulletList, got {:?}",
                body_rows[0].cells[1].content[0]
            );
        };
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_parse_simple_table_column_span_underline() {
        // Given the spec's span example: an underline merges the row above
        let input = "\
=====  =====
col 1  col 2
=====  =====
4 is a span
------------
5
=====  =====";

        // When
        let (header_rows, body_rows) = parse_table(input);

        // Then
        assert_eq!(header_rows.len(), 1);
        assert_eq!(body_rows.len(), 2);
        assert_eq!(body_rows[0].cells.len(), 1);
        assert_eq!(body_rows[0].cells[0].colspan, 2);
        assert_eq!(body_rows[0].cells[0].rowspan, 1);
        assert_eq!(cell_text(&body_rows[0].cells[0]), "4 is a span");
        assert_eq!(body_rows[1].cells.len(), 2);
        assert_eq!(body_rows[1].cells[0].colspan, 1);
    }

    #[test]
    fn test_parse_simple_table_emits_an_empty_row_for_a_span_above_the_bottom_border() {
        // Given a span underline sitting directly on the bottom border, so the
        // border closes a row that has no lines at all
        let input = "\
=====  =====
a span
------------
=====  =====";

        // When
        let (_, body_rows) = parse_table(input);

        // Then — the trailing empty row is docutils' behaviour too: a border
        // always closes the row above it, empty or not
        assert_eq!(body_rows.len(), 2);
        assert_eq!(body_rows[0].cells[0].colspan, 2);
        assert!(
            body_rows[1]
                .cells
                .iter()
                .all(|cell| cell.content.is_empty())
        );
    }

    #[test]
    fn test_parse_simple_table_allows_the_last_column_to_overflow() {
        // Given a last cell whose text runs past the border
        let input = "\
=====  =====
1      text that runs well past the border
=====  =====";

        // When
        let (_, body_rows) = parse_table(input);

        // Then
        assert_eq!(
            cell_text(&body_rows[0].cells[1]),
            "text that runs well past the border"
        );
    }

    #[test]
    fn test_parse_simple_table_renders_inline_markup_in_cells() {
        // Given emphasis inside a cell
        let input = "\
=====  ==========
a      *emphasis*
=====  ==========";

        // When
        let (_, body_rows) = parse_table(input);

        // Then
        let Node::Paragraph(inlines) = &body_rows[0].cells[1].content[0] else {
            panic!("Expected Paragraph");
        };
        assert_eq!(inlines[0], InlineNode::Emphasis("emphasis".to_string()));
    }

    #[test]
    fn test_parse_simple_table_leaves_an_empty_cell_empty() {
        // Given a row whose last cell has no text
        let input = "\
=====  =====
1
=====  =====";

        // When
        let (_, body_rows) = parse_table(input);

        // Then
        assert_eq!(body_rows[0].cells.len(), 2);
        assert!(body_rows[0].cells[1].content.is_empty());
    }

    #[test]
    fn test_parse_simple_table_consumes_only_the_table() {
        // Given a table followed by a paragraph
        let input = "\
=====  =====
1      2
=====  =====

After the table.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert!(matches!(doc.nodes[0], Node::Table { .. }));
        assert!(matches!(doc.nodes[1], Node::Paragraph(_)));
    }

    #[test]
    fn test_parse_simple_table_inside_a_directive_body() {
        // Given an indented table nested in an admonition
        let input = "\
.. note::

   =====  =====
   1      2
   =====  =====";

        // When
        let doc = parse("test.rst", input);

        // Then
        let Node::Directive(directive) = &doc.nodes[0] else {
            panic!("Expected Directive, got {:?}", doc.nodes[0]);
        };
        let body = match directive {
            rusty_sphinx_ast::Directive::Admonition { body, .. } => body,
            other => panic!("Expected Admonition, got {other:?}"),
        };
        assert!(matches!(body[0], Node::Table { .. }));
    }

    #[test]
    fn test_parse_single_equals_run_is_still_a_heading() {
        // Given a section underlined with `=`, which shares the simple table's
        // border character
        let input = "\
Title
=====

Body.";

        // When
        let doc = parse("test.rst", input);

        // Then — headings win because the two syntaxes are disjoint, not
        // because of dispatch order
        assert!(matches!(doc.nodes[0], Node::Heading { level: 1, .. }));
    }

    #[test]
    fn test_parse_simple_table_warns_when_a_first_column_cell_is_empty() {
        // Given a line that cannot open a row because its first cell is blank
        let input = "\
=====  =====
A      B
=====  =====
       orphaned
=====  =====";

        // When
        let doc = parse("test.rst", input);

        // Then — the table still parses, but the dropped text is reported
        assert!(matches!(doc.nodes[0], Node::Table { .. }));
        assert_eq!(doc.diagnostics.len(), 1);
        assert!(doc.diagnostics[0].contains("empty first-column cell"));
    }

    #[test]
    fn test_parse_malformed_simple_table_falls_back_to_a_paragraph() {
        // Given a table with text in a column margin
        let input = "\
=====  =====
1     x2
=====  =====";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert!(matches!(doc.nodes[0], Node::Paragraph(_)));
        assert!(doc.diagnostics[0].contains("text in the column margin"));
    }

    #[test]
    fn test_parse_simple_table_reports_an_incomplete_span_underline() {
        // Given an underline that stops short of the table's right edge
        let input = "\
=====  =====
a span
-------
1      2
=====  =====";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert!(matches!(doc.nodes[0], Node::Paragraph(_)));
        assert!(doc.diagnostics[0].contains("column span incomplete"));
    }
}
