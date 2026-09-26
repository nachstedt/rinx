//! Grid-table parsing: resolving the `+---+---+` / `|` character art into
//! rows and cells, using [`super::geometry`] for the column/divider maths.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::{indent_width, strip_indent};
use rinx_ast::{Diagnostic, DiagnosticCode, Node, TableRow};
use std::collections::BTreeMap;

use super::geometry::{
    ColumnRun, GridCtx, RowKind, build_row_cells, classify_row, column_runs, is_border_line,
    local_dividers,
};

/// Resolves one row-block (the content lines strictly between two
/// consecutive border lines `top` and `bottom`) into zero or more
/// [`TableRow`]s — more than one when the row-block contains row-spans, per
/// [`column_runs`]/[`run_rowspan`].
fn build_row_block(ctx: &mut GridCtx<'_>, top: usize, bottom: usize) -> Option<Vec<TableRow>> {
    let n = bottom - top - 1;
    let ncols = ctx.col_bounds.len() - 1;

    if n == 0 {
        return Some(Vec::new());
    }

    let divs_per_col: Vec<Vec<usize>> = (0..ncols)
        .map(|c| local_dividers(ctx.grid, top, n, ctx.col_bounds, c))
        .collect();

    let mut all_divs: Vec<usize> = divs_per_col.iter().flatten().copied().collect();
    all_divs.sort_unstable();
    all_divs.dedup();

    let mut rows_by_start: BTreeMap<usize, Vec<ColumnRun>> = BTreeMap::new();
    for (c, divs) in divs_per_col.iter().enumerate() {
        for (start, end) in column_runs(n, divs) {
            rows_by_start.entry(start).or_default().push(ColumnRun {
                column: c,
                start,
                end,
            });
        }
    }

    let mut rows = Vec::new();
    for raw_cells in rows_by_start.into_values() {
        let cells = build_row_cells(ctx, top, &raw_cells, &all_divs)?;
        rows.push(TableRow { cells });
    }
    Some(rows)
}

/// Collects the table's physical block: every consecutive non-blank line from
/// `start_i`, with the common indent stripped. Validates that all lines share
/// the top line's indent and exact width (a mismatch means a `+`/`|` boundary
/// is misaligned), and that the block is terminated by a border line. Returns
/// the stripped rows, or `None` (with a diagnostic) if malformed.
fn collect_grid_rows(
    lines: &[&str],
    start_i: usize,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Vec<String>> {
    let first_line = lines[start_i].trim_end();
    let indent = indent_width(first_line);
    let expected_width = strip_indent(first_line, indent).chars().count();

    let mut end = start_i;
    while end < lines.len() && !lines[end].trim().is_empty() {
        end += 1;
    }

    let mut raw_rows: Vec<String> = Vec::new();
    for (offset, &raw_line) in lines[start_i..end].iter().enumerate() {
        let line = raw_line.trim_end();
        let this_indent = indent_width(line);
        if this_indent != indent {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::TableGridInconsistentIndent,
                format!(
                    "grid table: inconsistent indentation (expected {indent}, got {this_indent})"
                ),
                ctx.line_span(start_i + offset, line),
            ));
            return None;
        }
        let content = strip_indent(line, indent);
        if content.chars().count() != expected_width {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::TableGridWidthMismatch,
                format!(
                    "grid table: width ({}) does not match the top border's width ({}) — a `+`/`|` column boundary is misaligned",
                    content.chars().count(),
                    expected_width,
                ),
                ctx.line_span(start_i + offset, line),
            ));
            return None;
        }
        raw_rows.push(content.to_string());
    }

    if raw_rows.len() < 2 || !is_border_line(raw_rows.last().expect("checked len >= 2"), true) {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::TableGridUnterminated,
            "grid table: not terminated by a border line",
            ctx.lines_span(start_i, end - 1, lines[end - 1]),
        ));
        return None;
    }

    Some(raw_rows)
}

/// Scans `raw_rows` for its border-line row indices and locates the at-most-
/// one `=` header/body divider. Returns `(border_row_indices,
/// header_separator_index)`, or `None` (with a diagnostic) if more than one
/// `=` divider is found or there's no row content between borders at all.
///
/// Deliberately does *not* check a border line's `+` positions against the
/// table's full `col_bounds`: a border line legitimately shows *fewer* `+`
/// than the full column set whenever columns are merged (colspan) right at
/// that boundary — see [`try_parse_grid_table`]'s doc comment for why this
/// can't be validated more strictly.
fn classify_border_rows(
    raw_rows: &[String],
    lines: &[&str],
    start_i: usize,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<(Vec<usize>, Option<usize>)> {
    let mut header_sep_idx: Option<usize> = None;
    let mut border_indices: Vec<usize> = Vec::new();
    for (r, raw_row) in raw_rows.iter().enumerate() {
        let RowKind::Border { has_equals } = classify_row(raw_row) else {
            continue;
        };
        if has_equals {
            if header_sep_idx.is_some() {
                diagnostics.push(Diagnostic::at(
                    DiagnosticCode::TableGridMultipleHeaderSeparators,
                    "grid table: more than one header/body separator (`=` line) found",
                    ctx.line_span(start_i + r, lines[start_i + r]),
                ));
                return None;
            }
            header_sep_idx = Some(r);
        }
        border_indices.push(r);
    }

    if border_indices.len() < 2 {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::TableGridNoRows,
            "grid table: no row content between borders",
            ctx.lines_span(
                start_i,
                start_i + raw_rows.len() - 1,
                lines[start_i + raw_rows.len() - 1],
            ),
        ));
        return None;
    }

    Some((border_indices, header_sep_idx))
}

/// Tries to parse an RST grid table (the `+---+---+` / `|` / `=` ASCII-art
/// syntax) starting at line `i`.
///
/// Column boundaries are derived from the union of every `+` found anywhere
/// in the table, not just the top border: real-world tables (e.g. a
/// hierarchical header whose sub-columns only appear on the header's second
/// line) can introduce a finer column split partway through, and a `+` is
/// unambiguously a column-boundary marker wherever it occurs. Row-blocks
/// that predate a given split simply read as an implicit colspan across the
/// not-yet-split columns, via the same "no `|` at this boundary" mechanism
/// [`build_row_cells`] already uses. One consequence: a stray/typo `+` that
/// doesn't line up with any other row's structure is silently absorbed as a
/// new column boundary rather than rejected — there's no way to distinguish
/// that from a deliberate finer split by structure alone, and real
/// Sphinx/docutils has the same ambiguity.
///
/// On any other structural inconsistency (unterminated table, mismatched
/// line widths, more than one header/body divider, or a malformed colspan
/// boundary), pushes a diagnostic and returns `None` so `parse_blocks` falls
/// back to treating the lines as an ordinary paragraph — consistent with
/// this parser's error-resilience policy.
pub(crate) fn try_parse_grid_table(
    lines: &[&str],
    start_i: usize,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<(usize, Node)> {
    let first_line = lines[start_i].trim_end();
    let indent = indent_width(first_line);
    if !is_border_line(strip_indent(first_line, indent), false) {
        return None;
    }

    let raw_rows = collect_grid_rows(lines, start_i, diagnostics, ctx)?;
    let grid: Vec<Vec<char>> = raw_rows.iter().map(|r| r.chars().collect()).collect();

    let mut col_bounds: Vec<usize> = grid
        .iter()
        .flat_map(|row| {
            row.iter()
                .enumerate()
                .filter(|&(_, &c)| c == '+')
                .map(|(idx, _)| idx)
        })
        .collect();
    col_bounds.sort_unstable();
    col_bounds.dedup();
    if col_bounds.len() < 2 {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::TableGridNoColumns,
            "grid table: top border defines no columns",
            ctx.line_span(start_i, first_line),
        ));
        return None;
    }

    let (border_indices, header_sep_idx) =
        classify_border_rows(&raw_rows, lines, start_i, diagnostics, ctx)?;

    let mut grid_ctx = GridCtx {
        grid: &grid,
        col_bounds: &col_bounds,
        adornment_order,
        diagnostics,
        parse_ctx: ctx,
        start_i,
        lines,
    };

    let mut header_rows = Vec::new();
    let mut body_rows = Vec::new();
    for window in border_indices.windows(2) {
        let (top, bottom) = (window[0], window[1]);
        let rows = build_row_block(&mut grid_ctx, top, bottom)?;
        if header_sep_idx.is_some_and(|h| bottom <= h) {
            header_rows.extend(rows);
        } else {
            body_rows.extend(rows);
        }
    }

    if body_rows.is_empty() {
        grid_ctx.diagnostics.push(Diagnostic::at(
            DiagnosticCode::TableGridNoBodyRows,
            "grid table: has no body rows",
            ctx.lines_span(
                start_i,
                start_i + raw_rows.len() - 1,
                lines[start_i + raw_rows.len() - 1],
            ),
        ));
        return None;
    }

    Some((
        raw_rows.len(),
        Node::Table {
            header_rows,
            body_rows,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rinx_ast::InlineNode;

    // --- try_parse_grid_table via parse() integration tests ---

    #[test]
    fn test_parse_grid_table_with_header() {
        // Given a table with one header row and one body row, no spans
        let input = "\
+------+------+
| A    | B    |
+======+======+
| a1   | b1   |
+------+------+";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        let Node::Table {
            header_rows,
            body_rows,
        } = &doc.nodes[0]
        else {
            panic!("Expected Table, got {:?}", doc.nodes[0]);
        };
        assert_eq!(header_rows.len(), 1);
        assert_eq!(body_rows.len(), 1);
        assert_eq!(header_rows[0].cells.len(), 2);
        assert_eq!(body_rows[0].cells.len(), 2);
        assert_eq!(header_rows[0].cells[0].colspan, 1);
        assert_eq!(header_rows[0].cells[0].rowspan, 1);
        if let Node::Paragraph(inlines) = &header_rows[0].cells[0].content[0] {
            assert_eq!(inlines[0], InlineNode::Text("A".to_string()));
        } else {
            panic!("Expected Paragraph cell content");
        }
        if let Node::Paragraph(inlines) = &body_rows[0].cells[1].content[0] {
            assert_eq!(inlines[0], InlineNode::Text("b1".to_string()));
        } else {
            panic!("Expected Paragraph cell content");
        }
    }

    #[test]
    fn test_parse_grid_table_without_header_row() {
        // Given — the spec says header rows are optional
        let input = "\
+------+------+
| a1   | b1   |
+------+------+
| a2   | b2   |
+------+------+";

        // When
        let doc = parse("test.rst", input);

        // Then
        let Node::Table {
            header_rows,
            body_rows,
        } = &doc.nodes[0]
        else {
            panic!("Expected Table, got {:?}", doc.nodes[0]);
        };
        assert!(header_rows.is_empty());
        assert_eq!(body_rows.len(), 2);
    }

    #[test]
    fn test_parse_grid_table_column_span() {
        // Given the spec's "Cells may span columns." example, reproduced
        // with original text
        let input = "\
+--------------+----------+-----------+-----------+
| row 1, col 1 | column 2 | column 3  | column 4  |
+--------------+----------+-----------+-----------+
| row 2        | Cells may span columns.          |
+--------------+----------+-----------+-----------+";

        // When
        let doc = parse("test.rst", input);

        // Then
        let Node::Table { body_rows, .. } = &doc.nodes[0] else {
            panic!("Expected Table, got {:?}", doc.nodes[0]);
        };
        assert_eq!(body_rows.len(), 2);
        let spanning_row = &body_rows[1];
        assert_eq!(spanning_row.cells.len(), 2);
        assert_eq!(spanning_row.cells[0].colspan, 1);
        assert_eq!(spanning_row.cells[1].colspan, 3);
        assert_eq!(spanning_row.cells[1].rowspan, 1);
        if let Node::Paragraph(inlines) = &spanning_row.cells[1].content[0] {
            assert_eq!(
                inlines[0],
                InlineNode::Text("Cells may span columns.".to_string())
            );
        } else {
            panic!("Expected Paragraph cell content");
        }
    }

    #[test]
    fn test_parse_grid_table_row_span() {
        // Given the spec's row-span example (column 1 spans two grid rows,
        // column 0 does not), reproduced with original text. There are two
        // row-blocks here: "row 1, col 1"/"column 2" (no spans), then
        // "row 2"/"row 3" where column 1's cell spans both.
        let input = "\
+--------------+------------+
| row 1, col 1 | column 2   |
+--------------+------------+
| row 2        | Cells span |
+--------------+ two rows.  |
| row 3        |            |
+--------------+------------+";

        // When
        let doc = parse("test.rst", input);

        // Then
        let Node::Table { body_rows, .. } = &doc.nodes[0] else {
            panic!("Expected Table, got {:?}", doc.nodes[0]);
        };
        assert_eq!(body_rows.len(), 3);
        assert_eq!(body_rows[0].cells.len(), 2);

        // Second row-block: column 0 has its own "row 2" cell (rowspan 1)...
        assert_eq!(body_rows[1].cells.len(), 2);
        assert_eq!(body_rows[1].cells[0].rowspan, 1);
        if let Node::Paragraph(inlines) = &body_rows[1].cells[0].content[0] {
            assert_eq!(inlines[0], InlineNode::Text("row 2".to_string()));
        } else {
            panic!("Expected Paragraph cell content");
        }
        // ...while column 1's cell spans both logical rows.
        assert_eq!(body_rows[1].cells[1].rowspan, 2);
        if let Node::Paragraph(inlines) = &body_rows[1].cells[1].content[0] {
            assert_eq!(
                inlines[0],
                InlineNode::Text("Cells span\ntwo rows.".to_string())
            );
        } else {
            panic!("Expected Paragraph cell content");
        }

        // The third logical row only contributes column 0's cell — column 1
        // is already covered by the row-spanning cell above.
        assert_eq!(body_rows[2].cells.len(), 1);
        if let Node::Paragraph(inlines) = &body_rows[2].cells[0].content[0] {
            assert_eq!(inlines[0], InlineNode::Text("row 3".to_string()));
        } else {
            panic!("Expected Paragraph cell content");
        }
    }

    #[test]
    fn test_parse_grid_table_rejects_unterminated_table() {
        // Given a table missing its closing border line — note the last
        // line is deliberately the same width as the rest, so this
        // exercises the "not terminated" check specifically, not the
        // width-mismatch check.
        let input = "+------+\n| a    |\n| a    |";

        // When
        let doc = parse("test.rst", input);

        // Then — falls back to a paragraph, with a diagnostic explaining why
        assert!(!matches!(doc.nodes[0], Node::Table { .. }));
        assert!(
            doc.diagnostics
                .iter()
                .any(|d| d.message.contains("not terminated by a border line"))
        );
    }

    #[test]
    fn test_parse_grid_table_rejects_misaligned_column_boundary() {
        // Given a table where one cell line is one character too wide,
        // shifting its trailing "|" out of alignment with the rest
        let input = "+------+------+\n| a    | b     |\n+------+------+";

        // When
        let doc = parse("test.rst", input);

        // Then — falls back to a paragraph, with a diagnostic naming the
        // misaligned line
        assert!(!matches!(doc.nodes[0], Node::Table { .. }));
        assert!(
            doc.diagnostics
                .iter()
                .any(|d| d.message.contains("does not match the top border's width"))
        );
    }

    #[test]
    fn test_parse_grid_table_hierarchical_header_introduces_column_split_partway_through() {
        // Given a trimmed reproduction of a real CPython docs table
        // (Doc/c-api/apiabiversion.rst, "Bit-packing macros"): the top
        // border only shows 5 columns, and the header's second line
        // introduces a brand-new "+" boundary splitting the last column
        // into two sub-columns ("3.4.1a2" / "3.10.0"). The "=" divider and
        // every row below it already use that finer 6-column structure.
        let input = "\
+------------------+-------+----------------+-----------+--------------------------+
|                  | No.   |                |           | Example values           |
|                  | of    |                |           +-------------+------------+
| Argument         | bits  | Bit mask       | Bit shift | ``3.4.1a2`` | ``3.10.0`` |
+==================+=======+================+===========+=============+============+
| *major*          |   8   | ``0xFF000000`` | 24        | ``0x03``    | ``0x03``   |
+------------------+-------+----------------+-----------+-------------+------------+";

        // When
        let doc = parse("test.rst", input);

        // Then — no diagnostics, and the header resolves into two logical
        // rows: one with four rowspan-2 cells plus a colspan-2 cell, and one
        // with the two sub-column cells.
        assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
        let Node::Table {
            header_rows,
            body_rows,
        } = &doc.nodes[0]
        else {
            panic!("Expected Table, got {:?}", doc.nodes[0]);
        };
        assert_eq!(header_rows.len(), 2);
        assert_eq!(header_rows[0].cells.len(), 5);
        for cell in &header_rows[0].cells[..4] {
            assert_eq!(cell.rowspan, 2);
            assert_eq!(cell.colspan, 1);
        }
        assert_eq!(header_rows[0].cells[4].colspan, 2);
        assert_eq!(header_rows[0].cells[4].rowspan, 1);
        assert_eq!(header_rows[1].cells.len(), 2);
        assert_eq!(body_rows.len(), 1);
        assert_eq!(body_rows[0].cells.len(), 6);
    }

    #[test]
    fn test_parse_grid_table_row_block_before_a_split_reads_as_implicit_colspan() {
        // Given a table whose top border only shows 2 columns, but a later
        // border (between the two body rows) introduces a 3rd column by
        // splitting the second one. The earlier row's "wide" cell has no
        // "|" at that not-yet-established boundary, so it must be resolved
        // as spanning both of the finer columns once the global column grid
        // is known.
        let input = "\
+-----+----------+
| A   | wide     |
+-----+-----+----+
| B   | C   | D  |
+-----+-----+----+";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
        let Node::Table { body_rows, .. } = &doc.nodes[0] else {
            panic!("Expected Table, got {:?}", doc.nodes[0]);
        };
        assert_eq!(body_rows.len(), 2);
        assert_eq!(body_rows[0].cells.len(), 2);
        assert_eq!(body_rows[0].cells[0].colspan, 1);
        assert_eq!(body_rows[0].cells[1].colspan, 2);
        assert_eq!(body_rows[1].cells.len(), 3);
        for cell in &body_rows[1].cells {
            assert_eq!(cell.colspan, 1);
        }
    }
}
