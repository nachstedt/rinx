//! The simple-table parse itself: walking the table's lines, delimiting rows
//! at each column-span underline, and lowering each row into cells.

use crate::context::ParseCtx;
use crate::headings::Adornment;
use crate::indent::{indent_width, strip_indent};
use rusty_sphinx_ast::{Node, TableRow};

use super::borders::{is_simple_table_top, parse_column_spans};
use super::layout::{HeadBodyRule, collect_simple_table_lines, find_head_body_rule};
use super::rows::{SimpleTableCtx, build_rows};

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
pub(crate) fn try_parse_simple_table(
    lines: &[&str],
    start_i: usize,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    ctx: &ParseCtx<'_>,
) -> Option<(usize, Node)> {
    let first_line = lines[start_i].trim_end();
    let indent = indent_width(first_line);
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
        parse_ctx: ctx,
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
    use rusty_sphinx_ast::{InlineNode, TableCell};

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
