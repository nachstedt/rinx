use crate::blocks::parse_blocks;
use crate::directives::body::join_body_lines;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;
use rusty_sphinx_ast::{
    Directive, Domain, ListItem, ListTableWidths, Node, TableAlign, TableCell, TableRow, TargetName,
};

/// The recognized `.. list-table::` options, scanned off the leading
/// `:option:` lines of its body by [`parse_list_table_options`]. `widths` is
/// kept as its raw string (not yet resolved to a [`ListTableWidths`])
/// because validating it needs the table's column count, which isn't known
/// until the rows have been lowered.
struct ListTableOptions {
    header_rows: usize,
    stub_columns: usize,
    widths_raw: Option<String>,
    width: Option<String>,
    align: Option<TableAlign>,
    classes: Vec<String>,
    name: Option<TargetName>,
}

/// Parses a `.. list-table::` directive: a table specified as a nested
/// bullet list (outer list = rows, each row's own bullet list = cells)
/// rather than character-art. `body_lines` is the raw, still-indented body
/// collected by `collect_directive_body`, exactly as every other
/// content-bearing directive parser receives it.
pub(super) fn parse_list_table(
    argument: String,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> Directive {
    let title = if argument.is_empty() {
        None
    } else {
        Some(argument.clone())
    };

    let unindented_lines = unindent_body_lines(body_lines);
    let (options, opt_idx) = parse_list_table_options(&unindented_lines, diagnostics);

    let body_content: Vec<&str> = unindented_lines[opt_idx..]
        .iter()
        .map(String::as_str)
        .collect();
    let body_nodes = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

    let [
        Node::BulletList {
            items: row_items, ..
        },
    ] = body_nodes.as_slice()
    else {
        diagnostics
            .push("list-table: directive body must be a single bullet list of rows".to_string());
        return unknown_list_table(argument, body_lines);
    };

    let (rows, ncols) = lower_list_table_rows(row_items, diagnostics);
    let widths = options
        .widths_raw
        .and_then(|raw| parse_widths_option(&raw, ncols, diagnostics));

    Directive::ListTable {
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

/// Scans the leading `:option:` lines off an already-unindented directive
/// body, returning the recognized options plus the index of the first line
/// that isn't an option (where the real body content starts).
fn parse_list_table_options(
    unindented_lines: &[String],
    diagnostics: &mut Vec<String>,
) -> (ListTableOptions, usize) {
    let mut options = ListTableOptions {
        header_rows: 0,
        stub_columns: 0,
        widths_raw: None,
        width: None,
        align: None,
        classes: Vec::new(),
        name: None,
    };

    let mut opt_idx = 0;
    while opt_idx < unindented_lines.len() {
        let line = unindented_lines[opt_idx].trim();
        if line.is_empty() {
            opt_idx += 1;
            continue;
        }
        if !line.starts_with(':') {
            break;
        }
        if let Some(rest) = line.strip_prefix(":header-rows:") {
            options.header_rows = parse_nonneg_int_option(rest.trim(), "header-rows", diagnostics);
        } else if let Some(rest) = line.strip_prefix(":stub-columns:") {
            options.stub_columns =
                parse_nonneg_int_option(rest.trim(), "stub-columns", diagnostics);
        } else if let Some(rest) = line.strip_prefix(":widths:") {
            options.widths_raw = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix(":width:") {
            options.width = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix(":align:") {
            match rest.trim().parse::<TableAlign>() {
                Ok(parsed) => options.align = Some(parsed),
                Err(()) => diagnostics.push(format!(
                    "list-table: invalid :align: value '{}', expected 'left', 'center', or 'right'",
                    rest.trim()
                )),
            }
        } else if let Some(rest) = line.strip_prefix(":class:") {
            options.classes = rest.split_whitespace().map(str::to_string).collect();
        } else if let Some(rest) = line.strip_prefix(":name:") {
            options.name = Some(TargetName::new(rest.trim()));
        } else {
            diagnostics.push(format!(
                "Invalid or non-standard Sphinx list-table option encountered: {line}"
            ));
        }
        opt_idx += 1;
    }

    (options, opt_idx)
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
    diagnostics: &mut Vec<String>,
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
            diagnostics
                .push("list-table: each row must itself be a bullet list of cells".to_string());
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
            Some(n) if n != cells.len() => diagnostics.push(format!(
                "list-table: row has {} cell(s), expected {n} (from the table's first row) — cell counts should match across rows",
                cells.len()
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

/// Parses a nonnegative-integer option value (`:header-rows:`/
/// `:stub-columns:`), pushing a diagnostic and defaulting to 0 on a
/// malformed value rather than failing the whole directive.
fn parse_nonneg_int_option(raw: &str, option_name: &str, diagnostics: &mut Vec<String>) -> usize {
    if let Ok(value) = raw.parse::<usize>() {
        value
    } else {
        diagnostics.push(format!(
            "list-table: :{option_name}: value '{raw}' is not a nonnegative integer"
        ));
        0
    }
}

/// Resolves a `:widths:` option's raw string into a [`ListTableWidths`],
/// validating an explicit integer list against the table's actual column
/// count. Not a `FromStr` impl since that validation needs `ncols`, which
/// isn't known until every row has been lowered.
fn parse_widths_option(
    raw: &str,
    ncols: usize,
    diagnostics: &mut Vec<String>,
) -> Option<ListTableWidths> {
    match raw {
        "auto" => Some(ListTableWidths::Auto),
        "grid" => Some(ListTableWidths::Grid),
        _ => {
            let values: Result<Vec<u32>, _> = raw
                .split([',', ' '])
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::parse)
                .collect();
            match values {
                Ok(values) if values.len() == ncols => Some(ListTableWidths::Explicit(values)),
                Ok(values) => {
                    diagnostics.push(format!(
                        "list-table: :widths: gives {} value(s), but the table has {ncols} column(s)",
                        values.len()
                    ));
                    None
                }
                Err(_) => {
                    diagnostics.push(format!(
                        "list-table: :widths: value '{raw}' is not 'auto', 'grid', or a list of integers"
                    ));
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::InlineNode;

    fn parse(body_lines: &[&str]) -> (Directive, Vec<String>) {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();
        let directive = parse_list_table(
            String::new(),
            body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::Py,
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
        if let Directive::ListTable {
            header_rows, rows, ..
        } = directive
        {
            assert_eq!(header_rows, 0);
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].cells.len(), 2);
            assert_eq!(rows[1].cells.len(), 2);
        } else {
            panic!("Expected ListTable directive, got {directive:?}");
        }
    }

    #[test]
    fn test_parse_list_table_with_title() {
        // Given
        let body_lines = vec!["   * - Cell"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let directive = parse_list_table(
            "My Title".to_string(),
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::Py,
        );

        // Then
        if let Directive::ListTable { title, .. } = directive {
            assert_eq!(title, Some("My Title".to_string()));
        } else {
            panic!("Expected ListTable directive");
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
        if let Directive::ListTable { header_rows, .. } = directive {
            assert_eq!(header_rows, 1);
        } else {
            panic!("Expected ListTable directive");
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
        if let Directive::ListTable { stub_columns, .. } = directive {
            assert_eq!(stub_columns, 1);
        } else {
            panic!("Expected ListTable directive");
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
        if let Directive::ListTable { widths, .. } = directive {
            assert_eq!(widths, Some(ListTableWidths::Auto));
        } else {
            panic!("Expected ListTable directive");
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
        if let Directive::ListTable { widths, .. } = directive {
            assert_eq!(widths, Some(ListTableWidths::Grid));
        } else {
            panic!("Expected ListTable directive");
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
        if let Directive::ListTable { widths, .. } = directive {
            assert_eq!(widths, Some(ListTableWidths::Explicit(vec![30, 70])));
        } else {
            panic!("Expected ListTable directive");
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
        assert!(diagnostics[0].contains(":widths:"));
        if let Directive::ListTable { widths, .. } = directive {
            assert_eq!(widths, None);
        } else {
            panic!("Expected ListTable directive");
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
        if let Directive::ListTable { width, .. } = directive {
            assert_eq!(width, Some("50%".to_string()));
        } else {
            panic!("Expected ListTable directive");
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
        if let Directive::ListTable { align, .. } = directive {
            assert_eq!(align, Some(TableAlign::Right));
        } else {
            panic!("Expected ListTable directive");
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
        assert!(diagnostics[0].contains(":align:"));
        if let Directive::ListTable { align, .. } = directive {
            assert_eq!(align, None);
        } else {
            panic!("Expected ListTable directive");
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
        if let Directive::ListTable { classes, .. } = directive {
            assert_eq!(classes, vec!["foo".to_string(), "bar".to_string()]);
        } else {
            panic!("Expected ListTable directive");
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
        if let Directive::ListTable { name, .. } = directive {
            assert_eq!(name, Some(TargetName::new("My Table")));
        } else {
            panic!("Expected ListTable directive");
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
        assert!(diagnostics[0].contains("Invalid or non-standard Sphinx list-table option"));
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
            diagnostics[0].contains("uneven")
                || diagnostics[0].contains("cell counts")
                || diagnostics[0].contains("expected")
        );
        if let Directive::ListTable { rows, .. } = directive {
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].cells.len(), 2);
            assert_eq!(rows[1].cells.len(), 3);
        } else {
            panic!("Expected ListTable directive");
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
        if let Directive::ListTable {
            header_rows, rows, ..
        } = directive
        {
            assert_eq!(header_rows, rows.len());
        } else {
            panic!("Expected ListTable directive");
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
        if let Directive::ListTable { stub_columns, .. } = directive {
            assert_eq!(stub_columns, 2);
        } else {
            panic!("Expected ListTable directive");
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
        if let Directive::ListTable { rows, .. } = directive {
            assert_eq!(rows.len(), 1);
            assert!(matches!(
                rows[0].cells[0].content.as_slice(),
                [Node::Directive(Directive::DomainObject(_))]
            ));
        } else {
            panic!("Expected ListTable directive");
        }
    }

    #[test]
    fn test_parse_nonneg_int_option_rejects_non_numeric_value() {
        // Given
        let mut diagnostics = Vec::new();

        // When
        let value = parse_nonneg_int_option("abc", "header-rows", &mut diagnostics);

        // Then
        assert_eq!(value, 0);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("header-rows"));
    }

    #[test]
    fn test_parse_widths_option_rejects_unparseable_value() {
        // Given
        let mut diagnostics = Vec::new();

        // When
        let result = parse_widths_option("banana", 2, &mut diagnostics);

        // Then
        assert_eq!(result, None);
        assert_eq!(diagnostics.len(), 1);
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
            Node::Directive(Directive::ListTable { title, header_rows, rows, .. })
                if title.as_deref() == Some("Fruit") && *header_rows == 1 && rows.len() == 2
        ));
    }

    #[test]
    fn test_parse_list_table_cell_content_supports_inline_markup() {
        // Given
        let body_lines = vec!["   * - Plain text cell"];

        // When
        let (directive, _) = parse(&body_lines);

        // Then
        if let Directive::ListTable { rows, .. } = directive {
            assert_eq!(
                rows[0].cells[0].content,
                vec![Node::Paragraph(vec![InlineNode::Text(
                    "Plain text cell".to_string()
                )])]
            );
        } else {
            panic!("Expected ListTable directive");
        }
    }
}
