//! `.. list-table::` directive rendering.

use rusty_sphinx_ast::{ListTableWidths, TableAlign, TableRow, TargetName};
use std::fmt::Write as _;

use super::RenderCtx;

/// The fields `render_list_table` needs, borrowed straight from
/// [`rusty_sphinx_ast::Directive::ListTable`] — grouped into one struct
/// (rather than ten separate parameters) purely to keep the function's
/// arity reasonable.
#[derive(Clone, Copy)]
pub(super) struct ListTableParams<'a> {
    pub title: Option<&'a str>,
    pub header_rows: usize,
    pub stub_columns: usize,
    pub widths: Option<&'a ListTableWidths>,
    pub width: Option<&'a str>,
    pub align: Option<TableAlign>,
    pub classes: &'a [String],
    pub name: Option<&'a TargetName>,
    pub rows: &'a [TableRow],
}

/// Renders a `.. list-table::` directive as HTML, reusing the grid-table
/// row/cell rendering machinery ([`super::render_table_cell`]) so both table
/// forms share one code path for the actual `<td>`/`<th>` output.
pub(super) fn render_list_table(
    html: &mut String,
    params: ListTableParams<'_>,
    ctx: &mut RenderCtx<'_>,
) {
    let ListTableParams {
        title,
        header_rows,
        stub_columns,
        widths,
        width,
        align,
        classes,
        name,
        rows,
    } = params;

    // A `:name:` anchor is emitted exactly like an explicit hyperlink target
    // (`Node::Target` with no `uri`, see `render_nodes`) — reusing that same
    // mechanism rather than inventing a new target-location concept.
    if let Some(target_name) = name {
        let escaped = html_escape::encode_text(target_name.as_str());
        let _ = writeln!(html, "<a id=\"{escaped}\"></a>");
    }

    let mut class_list = vec!["list-table".to_string()];
    class_list.extend(classes.iter().cloned());
    if let Some(align) = align {
        class_list.push(format!("align-{}", align.as_str()));
    }
    let class_string = class_list.join(" ");
    let class_attr = html_escape::encode_double_quoted_attribute(&class_string);
    let _ = write!(html, "<table class=\"{class_attr}\"");
    if let Some(width) = width {
        let width_escaped = html_escape::encode_double_quoted_attribute(width);
        let _ = write!(html, " style=\"width: {width_escaped}\"");
    }
    let _ = writeln!(html, ">");

    if let Some(title) = title {
        let title_escaped = html_escape::encode_text(title);
        let _ = writeln!(html, "<caption>{title_escaped}</caption>");
    }

    render_list_table_colgroup(html, widths);

    // Defensively re-clamp: `header_rows` is already clamped to `rows.len()`
    // at parse time, but nothing at the type level stops a directly
    // constructed `Directive::ListTable` (e.g. loaded from a hand-edited
    // `.ast` file) from violating that, and slicing `rows[..header_rows]`
    // below would otherwise panic.
    let header_rows = header_rows.min(rows.len());

    if header_rows > 0 {
        let _ = writeln!(html, "<thead>");
        for (row_idx, row) in rows[..header_rows].iter().enumerate() {
            render_list_table_row(html, row, row_idx, header_rows, stub_columns, ctx);
        }
        let _ = writeln!(html, "</thead>");
    }
    let _ = writeln!(html, "<tbody>");
    for (offset, row) in rows[header_rows..].iter().enumerate() {
        render_list_table_row(
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

/// Renders a `<colgroup>` for `:widths:`'s explicit-integer-list form,
/// normalizing the values as *relative* weights (per the spec) rather than
/// literal percentages. `Auto`/`Grid`/`None` all mean "let the renderer
/// decide" — no `<colgroup>` at all.
fn render_list_table_colgroup(html: &mut String, widths: Option<&ListTableWidths>) {
    let Some(ListTableWidths::Explicit(cols)) = widths else {
        return;
    };
    let total: u32 = cols.iter().sum();
    if total == 0 {
        return;
    }
    let _ = writeln!(html, "<colgroup>");
    for col in cols {
        let pct = f64::from(*col) * 100.0 / f64::from(total);
        let _ = writeln!(html, "<col style=\"width: {pct:.2}%\" />");
    }
    let _ = writeln!(html, "</colgroup>");
}

/// Renders one list-table row, picking `th`/`td` per cell rather than
/// uniformly per row (unlike grid tables): a cell is a header (`th`) when
/// its row is one of the leading `header_rows`, or when its column is one
/// of the leading `stub_columns` — the latter also gets `scope="row"`
/// (skipped for a cell that's already a header via `header_rows`, since its
/// header-ness there is a column header, not a row header).
fn render_list_table_row(
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
        crate::tables::render_table_cell(html, cell, tag, scope, ctx);
    }
    let _ = writeln!(html, "</tr>");
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{Directive, Document, InlineNode, Node};
    use rusty_sphinx_index::ProjectIndex;

    fn render_doc(doc: &Document) -> String {
        let index = ProjectIndex::default();
        crate::render(doc, &index, &doc.path).html
    }

    fn list_table_row(cells: &[&str]) -> TableRow {
        rusty_sphinx_ast::TableRow {
            cells: cells
                .iter()
                .map(|text| rusty_sphinx_ast::TableCell {
                    colspan: 1,
                    rowspan: 1,
                    content: vec![Node::Paragraph(vec![InlineNode::Text((*text).to_string())])],
                })
                .collect(),
        }
    }

    #[test]
    fn test_render_list_table_basic_two_by_two() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: None,
                rows: vec![
                    list_table_row(&["Fruit", "Colour"]),
                    list_table_row(&["Apple", "Red"]),
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
    fn test_render_list_table_header_rows_produces_thead() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: None,
                header_rows: 1,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: None,
                rows: vec![
                    list_table_row(&["Fruit", "Colour"]),
                    list_table_row(&["Apple", "Red"]),
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
    fn test_render_list_table_stub_columns_produces_mixed_th_td_in_body_row() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: None,
                header_rows: 0,
                stub_columns: 1,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: None,
                rows: vec![list_table_row(&["Stub", "Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<th scope=\"row\">"));
        assert!(result.contains("<td>"));
    }
    #[test]
    fn test_render_list_table_title_produces_caption() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: Some("Fruit".to_string()),
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: None,
                rows: vec![list_table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<caption>Fruit</caption>"));
    }
    #[test]
    fn test_render_list_table_explicit_widths_produces_colgroup() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: Some(ListTableWidths::Explicit(vec![30, 70])),
                width: None,
                align: None,
                classes: vec![],
                name: None,
                rows: vec![list_table_row(&["A", "B"])],
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
    fn test_render_list_table_width_option_produces_inline_style() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: Some("50%".to_string()),
                align: None,
                classes: vec![],
                name: None,
                rows: vec![list_table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("style=\"width: 50%\""));
    }
    #[test]
    fn test_render_list_table_align_option_produces_class() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: Some(TableAlign::Center),
                classes: vec![],
                name: None,
                rows: vec![list_table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("class=\"list-table align-center\""));
    }
    #[test]
    fn test_render_list_table_class_option_appends_extra_classes() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec!["custom".to_string()],
                name: None,
                rows: vec![list_table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("class=\"list-table custom\""));
    }
    #[test]
    fn test_render_list_table_name_option_produces_anchor() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: Some(TargetName::new("fruit-table")),
                rows: vec![list_table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<a id=\"fruit-table\"></a>"));
    }
    #[test]
    fn test_render_list_table_colgroup_skipped_for_auto_and_grid_widths() {
        // Given
        let mut html = String::new();

        // When
        render_list_table_colgroup(&mut html, Some(&ListTableWidths::Auto));
        render_list_table_colgroup(&mut html, Some(&ListTableWidths::Grid));
        render_list_table_colgroup(&mut html, None);

        // Then
        assert!(html.is_empty());
    }
    #[test]
    fn test_render_table_cell_extraction_matches_grid_table_output_byte_for_byte() {
        // Given — the same grid-table shape `test_render_table_with_header`
        // exercises, verifying the `render_table_cell` extraction changed
        // nothing about grid-table rendering.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Table {
                header_rows: vec![list_table_row(&["Fruit", "Colour"])],
                body_rows: vec![list_table_row(&["Apple", "Red"])],
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
