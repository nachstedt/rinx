//! `.. list-table::` — a table whose rows are a nested bullet list.

use crate::diagnostics::Diagnostics;
use rusty_sphinx_ast::{
    Diagnostic, DiagnosticCode, Directive, ListItem, Node, Span, TableCell, TableRow, TableSource,
    TableWidths,
};

use super::options::{SharedTableOptions, parse_shared_table_options};
use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::directives::body::join_body_lines;
use crate::directives::table_options::{report_unknown_options, scan_option_lines};
use crate::directives::table_widths::parse_widths_option;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;

/// Parses a `.. list-table::` directive: a table specified as a nested
/// bullet list (outer list = rows, each row's own bullet list = cells)
/// rather than character-art. `body_lines` is the raw, still-indented body
/// collected by `collect_directive_body`, exactly as every other
/// content-bearing directive parser receives it.
pub(in crate::directives) fn parse_list_table(
    argument: String,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let title = if argument.is_empty() {
        None
    } else {
        Some(argument.clone())
    };

    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, opt_idx) = scan_option_lines(&unindented_lines);
    let (options, unrecognized) =
        parse_shared_table_options(&option_lines, TableSource::List.as_str(), diagnostics, ctx);
    report_unknown_options(&unrecognized, TableSource::List.as_str(), diagnostics, ctx);

    let body_content: Vec<&str> = unindented_lines[opt_idx..]
        .iter()
        .map(String::as_str)
        .collect();
    let body_nodes = parse_blocks(&body_content, adornment_order, diagnostics, ctx);

    let [
        Node::BulletList {
            items: row_items, ..
        },
    ] = body_nodes.as_slice()
    else {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::TableDataNotABulletList,
            "list-table: directive body must be a single bullet list of rows",
            ctx.line_span(0, body_lines.first().unwrap_or(&"")),
        ));
        return unknown_list_table(argument, body_lines);
    };

    // Rows are lowered from already-parsed nodes, which carry no position of
    // their own, so every row-level diagnostic points at the directive body.
    let table_span = ctx.line_span(0, body_lines.first().unwrap_or(&""));
    let (rows, ncols) = lower_list_table_rows(row_items, diagnostics, table_span);
    build_list_table(title, options, rows, ncols, diagnostics, table_span)
}

/// Assembles the parsed pieces into the AST node, resolving `:widths:` (which
/// needs `ncols`) and clamping the two count options to the table's actual
/// dimensions.
///
/// list-table clamps where csv-table rejects: a bullet list has no
/// independent statement of its own size to contradict, so an oversized
/// `:header-rows:` is a harmless over-count rather than the dimension
/// mismatch docutils diagnoses for CSV data.
fn build_list_table(
    title: Option<String>,
    options: SharedTableOptions,
    rows: Vec<TableRow>,
    ncols: usize,
    diagnostics: &mut Diagnostics,
    span: Option<Span>,
) -> Directive {
    let widths: Option<TableWidths> = options
        .widths_raw
        .and_then(|raw| parse_widths_option(&raw, ncols, "list-table", diagnostics, span));

    Directive::DataTable {
        source: TableSource::List,
        title,
        header_rows: options.header_rows.min(rows.len()),
        stub_columns: options.stub_columns.min(ncols),
        widths,
        width: options.width,
        align: options.align,
        classes: options.classes,
        name: options.name,
        rows,
    }
}

/// Lowers the outer bullet list's row items into `TableRow`s, each row's own
/// nested bullet list becoming that row's cells (`colspan`/`rowspan` always
/// 1 — list-table has no span syntax). Returns the rows plus the column
/// count taken from the first row; a later row with a different cell count
/// gets a diagnostic but is still included with its own actual cell count
/// (HTML tolerates ragged rows fine, and there's no ambiguity to resolve
/// here unlike a grid table's character alignment).
fn lower_list_table_rows(
    row_items: &[ListItem],
    diagnostics: &mut Diagnostics,
    span: Option<Span>,
) -> (Vec<TableRow>, usize) {
    let mut rows = Vec::new();
    let mut expected_ncols: Option<usize> = None;
    for row_item in row_items {
        let [
            Node::BulletList {
                items: cell_items, ..
            },
        ] = row_item.nodes.as_slice()
        else {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::TableDataNotABulletList,
                "list-table: each row must itself be a bullet list of cells",
                span,
            ));
            continue;
        };
        let cells: Vec<TableCell> = cell_items
            .iter()
            .map(|cell_item| TableCell {
                colspan: 1,
                rowspan: 1,
                content: cell_item.nodes.clone(),
            })
            .collect();
        match expected_ncols {
            None => expected_ncols = Some(cells.len()),
            Some(n) if n != cells.len() => diagnostics.push(Diagnostic::at(
                DiagnosticCode::TableDataRowCellCount,
                format!(
                    "list-table: row has {} cell(s), expected {n} (from the table's first row) — cell counts should match across rows",
                    cells.len()
                ),
                span,
            )),
            _ => {}
        }
        rows.push(TableRow { cells });
    }
    let ncols = expected_ncols.unwrap_or(0);
    (rows, ncols)
}

fn unknown_list_table(argument: String, body_lines: &[&str]) -> Directive {
    Directive::Unknown {
        name: "list-table".to_string(),
        argument,
        body: join_body_lines(body_lines),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{Domain, InlineNode, TableAlign, TargetName};

    fn parse(body_lines: &[&str]) -> (Directive, Diagnostics) {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();
        let directive = parse_list_table(
            String::new(),
            body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
        (directive, diagnostics)
    }

    #[test]
    fn test_parse_list_table_basic_two_by_two() {
        // Given
        let body_lines = vec![
            "   * - Header 1",
            "     - Header 2",
            "   * - Row 1 Col 1",
            "     - Row 1 Col 2",
        ];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty());
        if let Directive::DataTable {
            source,
            header_rows,
            rows,
            ..
        } = directive
        {
            assert_eq!(source, TableSource::List);
            assert_eq!(header_rows, 0);
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].cells.len(), 2);
            assert_eq!(rows[1].cells.len(), 2);
        } else {
            panic!("Expected DataTable directive, got {directive:?}");
        }
    }

    #[test]
    fn test_parse_list_table_with_title() {
        // Given
        let body_lines = vec!["   * - Cell"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When
        let directive = parse_list_table(
            "My Title".to_string(),
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        if let Directive::DataTable { title, .. } = directive {
            assert_eq!(title, Some("My Title".to_string()));
        } else {
            panic!("Expected DataTable directive");
        }
    }

    #[test]
    fn test_parse_list_table_parses_header_rows_option() {
        // Given
        let body_lines = vec![
            "   :header-rows: 1",
            "",
            "   * - Header 1",
            "     - Header 2",
            "   * - Row 1 Col 1",
            "     - Row 1 Col 2",
        ];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty());
        if let Directive::DataTable { header_rows, .. } = directive {
            assert_eq!(header_rows, 1);
        } else {
            panic!("Expected DataTable directive");
        }
    }

    #[test]
    fn test_parse_list_table_parses_stub_columns_option() {
        // Given
        let body_lines = vec!["   :stub-columns: 1", "", "   * - Stub", "     - Cell"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty());
        if let Directive::DataTable { stub_columns, .. } = directive {
            assert_eq!(stub_columns, 1);
        } else {
            panic!("Expected DataTable directive");
        }
    }

    #[test]
    fn test_parse_list_table_widths_auto() {
        // Given
        let body_lines = vec!["   :widths: auto", "", "   * - Cell"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty());
        if let Directive::DataTable { widths, .. } = directive {
            assert_eq!(widths, Some(TableWidths::Auto));
        } else {
            panic!("Expected DataTable directive");
        }
    }

    #[test]
    fn test_parse_list_table_widths_grid() {
        // Given
        let body_lines = vec!["   :widths: grid", "", "   * - Cell"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty());
        if let Directive::DataTable { widths, .. } = directive {
            assert_eq!(widths, Some(TableWidths::Grid));
        } else {
            panic!("Expected DataTable directive");
        }
    }

    #[test]
    fn test_parse_list_table_widths_explicit_matching_count() {
        // Given
        let body_lines = vec!["   :widths: 30 70", "", "   * - A", "     - B"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty());
        if let Directive::DataTable { widths, .. } = directive {
            assert_eq!(widths, Some(TableWidths::Explicit(vec![30, 70])));
        } else {
            panic!("Expected DataTable directive");
        }
    }

    #[test]
    fn test_parse_list_table_widths_explicit_mismatched_count_emits_diagnostic() {
        // Given
        let body_lines = vec!["   :widths: 30 30 40", "", "   * - A", "     - B"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains(":widths:"));
        if let Directive::DataTable { widths, .. } = directive {
            assert_eq!(widths, None);
        } else {
            panic!("Expected DataTable directive");
        }
    }

    #[test]
    fn test_parse_list_table_parses_width_option() {
        // Given
        let body_lines = vec!["   :width: 50%", "", "   * - Cell"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty());
        if let Directive::DataTable { width, .. } = directive {
            assert_eq!(width, Some("50%".to_string()));
        } else {
            panic!("Expected DataTable directive");
        }
    }

    #[test]
    fn test_parse_list_table_parses_align_option() {
        // Given
        let body_lines = vec!["   :align: right", "", "   * - Cell"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty());
        if let Directive::DataTable { align, .. } = directive {
            assert_eq!(align, Some(TableAlign::Right));
        } else {
            panic!("Expected DataTable directive");
        }
    }

    #[test]
    fn test_parse_list_table_rejects_invalid_align_value() {
        // Given
        let body_lines = vec!["   :align: diagonal", "", "   * - Cell"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains(":align:"));
        if let Directive::DataTable { align, .. } = directive {
            assert_eq!(align, None);
        } else {
            panic!("Expected DataTable directive");
        }
    }

    #[test]
    fn test_parse_list_table_parses_class_option_splits_on_whitespace() {
        // Given
        let body_lines = vec!["   :class: foo bar", "", "   * - Cell"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty());
        if let Directive::DataTable { classes, .. } = directive {
            assert_eq!(classes, vec!["foo".to_string(), "bar".to_string()]);
        } else {
            panic!("Expected DataTable directive");
        }
    }

    #[test]
    fn test_parse_list_table_parses_name_option() {
        // Given
        let body_lines = vec!["   :name: My Table", "", "   * - Cell"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty());
        if let Directive::DataTable { name, .. } = directive {
            assert_eq!(name, Some(TargetName::new("My Table")));
        } else {
            panic!("Expected DataTable directive");
        }
    }

    #[test]
    fn test_parse_list_table_emits_diagnostic_for_unknown_option() {
        // Given
        let body_lines = vec!["   :bogus:", "", "   * - Cell"];

        // When
        let (_, diagnostics) = parse(&body_lines);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("Invalid or non-standard Sphinx list-table option")
        );
    }

    #[test]
    fn test_parse_list_table_emits_diagnostic_for_uneven_row_cell_counts_but_still_builds_table() {
        // Given
        let body_lines = vec!["   * - A", "     - B", "   * - C", "     - D", "     - E"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].message.contains("uneven")
                || diagnostics[0].message.contains("cell counts")
                || diagnostics[0].message.contains("expected")
        );
        if let Directive::DataTable { rows, .. } = directive {
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].cells.len(), 2);
            assert_eq!(rows[1].cells.len(), 3);
        } else {
            panic!("Expected DataTable directive");
        }
    }

    #[test]
    fn test_parse_list_table_clamps_header_rows_exceeding_actual_row_count() {
        // Given
        let body_lines = vec!["   :header-rows: 5", "", "   * - A", "   * - B"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty());
        if let Directive::DataTable {
            header_rows, rows, ..
        } = directive
        {
            assert_eq!(header_rows, rows.len());
        } else {
            panic!("Expected DataTable directive");
        }
    }

    #[test]
    fn test_parse_list_table_clamps_stub_columns_exceeding_actual_column_count() {
        // Given
        let body_lines = vec!["   :stub-columns: 5", "", "   * - A", "     - B"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty());
        if let Directive::DataTable { stub_columns, .. } = directive {
            assert_eq!(stub_columns, 2);
        } else {
            panic!("Expected DataTable directive");
        }
    }

    #[test]
    fn test_parse_list_table_falls_back_to_unknown_for_non_bullet_list_body() {
        // Given
        let body_lines = vec!["   This is just a paragraph, not a bullet list."];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(matches!(directive, Directive::Unknown { name, .. } if name == "list-table"));
    }

    #[test]
    fn test_parse_list_table_nested_domain_object_in_cell_is_fully_parsed() {
        // Given — regression case for the known_bugs.md list-table gap: a
        // cell nesting a domain-object definition must be reparsed as a
        // real node, not swallowed as opaque text.
        let body_lines = vec![
            "   * - .. py:attribute:: method.__self__",
            "     - Description",
        ];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty());
        if let Directive::DataTable { rows, .. } = directive {
            assert_eq!(rows.len(), 1);
            assert!(matches!(
                rows[0].cells[0].content.as_slice(),
                [Node::Directive(Directive::DomainObject(_))]
            ));
        } else {
            panic!("Expected DataTable directive");
        }
    }

    #[test]
    fn test_unknown_list_table_carries_argument_and_body() {
        // Given
        let body_lines = vec!["   not a list"];

        // When
        let directive = unknown_list_table("My Title".to_string(), &body_lines);

        // Then
        assert_eq!(
            directive,
            Directive::Unknown {
                name: "list-table".to_string(),
                argument: "My Title".to_string(),
                body: "not a list".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_list_table_via_full_parse_pipeline() {
        // Given
        let input = "\
.. list-table:: Fruit
   :header-rows: 1

   * - Fruit
     - Colour
   * - Apple
     - Red
";

        // When
        let doc = crate::parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::DataTable { source, title, header_rows, rows, .. })
                if *source == TableSource::List
                    && title.as_deref() == Some("Fruit")
                    && *header_rows == 1
                    && rows.len() == 2
        ));
    }

    #[test]
    fn test_parse_list_table_cell_content_supports_inline_markup() {
        // Given
        let body_lines = vec!["   * - Plain text cell"];

        // When
        let (directive, _) = parse(&body_lines);

        // Then
        if let Directive::DataTable { rows, .. } = directive {
            assert_eq!(
                rows[0].cells[0].content,
                vec![Node::Paragraph(vec![InlineNode::Text(
                    "Plain text cell".to_string()
                )])]
            );
        } else {
            panic!("Expected DataTable directive");
        }
    }
}
