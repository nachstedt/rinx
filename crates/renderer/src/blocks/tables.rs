//! Grid-table row/cell rendering, shared with `list_table` for the
//! actual `<td>`/`<th>` output.

use rinx_ast::TableRow;
use std::fmt::Write as _;

use super::render_nodes;
use crate::RenderCtx;

/// Renders a bare grid or simple table — [`rinx_ast::Node::Table`],
/// as opposed to the `.. table::`-wrapped or `list-table`/`csv-table` forms,
/// which have their own presentation shells and call [`render_table_row`]
/// directly instead.
pub(crate) fn render_table(
    html: &mut String,
    header_rows: &[TableRow],
    body_rows: &[TableRow],
    ctx: &mut RenderCtx<'_>,
) {
    let _ = writeln!(html, "<table>");
    if !header_rows.is_empty() {
        let _ = writeln!(html, "<thead>");
        for row in header_rows {
            render_table_row(html, row, "th", ctx);
        }
        let _ = writeln!(html, "</thead>");
    }
    let _ = writeln!(html, "<tbody>");
    for row in body_rows {
        render_table_row(html, row, "td", ctx);
    }
    let _ = writeln!(html, "</tbody>");
    let _ = writeln!(html, "</table>");
}

/// Renders a single grid-table row, emitting each cell with the given tag
/// (`th` for header rows, `td` for body rows) via [`render_table_cell`].
pub(crate) fn render_table_row(
    html: &mut String,
    row: &TableRow,
    cell_tag: &str,
    ctx: &mut RenderCtx<'_>,
) {
    let _ = writeln!(html, "<tr>");
    for cell in &row.cells {
        render_table_cell(html, cell, cell_tag, None, ctx);
    }
    let _ = writeln!(html, "</tr>");
}

/// Renders a single table cell with the given tag (`th`/`td`) and an
/// optional `scope` attribute (used by `list-table`'s `:stub-columns:` to
/// mark a stub cell as a row header; grid tables never pass one).
/// `colspan`/`rowspan` attributes are written only when greater than 1,
/// matching how `LiteralBlock` only emits its optional `language` attribute
/// when present.
pub(crate) fn render_table_cell(
    html: &mut String,
    cell: &rinx_ast::TableCell,
    tag: &str,
    scope: Option<&str>,
    ctx: &mut RenderCtx<'_>,
) {
    let _ = write!(html, "<{tag}");
    if cell.colspan > 1 {
        let _ = write!(html, " colspan=\"{}\"", cell.colspan);
    }
    if cell.rowspan > 1 {
        let _ = write!(html, " rowspan=\"{}\"", cell.rowspan);
    }
    if let Some(scope) = scope {
        let _ = write!(html, " scope=\"{scope}\"");
    }
    let _ = write!(html, ">");
    render_nodes(html, &cell.content, ctx);
    let _ = writeln!(html, "</{tag}>");
}

#[cfg(test)]
mod tests {
    use crate::render;
    use rinx_ast::{Document, InlineNode, Node};
    use rinx_index::ProjectIndex;

    #[test]
    fn test_render_table_with_header() {
        // Given a table with a header row and a body row, no spans
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Table {
                header_rows: vec![rinx_ast::TableRow {
                    cells: vec![
                        rinx_ast::TableCell {
                            colspan: 1,
                            rowspan: 1,
                            content: vec![Node::Paragraph(vec![InlineNode::Text("A".to_string())])],
                        },
                        rinx_ast::TableCell {
                            colspan: 1,
                            rowspan: 1,
                            content: vec![Node::Paragraph(vec![InlineNode::Text("B".to_string())])],
                        },
                    ],
                }],
                body_rows: vec![rinx_ast::TableRow {
                    cells: vec![
                        rinx_ast::TableCell {
                            colspan: 1,
                            rowspan: 1,
                            content: vec![Node::Paragraph(vec![InlineNode::Text(
                                "a1".to_string(),
                            )])],
                        },
                        rinx_ast::TableCell {
                            colspan: 1,
                            rowspan: 1,
                            content: vec![Node::Paragraph(vec![InlineNode::Text(
                                "b1".to_string(),
                            )])],
                        },
                    ],
                }],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(
            result,
            "<table>\n\
             <thead>\n<tr>\n<th><p>A</p>\n</th>\n<th><p>B</p>\n</th>\n</tr>\n</thead>\n\
             <tbody>\n<tr>\n<td><p>a1</p>\n</td>\n<td><p>b1</p>\n</td>\n</tr>\n</tbody>\n\
             </table>\n"
        );
    }
    #[test]
    fn test_render_table_without_header_omits_thead() {
        // Given a header-less table
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Table {
                header_rows: vec![],
                body_rows: vec![rinx_ast::TableRow {
                    cells: vec![rinx_ast::TableCell {
                        colspan: 1,
                        rowspan: 1,
                        content: vec![Node::Paragraph(vec![InlineNode::Text("only".to_string())])],
                    }],
                }],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then — no <thead> element at all
        assert!(!result.contains("<thead>"));
        assert_eq!(
            result,
            "<table>\n<tbody>\n<tr>\n<td><p>only</p>\n</td>\n</tr>\n</tbody>\n</table>\n"
        );
    }
    #[test]
    fn test_render_table_emits_colspan_and_rowspan_attributes() {
        // Given a body cell spanning 2 columns and 3 rows
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Table {
                header_rows: vec![],
                body_rows: vec![rinx_ast::TableRow {
                    cells: vec![rinx_ast::TableCell {
                        colspan: 2,
                        rowspan: 3,
                        content: vec![Node::Paragraph(vec![InlineNode::Text(
                            "spanning".to_string(),
                        )])],
                    }],
                }],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then — attributes present with the correct values
        assert!(result.contains("<td colspan=\"2\" rowspan=\"3\"><p>spanning</p>\n</td>"));
    }
}
