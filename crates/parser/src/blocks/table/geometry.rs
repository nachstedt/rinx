use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::headings::Adornment;
use rusty_sphinx_ast::TableCell;

pub(super) fn is_border_line(line: &str, allow_equals: bool) -> bool {
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
pub(super) enum RowKind {
    Border { has_equals: bool },
    Content,
}

/// Classifies a single physical line of a grid table's character block.
pub(super) fn classify_row(line: &str) -> RowKind {
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
pub(super) fn local_dividers(
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
pub(super) fn column_runs(n: usize, divs: &[usize]) -> Vec<(usize, usize)> {
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
pub(super) fn run_rowspan(all_divs: &[usize], start: usize, end: usize) -> usize {
    1 + all_divs.iter().filter(|&&d| d > start && d < end).count()
}

/// One column's contribution to a row-block, before colspan merging.
#[derive(Debug, Clone, Copy)]
pub(super) struct ColumnRun {
    pub(super) column: usize,
    pub(super) start: usize,
    pub(super) end: usize,
}

/// Shared state threaded through the row-block/cell resolution: the immutable
/// character grid and column boundaries, plus the mutable parse state that
/// recursive cell-content parsing needs. Mirrors the renderer's `RenderCtx`
/// pattern and keeps helper signatures small.
pub(super) struct GridCtx<'a> {
    pub(super) grid: &'a [Vec<char>],
    pub(super) col_bounds: &'a [usize],
    pub(super) adornment_order: &'a mut Vec<Adornment>,
    pub(super) diagnostics: &'a mut Vec<String>,
    pub(super) parse_ctx: &'a ParseCtx<'a>,
}

/// Extracts a resolved cell's text, spanning grid columns `[col_start,
/// col_end)` and raw offsets `[row_start, row_end]` within one row-block.
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
    let cell_lines: Vec<String> = (row_start..=row_end)
        .map(|offset| grid[top + 1 + offset][left..right].iter().collect())
        .collect();

    normalize_cell_lines(cell_lines)
}

/// Normalizes the raw character rectangle sliced out for one table cell:
/// strips the shared leading margin and trailing whitespace per line (RST
/// spec: cell margins "are removed before processing"), and trims leading/
/// trailing blank lines. Shared with [`super::simple_table`], whose cells are
/// delimited by whitespace columns rather than `|` but need the exact same
/// treatment once sliced.
pub(crate) fn normalize_cell_lines(mut cell_lines: Vec<String>) -> Vec<String> {
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
pub(super) fn build_row_cells(
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
            ctx.parse_ctx,
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
