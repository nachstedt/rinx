//! `.. list-table::` and `.. csv-table::` directive rendering.
//!
//! Both parse into one `Directive::DataTable`, so both render through this
//! module; `source` decides only which CSS class the `<table>` carries.

use rinx_ast::{TableAlign, TableRow, TableSource, TableWidths, TargetName};
use std::fmt::Write as _;

use super::table_shell::{
    render_table_caption, render_table_colgroup, render_table_name_anchor, render_table_open_tag,
};
use crate::RenderCtx;

/// The fields `render_data_table` needs, borrowed straight from
/// [`rinx_ast::Directive::DataTable`] — grouped into one struct
/// (rather than ten separate parameters) purely to keep the function's
/// arity reasonable.
#[derive(Clone, Copy)]
pub(super) struct DataTableParams<'a> {
    pub source: TableSource,
    pub title: Option<&'a str>,
    pub header_rows: usize,
    pub stub_columns: usize,
    pub widths: Option<&'a TableWidths>,
    pub width: Option<&'a str>,
    pub align: Option<TableAlign>,
    pub classes: &'a [String],
    pub name: Option<&'a TargetName>,
    /// The `numfig` number its caption starts with, if it has one.
    pub number: Option<&'a str>,
    pub rows: &'a [TableRow],
}

/// Renders a `.. list-table::` or `.. csv-table::` directive as HTML,
/// reusing the grid-table row/cell rendering machinery
/// ([`super::tables::render_table_cell`]) so every table form shares one code
/// path for the actual `<td>`/`<th>` output.
pub(super) fn render_data_table(
    html: &mut String,
    params: DataTableParams<'_>,
    ctx: &mut RenderCtx<'_>,
) {
    let DataTableParams {
        source,
        title,
        header_rows,
        stub_columns,
        widths,
        width,
        align,
        classes,
        name,
        number,
        rows,
    } = params;

    render_table_name_anchor(html, name);

    // The directive's own name is the table's first class, so a stylesheet
    // can target `.list-table` and `.csv-table` separately even though the two
    // render identically otherwise.
    let mut class_list = vec![source.as_str().to_string()];
    class_list.extend(classes.iter().cloned());
    render_table_open_tag(html, &class_list, align, width);

    render_table_caption(html, title, number);
    render_table_colgroup(html, widths);

    // Defensively re-clamp: `header_rows` is already clamped to `rows.len()`
    // at parse time, but nothing at the type level stops a directly
    // constructed `Directive::DataTable` (e.g. loaded from a hand-edited
    // `.ast` file) from violating that, and slicing `rows[..header_rows]`
    // below would otherwise panic.
    let header_rows = header_rows.min(rows.len());

    if header_rows > 0 {
        let _ = writeln!(html, "<thead>");
        for (row_idx, row) in rows[..header_rows].iter().enumerate() {
            render_data_table_row(html, row, row_idx, header_rows, stub_columns, ctx);
        }
        let _ = writeln!(html, "</thead>");
    }
    let _ = writeln!(html, "<tbody>");
    for (offset, row) in rows[header_rows..].iter().enumerate() {
        render_data_table_row(
            html,
            row,
            header_rows + offset,
            header_rows,
            stub_columns,
            ctx,
        );
    }
    let _ = writeln!(html, "</tbody>");
    let _ = writeln!(html, "</table>");
}

/// Renders one data-table row, picking `th`/`td` per cell rather than
/// uniformly per row (unlike grid tables): a cell is a header (`th`) when
/// its row is one of the leading `header_rows`, or when its column is one
/// of the leading `stub_columns` — the latter also gets `scope="row"`
/// (skipped for a cell that's already a header via `header_rows`, since its
/// header-ness there is a column header, not a row header).
fn render_data_table_row(
    html: &mut String,
    row: &TableRow,
    row_idx: usize,
    header_rows: usize,
    stub_columns: usize,
    ctx: &mut RenderCtx<'_>,
) {
    let _ = writeln!(html, "<tr>");
    let is_header_row = row_idx < header_rows;
    for (col_idx, cell) in row.cells.iter().enumerate() {
        let is_stub_column = col_idx < stub_columns;
        let tag = if is_header_row || is_stub_column {
            "th"
        } else {
            "td"
        };
        let scope = (!is_header_row && is_stub_column).then_some("row");
        super::tables::render_table_cell(html, cell, tag, scope, ctx);
    }
    let _ = writeln!(html, "</tr>");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::table_test_support::{render_doc, table_row};
    use rinx_ast::{Directive, Document, Node};

    /// A minimal table carrying only `source`, for the tests that care about
    /// nothing else.
    fn data_table(source: TableSource) -> Document {
        Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DataTable {
                source,
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: None,
                rows: vec![table_row(&["Fruit", "Colour"])],
            })],
        )
    }

    #[test]
    fn test_render_data_table_uses_the_list_table_class_for_a_list_source() {
        // Given
        let doc = data_table(TableSource::List);

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<table class=\"list-table\">"), "{result}");
    }

    #[test]
    fn test_render_data_table_uses_the_csv_table_class_for_a_csv_source() {
        // Given
        let doc = data_table(TableSource::Csv);

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<table class=\"csv-table\">"), "{result}");
    }

    #[test]
    fn test_render_data_table_puts_extra_classes_after_the_source_class() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DataTable {
                source: TableSource::Csv,
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec!["compact".to_string()],
                name: None,
                rows: vec![table_row(&["Fruit"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("class=\"csv-table compact\""), "{result}");
    }

    #[test]
    fn test_render_data_table_basic_two_by_two() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DataTable {
                source: TableSource::List,
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: None,
                rows: vec![
                    table_row(&["Fruit", "Colour"]),
                    table_row(&["Apple", "Red"]),
                ],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<table class=\"list-table\">"));
        assert!(!result.contains("<thead>"));
        assert!(result.contains("<td>"));
        assert!(result.contains("<p>Fruit</p>"));
    }
    #[test]
    fn test_render_data_table_header_rows_produces_thead() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DataTable {
                source: TableSource::List,
                title: None,
                header_rows: 1,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: None,
                rows: vec![
                    table_row(&["Fruit", "Colour"]),
                    table_row(&["Apple", "Red"]),
                ],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<thead>"));
        assert!(result.contains("<th>"));
        assert!(result.contains("<tbody>"));
    }
    #[test]
    fn test_render_data_table_stub_columns_produces_mixed_th_td_in_body_row() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DataTable {
                source: TableSource::List,
                title: None,
                header_rows: 0,
                stub_columns: 1,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: None,
                rows: vec![table_row(&["Stub", "Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<th scope=\"row\">"));
        assert!(result.contains("<td>"));
    }
    #[test]
    fn test_render_data_table_title_produces_caption() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DataTable {
                source: TableSource::List,
                title: Some("Fruit".to_string()),
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: None,
                rows: vec![table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<caption>Fruit</caption>"));
    }
    #[test]
    fn test_render_data_table_explicit_widths_produces_colgroup() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DataTable {
                source: TableSource::List,
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: Some(TableWidths::Explicit(vec![30, 70])),
                width: None,
                align: None,
                classes: vec![],
                name: None,
                rows: vec![table_row(&["A", "B"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<colgroup>"));
        assert!(result.contains("<col style=\"width: 30.00%\" />"));
        assert!(result.contains("<col style=\"width: 70.00%\" />"));
    }
    #[test]
    fn test_render_data_table_width_option_produces_inline_style() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DataTable {
                source: TableSource::List,
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: Some("50%".to_string()),
                align: None,
                classes: vec![],
                name: None,
                rows: vec![table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("style=\"width: 50%\""));
    }
    #[test]
    fn test_render_data_table_align_option_produces_class() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DataTable {
                source: TableSource::List,
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: Some(TableAlign::Center),
                classes: vec![],
                name: None,
                rows: vec![table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("class=\"list-table align-center\""));
    }
    #[test]
    fn test_render_data_table_class_option_appends_extra_classes() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DataTable {
                source: TableSource::List,
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec!["custom".to_string()],
                name: None,
                rows: vec![table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("class=\"list-table custom\""));
    }
    #[test]
    fn test_render_data_table_name_option_produces_anchor() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DataTable {
                source: TableSource::List,
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: Some(TargetName::new("fruit-table")),
                rows: vec![table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<a id=\"fruit-table\"></a>"));
    }
    #[test]
    fn test_render_table_cell_extraction_matches_grid_table_output_byte_for_byte() {
        // Given — the same grid-table shape `test_render_table_with_header`
        // exercises, verifying the `render_table_cell` extraction changed
        // nothing about grid-table rendering.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Table {
                header_rows: vec![table_row(&["Fruit", "Colour"])],
                body_rows: vec![table_row(&["Apple", "Red"])],
            }],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert_eq!(
            result,
            "<table>\n\
             <thead>\n<tr>\n<th><p>Fruit</p>\n</th>\n<th><p>Colour</p>\n</th>\n</tr>\n</thead>\n\
             <tbody>\n<tr>\n<td><p>Apple</p>\n</td>\n<td><p>Red</p>\n</td>\n</tr>\n</tbody>\n\
             </table>\n"
        );
    }
}
