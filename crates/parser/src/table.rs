use super::blocks::parse_blocks;
use super::bullet_list::strip_indent;
use super::headings::Adornment;
use rusty_sphinx_ast::{Domain, Node, TableCell, TableRow};
use std::collections::BTreeMap;

fn leading_whitespace_count(line: &str) -> usize {
    line.chars().take_while(|c| c.is_whitespace()).count()
}

/// Checks whether `line` is a full grid-table border line: starts and ends
/// with `+`, and every character in between is `-`/`+` (or additionally `=`
/// when `allow_equals`, for the header/body divider line).
fn is_border_line(line: &str, allow_equals: bool) -> bool {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() < 3 {
        return false;
    }
    if chars[0] != '+' || *chars.last().expect("checked len >= 3") != '+' {
        return false;
    }
    chars[1..chars.len() - 1]
        .iter()
        .all(|&c| c == '+' || c == '-' || (allow_equals && c == '='))
}

#[derive(Debug, Clone, Copy)]
enum RowKind {
    Border { has_equals: bool },
    Content,
}

/// Classifies a single physical line of a grid table's character block.
fn classify_row(line: &str) -> RowKind {
    if is_border_line(line, true) {
        RowKind::Border {
            has_equals: line.contains('='),
        }
    } else {
        RowKind::Content
    }
}

/// Finds every raw offset in `0..n` at which `column`'s own local segment is
/// itself a border (a `+` at both column boundaries with only `-` between
/// them). These offsets are the row-span boundaries specific to this column;
/// other columns may hold real text on that very same physical line, which is
/// exactly how a row-span arises.
fn local_dividers(
    grid: &[Vec<char>],
    top: usize,
    n: usize,
    col_bounds: &[usize],
    column: usize,
) -> Vec<usize> {
    let left = col_bounds[column];
    let right = col_bounds[column + 1];
    (0..n)
        .filter(|&offset| {
            let row = &grid[top + 1 + offset];
            row[left] == '+' && row[right] == '+' && row[left + 1..right].iter().all(|&c| c == '-')
        })
        .collect()
}

/// Splits `0..n` into a column's cell runs (inclusive `(start, end)` raw
/// offset ranges), skipping over that column's own local-divider offsets
/// (which are structural, not part of any cell's content).
fn column_runs(n: usize, divs: &[usize]) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut cursor = 0usize;
    for &d in divs {
        if cursor < d {
            runs.push((cursor, d - 1));
        }
        cursor = d + 1;
    }
    if cursor < n {
        runs.push((cursor, n - 1));
    }
    runs
}

/// The number of logical rows a run `(start, end)` spans, given the union of
/// every column's local-divider offsets in this row-block: any such divider
/// strictly inside the run means some other column split there, so this run
/// spans across that row boundary too (a row-span).
fn run_rowspan(all_divs: &[usize], start: usize, end: usize) -> usize {
    1 + all_divs.iter().filter(|&&d| d > start && d < end).count()
}

/// One column's contribution to a row-block, before colspan merging.
#[derive(Debug, Clone, Copy)]
struct ColumnRun {
    column: usize,
    start: usize,
    end: usize,
}

/// Shared state threaded through the row-block/cell resolution: the immutable
/// character grid and column boundaries, plus the mutable parse state that
/// recursive cell-content parsing needs. Mirrors the renderer's `RenderCtx`
/// pattern and keeps helper signatures small.
struct GridCtx<'a> {
    grid: &'a [Vec<char>],
    col_bounds: &'a [usize],
    adornment_order: &'a mut Vec<Adornment>,
    diagnostics: &'a mut Vec<String>,
    default_domain: Domain,
}

/// Extracts a resolved cell's text, spanning grid columns `[col_start,
/// col_end)` and raw offsets `[row_start, row_end]` within one row-block.
/// Strips the shared leading margin and trailing whitespace per line (RST
/// spec: cell margins "are removed before processing"), and trims leading/
/// trailing blank lines.
fn extract_cell_text(
    grid: &[Vec<char>],
    top: usize,
    col_bounds: &[usize],
    col_start: usize,
    col_end: usize,
    row_start: usize,
    row_end: usize,
) -> Vec<String> {
    let left = col_bounds[col_start] + 1;
    let right = col_bounds[col_end];
    let mut cell_lines: Vec<String> = (row_start..=row_end)
        .map(|offset| grid[top + 1 + offset][left..right].iter().collect())
        .collect();

    let min_indent = cell_lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.chars().take_while(|c| c.is_whitespace()).count())
        .min()
        .unwrap_or(0);

    for line in &mut cell_lines {
        if line.trim().is_empty() {
            line.clear();
        } else {
            *line = line.chars().skip(min_indent).collect::<String>();
            let trimmed_len = line.trim_end().len();
            line.truncate(trimmed_len);
        }
    }

    while cell_lines.first().is_some_and(String::is_empty) {
        cell_lines.remove(0);
    }
    while cell_lines.last().is_some_and(String::is_empty) {
        cell_lines.pop();
    }

    cell_lines
}

/// Resolves one logical row's worth of [`ColumnRun`]s (all sharing the same
/// start offset) into [`TableCell`]s, merging horizontally-adjacent runs
/// with matching extents into a single colspan when the boundary between
/// them isn't a real `|`.
fn build_row_cells(
    ctx: &mut GridCtx<'_>,
    top: usize,
    raw_cells: &[ColumnRun],
    all_divs: &[usize],
) -> Option<Vec<TableCell>> {
    let mut cells = Vec::new();
    let mut idx = 0;
    while idx < raw_cells.len() {
        let ColumnRun { column, start, end } = raw_cells[idx];
        let rowspan = run_rowspan(all_divs, start, end);
        let mut colspan = 1usize;
        let mut next_idx = idx + 1;

        while next_idx < raw_cells.len() {
            let next = raw_cells[next_idx];
            if next.column != column + colspan {
                break;
            }
            if (next.start, next.end) != (start, end) {
                break;
            }
            let boundary_col = ctx.col_bounds[column + colspan];
            let is_real =
                (start..=end).all(|offset| ctx.grid[top + 1 + offset][boundary_col] == '|');
            if is_real {
                break;
            }
            let is_absorbed =
                (start..=end).all(|offset| ctx.grid[top + 1 + offset][boundary_col] != '|');
            if !is_absorbed {
                ctx.diagnostics.push(format!(
                    "grid table: inconsistent column boundary between columns {} and {}",
                    column + colspan - 1,
                    column + colspan,
                ));
                return None;
            }
            colspan += 1;
            next_idx += 1;
        }

        let content_lines = extract_cell_text(
            ctx.grid,
            top,
            ctx.col_bounds,
            column,
            column + colspan,
            start,
            end,
        );
        let content_refs: Vec<&str> = content_lines.iter().map(String::as_str).collect();
        let content = parse_blocks(
            &content_refs,
            ctx.adornment_order,
            ctx.diagnostics,
            ctx.default_domain,
        );
        cells.push(TableCell {
            colspan,
            rowspan,
            content,
        });
        idx = next_idx;
    }
    Some(cells)
}

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
    diagnostics: &mut Vec<String>,
) -> Option<Vec<String>> {
    let first_line = lines[start_i].trim_end();
    let indent = leading_whitespace_count(first_line);
    let expected_width = strip_indent(first_line, indent).chars().count();

    let mut end = start_i;
    while end < lines.len() && !lines[end].trim().is_empty() {
        end += 1;
    }

    let mut raw_rows: Vec<String> = Vec::new();
    for (offset, &raw_line) in lines[start_i..end].iter().enumerate() {
        let line = raw_line.trim_end();
        let this_indent = leading_whitespace_count(line);
        if this_indent != indent {
            diagnostics.push(format!(
                "grid table: line {} has inconsistent indentation (expected {indent}, got {this_indent})",
                start_i + offset + 1,
            ));
            return None;
        }
        let content = strip_indent(line, indent);
        if content.chars().count() != expected_width {
            diagnostics.push(format!(
                "grid table: line {} width ({}) does not match the top border's width ({}) — a `+`/`|` column boundary is misaligned",
                start_i + offset + 1,
                content.chars().count(),
                expected_width,
            ));
            return None;
        }
        raw_rows.push(content.to_string());
    }

    if raw_rows.len() < 2 || !is_border_line(raw_rows.last().expect("checked len >= 2"), true) {
        diagnostics.push("grid table: not terminated by a border line".to_string());
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
    diagnostics: &mut Vec<String>,
) -> Option<(Vec<usize>, Option<usize>)> {
    let mut header_sep_idx: Option<usize> = None;
    let mut border_indices: Vec<usize> = Vec::new();
    for (r, raw_row) in raw_rows.iter().enumerate() {
        let RowKind::Border { has_equals } = classify_row(raw_row) else {
            continue;
        };
        if has_equals {
            if header_sep_idx.is_some() {
                diagnostics.push(
                    "grid table: more than one header/body separator (`=` line) found".to_string(),
                );
                return None;
            }
            header_sep_idx = Some(r);
        }
        border_indices.push(r);
    }

    if border_indices.len() < 2 {
        diagnostics.push("grid table: no row content between borders".to_string());
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
pub(super) fn try_parse_grid_table(
    lines: &[&str],
    start_i: usize,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> Option<(usize, Node)> {
    let first_line = lines[start_i].trim_end();
    let indent = leading_whitespace_count(first_line);
    if !is_border_line(strip_indent(first_line, indent), false) {
        return None;
    }

    let raw_rows = collect_grid_rows(lines, start_i, diagnostics)?;
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
        diagnostics.push("grid table: top border defines no columns".to_string());
        return None;
    }

    let (border_indices, header_sep_idx) = classify_border_rows(&raw_rows, diagnostics)?;

    let mut ctx = GridCtx {
        grid: &grid,
        col_bounds: &col_bounds,
        adornment_order,
        diagnostics,
        default_domain,
    };

    let mut header_rows = Vec::new();
    let mut body_rows = Vec::new();
    for window in border_indices.windows(2) {
        let (top, bottom) = (window[0], window[1]);
        let rows = build_row_block(&mut ctx, top, bottom)?;
        if header_sep_idx.is_some_and(|h| bottom <= h) {
            header_rows.extend(rows);
        } else {
            body_rows.extend(rows);
        }
    }

    if body_rows.is_empty() {
        ctx.diagnostics
            .push("grid table: has no body rows".to_string());
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
    use rusty_sphinx_ast::InlineNode;

    // --- is_border_line ---

    #[test]
    fn test_is_border_line_accepts_plain_dash_border() {
        assert!(is_border_line("+---+---+", false));
    }

    #[test]
    fn test_is_border_line_rejects_missing_plus_at_start() {
        assert!(!is_border_line("----+---+", false));
    }

    #[test]
    fn test_is_border_line_rejects_missing_plus_at_end() {
        assert!(!is_border_line("+---+----", false));
    }

    #[test]
    fn test_is_border_line_rejects_content_characters() {
        assert!(!is_border_line("+---+ a +", false));
    }

    #[test]
    fn test_is_border_line_rejects_equals_when_not_allowed() {
        assert!(!is_border_line("+===+===+", false));
    }

    #[test]
    fn test_is_border_line_accepts_equals_when_allowed() {
        assert!(is_border_line("+===+===+", true));
    }

    #[test]
    fn test_is_border_line_rejects_too_short_line() {
        assert!(!is_border_line("++", false));
    }

    // --- classify_row ---

    #[test]
    fn test_classify_row_identifies_plain_border() {
        assert!(matches!(
            classify_row("+---+---+"),
            RowKind::Border { has_equals: false }
        ));
    }

    #[test]
    fn test_classify_row_identifies_equals_border() {
        assert!(matches!(
            classify_row("+===+===+"),
            RowKind::Border { has_equals: true }
        ));
    }

    #[test]
    fn test_classify_row_identifies_content_line() {
        assert!(matches!(
            classify_row("| text   | more |"),
            RowKind::Content
        ));
    }

    // --- local_dividers ---

    #[test]
    fn test_local_dividers_finds_none_when_column_has_no_internal_border() {
        // Given a 2-line row-block where column 0 never shows a local border
        // (every line is exactly 9 chars wide, matching the top border)
        let grid: Vec<Vec<char>> = vec![
            "+---+---+".chars().collect(),
            "| a | b |".chars().collect(),
            "| c | d |".chars().collect(),
            "+---+---+".chars().collect(),
        ];
        let col_bounds = vec![0, 4, 8];

        // When
        let divs = local_dividers(&grid, 0, 2, &col_bounds, 0);

        // Then
        assert!(divs.is_empty());
    }

    #[test]
    fn test_local_dividers_finds_offset_where_column_has_internal_border() {
        // Given column 0 has a real "+-----+" style border on the middle
        // line, while column 1's text continues through it (a row-span for
        // column 1); every line is exactly 13 chars wide.
        let grid: Vec<Vec<char>> = vec![
            "+-----+-----+".chars().collect(),
            "|top  |x1   |".chars().collect(),
            "+-----+x2   |".chars().collect(),
            "|bot  |     |".chars().collect(),
            "+-----+-----+".chars().collect(),
        ];
        let col_bounds = vec![0, 6, 12];

        // When
        let divs_col0 = local_dividers(&grid, 0, 3, &col_bounds, 0);
        let divs_col1 = local_dividers(&grid, 0, 3, &col_bounds, 1);

        // Then
        assert_eq!(divs_col0, vec![1]);
        assert!(divs_col1.is_empty());
    }

    // --- column_runs ---

    #[test]
    fn test_column_runs_no_dividers_yields_single_run() {
        assert_eq!(column_runs(3, &[]), vec![(0, 2)]);
    }

    #[test]
    fn test_column_runs_single_divider_splits_into_two_runs() {
        assert_eq!(column_runs(3, &[1]), vec![(0, 0), (2, 2)]);
    }

    #[test]
    fn test_column_runs_divider_at_start_yields_only_trailing_run() {
        assert_eq!(column_runs(3, &[0]), vec![(1, 2)]);
    }

    #[test]
    fn test_column_runs_divider_at_end_yields_only_leading_run() {
        assert_eq!(column_runs(3, &[2]), vec![(0, 1)]);
    }

    // --- run_rowspan ---

    #[test]
    fn test_run_rowspan_is_one_with_no_interior_dividers() {
        assert_eq!(run_rowspan(&[], 0, 0), 1);
    }

    #[test]
    fn test_run_rowspan_counts_interior_dividers_from_other_columns() {
        assert_eq!(run_rowspan(&[1], 0, 2), 2);
    }

    #[test]
    fn test_run_rowspan_ignores_dividers_outside_the_run() {
        assert_eq!(run_rowspan(&[5], 0, 2), 1);
    }

    // --- extract_cell_text ---

    #[test]
    fn test_extract_cell_text_strips_shared_margin() {
        let grid: Vec<Vec<char>> = vec![
            "+-----+".chars().collect(),
            "|  hi |".chars().collect(),
            "|  yo |".chars().collect(),
            "+-----+".chars().collect(),
        ];
        let col_bounds = vec![0, 6];

        let lines = extract_cell_text(&grid, 0, &col_bounds, 0, 1, 0, 1);

        assert_eq!(lines, vec!["hi".to_string(), "yo".to_string()]);
    }

    #[test]
    fn test_extract_cell_text_trims_leading_and_trailing_blank_lines() {
        let grid: Vec<Vec<char>> = vec![
            "+-----+".chars().collect(),
            "|     |".chars().collect(),
            "| hi  |".chars().collect(),
            "|     |".chars().collect(),
            "+-----+".chars().collect(),
        ];
        let col_bounds = vec![0, 6];

        let lines = extract_cell_text(&grid, 0, &col_bounds, 0, 1, 0, 2);

        assert_eq!(lines, vec!["hi".to_string()]);
    }

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
                .any(|d| d.contains("not terminated by a border line"))
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
                .any(|d| d.contains("does not match the top border's width"))
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
