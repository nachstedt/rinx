use super::borders::{ColumnSpan, is_span_line, parse_column_spans};
use crate::blocks::parse_blocks;
use crate::blocks::table::normalize_cell_lines;
use crate::context::ParseCtx;
use crate::headings::Adornment;
use rusty_sphinx_ast::{TableCell, TableRow};

/// The immutable table geometry plus the mutable parse state that recursive
/// cell-content parsing needs. Mirrors [`super::table`]'s `GridCtx`.
pub(super) struct SimpleTableCtx<'a> {
    /// The character grid of the table's lines, indentation already stripped.
    pub(super) grid: &'a [Vec<char>],
    /// The table's own column layout, taken from the top border.
    pub(super) columns: &'a [ColumnSpan],
    /// Document line index of the table's top border, so diagnostics can name
    /// real source lines rather than table-relative ones.
    pub(super) start_i: usize,
    pub(super) adornment_order: &'a mut Vec<Adornment>,
    pub(super) diagnostics: &'a mut Vec<String>,
    pub(super) parse_ctx: &'a ParseCtx<'a>,
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
            ctx.parse_ctx,
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
pub(super) fn build_rows(
    ctx: &mut SimpleTableCtx<'_>,
    row_count: usize,
) -> Option<Vec<(usize, TableRow)>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::Domain;

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
                parse_ctx: &ParseCtx::with_domain(Domain::Py),
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
}
