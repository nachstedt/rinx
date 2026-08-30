//! `.. csv-table::` — a table whose rows are CSV data.

use rusty_sphinx_ast::{Directive, Node, TableCell, TableRow, TableSource};

use super::csv_dialect::{CsvDialect, parse_csv_rows};
use super::options::{
    OptionLine, SharedTableOptions, parse_shared_table_options, report_unknown_options,
    scan_option_lines,
};
use super::widths::parse_widths_option;
use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::directives::body::join_body_lines;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;

/// The directive name, used throughout this module's diagnostics.
const DIRECTIVE: &str = "csv-table";

/// The `.. csv-table::` options that say where the data comes from and how it
/// is decoded, as opposed to how it is tokenized (see [`CsvDialect`]) or how
/// the resulting table is presented (see [`SharedTableOptions`]).
#[derive(Default)]
struct CsvSource {
    file: Option<String>,
    url: Option<String>,
    encoding: Option<String>,
    /// `:header:` — extra header rows, given as CSV data in the option value
    /// itself rather than in the directive body.
    header: Option<String>,
}

impl CsvSource {
    /// Applies one option line, reporting `true` if it recognized the name.
    fn apply_option(&mut self, line: &OptionLine) -> bool {
        match line.name.as_str() {
            "file" => self.file = Some(line.value.clone()),
            "url" => self.url = Some(line.value.clone()),
            "encoding" => self.encoding = Some(line.value.clone()),
            "header" => self.header = Some(line.value.clone()),
            _ => return false,
        }
        true
    }
}

/// Parses a `.. csv-table::` directive: a table whose rows are CSV data,
/// either inline in the directive body or in the file named by `:file:`.
/// `body_lines` is the raw, still-indented body collected by
/// `collect_directive_body`, exactly as every other content-bearing directive
/// parser receives it.
pub(in crate::directives) fn parse_csv_table(
    argument: String,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let title = if argument.is_empty() {
        None
    } else {
        Some(argument.clone())
    };

    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, body_start) = scan_option_lines(&unindented_lines);
    let (options, unrecognized) =
        parse_shared_table_options(&option_lines, TableSource::Csv, diagnostics);

    let mut dialect = CsvDialect::default();
    let mut source = CsvSource::default();
    let unclaimed: Vec<&OptionLine> = unrecognized
        .into_iter()
        .filter(|line| !dialect.apply_option(line, diagnostics) && !source.apply_option(line))
        .collect();
    report_unknown_options(&unclaimed, TableSource::Csv, diagnostics);

    let inline_data = join_body_lines(
        &unindented_lines[body_start..]
            .iter()
            .map(String::as_str)
            .collect::<Vec<&str>>(),
    );

    let Some(data) = resolve_csv_data(&inline_data, &source, ctx, diagnostics) else {
        return unknown_csv_table(argument, body_lines);
    };

    let Some(rows) = collect_csv_rows(&data, source.header.as_deref(), &dialect, diagnostics)
    else {
        return unknown_csv_table(argument, body_lines);
    };
    let header_rows = source.header.as_deref().map_or(0, |header| {
        parse_csv_rows(header, &dialect).map_or(0, |rows| rows.len())
    });

    build_csv_table(
        title,
        options,
        header_rows,
        rows,
        adornment_order,
        diagnostics,
        ctx,
    )
    .unwrap_or_else(|| unknown_csv_table(argument, body_lines))
}

/// Obtains the CSV text, from the directive body or from `:file:`.
///
/// Reproduces docutils' two mutual-exclusion errors — data given both ways,
/// or neither way — and adds one of its own for `:url:`, which rusty-sphinx
/// does not implement: fetching over the network inside a cached, sandboxed
/// build action would make the build non-hermetic, so it is refused rather
/// than silently ignored.
fn resolve_csv_data(
    inline_data: &str,
    source: &CsvSource,
    ctx: &ParseCtx<'_>,
    diagnostics: &mut Vec<String>,
) -> Option<String> {
    if source.url.is_some() {
        diagnostics.push(format!(
            "{DIRECTIVE}: the :url: option is not supported — fetching over the network \
             would make the build non-hermetic; download the data and use :file: instead"
        ));
        return None;
    }

    let has_inline = !inline_data.trim().is_empty();
    match (&source.file, has_inline) {
        (Some(_), true) => {
            diagnostics.push(format!(
                "{DIRECTIVE}: cannot have both a :file: option and directive content"
            ));
            None
        }
        (None, false) => {
            diagnostics.push(format!("{DIRECTIVE}: no table data"));
            None
        }
        (None, true) => Some(inline_data.to_string()),
        (Some(path), false) => {
            check_encoding(source.encoding.as_deref(), diagnostics)?;
            match ctx.csv_files.load(path) {
                Ok(data) => Some(data),
                Err(message) => {
                    diagnostics.push(format!("{DIRECTIVE}: {message}"));
                    None
                }
            }
        }
    }
}

/// Accepts only the encodings that are UTF-8 (or a subset of it).
///
/// **Deliberate deviation from docutils:** it decodes any encoding Python
/// knows. Supporting that here would mean a transcoding dependency for a case
/// no document in the benchmark corpus exercises, so anything else is refused
/// with a diagnostic naming the limitation rather than mis-decoding the file
/// into replacement characters.
fn check_encoding(encoding: Option<&str>, diagnostics: &mut Vec<String>) -> Option<()> {
    let Some(encoding) = encoding else {
        return Some(());
    };
    let normalized = encoding.to_ascii_lowercase().replace('_', "-");
    if matches!(normalized.as_str(), "utf-8" | "utf8" | "ascii" | "us-ascii") {
        Some(())
    } else {
        diagnostics.push(format!(
            "{DIRECTIVE}: :encoding: '{encoding}' is not supported — only UTF-8 \
             (and its ASCII subset) can be decoded"
        ));
        None
    }
}

/// Tokenizes the `:header:` option's data and the table's own data into one
/// list of rows, header rows first, padded to the widest row.
///
/// Padding rather than rejecting matches docutils, which extends every short
/// row to `max_cols` with empty cells.
fn collect_csv_rows(
    data: &str,
    header: Option<&str>,
    dialect: &CsvDialect,
    diagnostics: &mut Vec<String>,
) -> Option<Vec<Vec<String>>> {
    let mut rows = Vec::new();
    if let Some(header) = header {
        match parse_csv_rows(header, dialect) {
            Ok(header_rows) => rows.extend(header_rows),
            Err(message) => {
                diagnostics.push(format!("{DIRECTIVE}: malformed :header: data: {message}"));
                return None;
            }
        }
    }
    match parse_csv_rows(data, dialect) {
        Ok(data_rows) => rows.extend(data_rows),
        Err(message) => {
            diagnostics.push(format!("{DIRECTIVE}: malformed CSV data: {message}"));
            return None;
        }
    }

    let max_cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    for row in &mut rows {
        row.resize(max_cols, String::new());
    }
    Some(rows)
}

/// Turns the field strings into an AST node, re-parsing every field as
/// block-level RST so a cell is "a miniature document" exactly as a
/// list-table's or grid table's cell is.
///
/// Returns `None` when `:header-rows:` or `:stub-columns:` exceeds the
/// table's actual dimensions — docutils' `check_table_dimensions` errors
/// there, and unlike a list-table (whose row count is nothing but the bullet
/// list's length) a csv-table's author has stated a count that the data
/// contradicts, which is worth reporting rather than quietly clamping.
fn build_csv_table(
    title: Option<String>,
    options: SharedTableOptions,
    header_option_rows: usize,
    field_rows: Vec<Vec<String>>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    ctx: &ParseCtx<'_>,
) -> Option<Directive> {
    let ncols = field_rows.first().map_or(0, Vec::len);
    let header_rows = header_option_rows + options.header_rows;

    if header_rows > field_rows.len() {
        diagnostics.push(format!(
            "{DIRECTIVE}: {header_rows} header row(s) requested, but the table has only {} row(s)",
            field_rows.len()
        ));
        return None;
    }
    if options.stub_columns > ncols {
        diagnostics.push(format!(
            "{DIRECTIVE}: :stub-columns: {} exceeds the table's {ncols} column(s)",
            options.stub_columns
        ));
        return None;
    }

    let widths = options
        .widths_raw
        .and_then(|raw| parse_widths_option(&raw, ncols, DIRECTIVE, diagnostics));

    let rows = field_rows
        .into_iter()
        .map(|fields| TableRow {
            cells: fields
                .into_iter()
                .map(|field| TableCell {
                    colspan: 1,
                    rowspan: 1,
                    content: parse_cell_content(&field, adornment_order, diagnostics, ctx),
                })
                .collect(),
        })
        .collect();

    Some(Directive::DataTable {
        source: TableSource::Csv,
        title,
        header_rows,
        stub_columns: options.stub_columns,
        widths,
        width: options.width,
        align: options.align,
        classes: options.classes,
        name: options.name,
        rows,
    })
}

/// Re-parses one field's text as block-level RST. A field may span several
/// lines (a quoted field can contain newlines), so it is split back into
/// lines for the block parser.
fn parse_cell_content(
    field: &str,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    ctx: &ParseCtx<'_>,
) -> Vec<Node> {
    let lines: Vec<&str> = field.lines().collect();
    parse_blocks(&lines, adornment_order, diagnostics, ctx)
}

fn unknown_csv_table(argument: String, body_lines: &[&str]) -> Directive {
    Directive::Unknown {
        name: DIRECTIVE.to_string(),
        argument,
        body: join_body_lines(body_lines),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CsvFileLoader;
    use rusty_sphinx_ast::{Domain, InlineNode, TableAlign, TableWidths, TargetName};
    use std::collections::HashMap;

    /// An in-memory stand-in for the worker's filesystem loader, so `:file:`
    /// can be exercised without touching the disk.
    struct FakeCsvFiles(HashMap<String, String>);

    impl FakeCsvFiles {
        fn with(path: &str, data: &str) -> Self {
            let mut files = HashMap::new();
            files.insert(path.to_string(), data.to_string());
            Self(files)
        }
    }

    impl CsvFileLoader for FakeCsvFiles {
        fn load(&self, path: &str) -> Result<String, String> {
            self.0
                .get(path)
                .cloned()
                .ok_or_else(|| format!("cannot read '{path}': no such file"))
        }
    }

    fn parse_with(body_lines: &[&str], ctx: &ParseCtx<'_>) -> (Directive, Vec<String>) {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();
        let directive = parse_csv_table(
            String::new(),
            body_lines,
            &mut adornment_order,
            &mut diagnostics,
            ctx,
        );
        (directive, diagnostics)
    }

    fn parse(body_lines: &[&str]) -> (Directive, Vec<String>) {
        parse_with(body_lines, &ParseCtx::with_domain(Domain::Py))
    }

    /// The plain text of every cell, row by row — the shape most tests here
    /// actually care about.
    fn cell_texts(directive: &Directive) -> Vec<Vec<String>> {
        let Directive::DataTable { rows, .. } = directive else {
            panic!("Expected DataTable directive, got {directive:?}");
        };
        rows.iter()
            .map(|row| {
                row.cells
                    .iter()
                    .map(|cell| match cell.content.as_slice() {
                        [Node::Paragraph(inlines)] => inlines
                            .iter()
                            .map(|inline| match inline {
                                InlineNode::Text(text) => text.clone(),
                                other => format!("{other:?}"),
                            })
                            .collect(),
                        [] => String::new(),
                        other => format!("{other:?}"),
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn test_parse_csv_table_basic_inline_data() {
        // Given
        let body_lines = vec!["   Fruit, Colour", "   Apple, Red"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            cell_texts(&directive),
            vec![vec!["Fruit", "Colour"], vec!["Apple", "Red"]]
        );
        let Directive::DataTable {
            source,
            header_rows,
            ..
        } = directive
        else {
            panic!("Expected DataTable directive");
        };
        assert_eq!(source, TableSource::Csv);
        assert_eq!(header_rows, 0);
    }

    #[test]
    fn test_parse_csv_table_with_title() {
        // Given
        let body_lines = vec!["   Apple, Red"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let directive = parse_csv_table(
            "Popular Fruits".to_string(),
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        let Directive::DataTable { title, .. } = directive else {
            panic!("Expected DataTable directive");
        };
        assert_eq!(title, Some("Popular Fruits".to_string()));
    }

    #[test]
    fn test_parse_csv_table_reads_every_shared_option() {
        // Given
        let body_lines = vec![
            "   :header-rows: 1",
            "   :stub-columns: 1",
            "   :widths: 30 70",
            "   :width: 50%",
            "   :align: center",
            "   :class: compact",
            "   :name: fruit-csv",
            "",
            "   Fruit, Colour",
            "   Apple, Red",
        ];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let Directive::DataTable {
            header_rows,
            stub_columns,
            widths,
            width,
            align,
            classes,
            name,
            ..
        } = directive
        else {
            panic!("Expected DataTable directive");
        };
        assert_eq!(header_rows, 1);
        assert_eq!(stub_columns, 1);
        assert_eq!(widths, Some(TableWidths::Explicit(vec![30, 70])));
        assert_eq!(width, Some("50%".to_string()));
        assert_eq!(align, Some(TableAlign::Center));
        assert_eq!(classes, vec!["compact".to_string()]);
        assert_eq!(name, Some(TargetName::new("fruit-csv")));
    }

    #[test]
    fn test_parse_csv_table_header_option_prepends_rows_and_counts_as_header() {
        // Given
        let body_lines = vec!["   :header: Fruit, Colour", "", "   Apple, Red"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            cell_texts(&directive),
            vec![vec!["Fruit", "Colour"], vec!["Apple", "Red"]]
        );
        let Directive::DataTable { header_rows, .. } = directive else {
            panic!("Expected DataTable directive");
        };
        assert_eq!(header_rows, 1);
    }

    #[test]
    fn test_parse_csv_table_header_option_adds_to_header_rows_option() {
        // Given
        let body_lines = vec![
            "   :header: Fruit, Colour",
            "   :header-rows: 1",
            "",
            "   Citrus, Group",
            "   Lemon, Yellow",
        ];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let Directive::DataTable { header_rows, .. } = directive else {
            panic!("Expected DataTable directive");
        };
        assert_eq!(header_rows, 2);
    }

    #[test]
    fn test_parse_csv_table_pads_short_rows_to_the_widest_row() {
        // Given
        let body_lines = vec!["   a, b, c", "   d, e"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            cell_texts(&directive),
            vec![vec!["a", "b", "c"], vec!["d", "e", ""]]
        );
    }

    #[test]
    fn test_parse_csv_table_keeps_a_comma_inside_a_quoted_field() {
        // Given
        let body_lines = vec!["   \"Apple, Braeburn\", Red"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(cell_texts(&directive), vec![vec!["Apple, Braeburn", "Red"]]);
    }

    #[test]
    fn test_parse_csv_table_honours_the_delim_option() {
        // Given
        let body_lines = vec!["   :delim: ;", "", "   Apple; Red"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(cell_texts(&directive), vec![vec!["Apple", "Red"]]);
    }

    #[test]
    fn test_parse_csv_table_honours_the_quote_option() {
        // Given
        let body_lines = vec!["   :quote: '", "", "   'Apple, Braeburn', Red"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(cell_texts(&directive), vec![vec!["Apple, Braeburn", "Red"]]);
    }

    #[test]
    fn test_parse_csv_table_accepts_the_keepspace_flag() {
        // Given — `:keepspace:` takes no value; what it changes is the field
        // text handed to the block parser, which is asserted directly in
        // `csv_dialect`'s tests. Here it only has to be recognized rather
        // than reported as an unknown option (the RST re-parse of a cell
        // strips leading indentation again, so the difference is not visible
        // in the resulting nodes).
        let body_lines = vec!["   :keepspace:", "", "   Apple,   Red"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(cell_texts(&directive), vec![vec!["Apple", "Red"]]);
    }

    #[test]
    fn test_parse_csv_table_cell_content_is_parsed_as_rst() {
        // Given
        let body_lines = vec!["   *emphasis*, plain"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let Directive::DataTable { rows, .. } = directive else {
            panic!("Expected DataTable directive");
        };
        assert!(
            matches!(
                rows[0].cells[0].content.as_slice(),
                [Node::Paragraph(inlines)] if matches!(inlines.as_slice(), [InlineNode::Emphasis(_)])
            ),
            "expected emphasis, got {:?}",
            rows[0].cells[0].content
        );
    }

    #[test]
    fn test_parse_csv_table_empty_field_becomes_an_empty_cell() {
        // Given
        let body_lines = vec!["   Apple,"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let Directive::DataTable { rows, .. } = directive else {
            panic!("Expected DataTable directive");
        };
        assert!(rows[0].cells[1].content.is_empty());
    }

    #[test]
    fn test_parse_csv_table_reads_the_file_option() {
        // Given
        let loader = FakeCsvFiles::with("data/fruits.csv", "Apple, Red\nBanana, Yellow\n");
        let ctx = ParseCtx {
            default_domain: Domain::Py,
            csv_files: &loader,
        };
        let body_lines = vec!["   :file: data/fruits.csv"];

        // When
        let (directive, diagnostics) = parse_with(&body_lines, &ctx);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            cell_texts(&directive),
            vec![vec!["Apple", "Red"], vec!["Banana", "Yellow"]]
        );
    }

    #[test]
    fn test_parse_csv_table_reports_an_unreadable_file() {
        // Given
        let loader = FakeCsvFiles::with("data/fruits.csv", "Apple, Red\n");
        let ctx = ParseCtx {
            default_domain: Domain::Py,
            csv_files: &loader,
        };
        let body_lines = vec!["   :file: data/missing.csv"];

        // When
        let (directive, diagnostics) = parse_with(&body_lines, &ctx);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].contains("data/missing.csv"),
            "{}",
            diagnostics[0]
        );
        assert!(matches!(directive, Directive::Unknown { name, .. } if name == "csv-table"));
    }

    #[test]
    fn test_parse_csv_table_rejects_a_file_option_without_a_loader() {
        // Given — the default context has no filesystem access.
        let body_lines = vec!["   :file: data/fruits.csv"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains(":file:"), "{}", diagnostics[0]);
        assert!(matches!(directive, Directive::Unknown { .. }));
    }

    #[test]
    fn test_parse_csv_table_rejects_the_url_option() {
        // Given
        let body_lines = vec!["   :url: https://example.com/fruits.csv"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains(":url:"), "{}", diagnostics[0]);
        assert!(diagnostics[0].contains("hermetic"), "{}", diagnostics[0]);
        assert!(matches!(directive, Directive::Unknown { .. }));
    }

    #[test]
    fn test_parse_csv_table_rejects_both_file_and_inline_content() {
        // Given
        let loader = FakeCsvFiles::with("data/fruits.csv", "Apple, Red\n");
        let ctx = ParseCtx {
            default_domain: Domain::Py,
            csv_files: &loader,
        };
        let body_lines = vec!["   :file: data/fruits.csv", "", "   Banana, Yellow"];

        // When
        let (directive, diagnostics) = parse_with(&body_lines, &ctx);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("both"), "{}", diagnostics[0]);
        assert!(matches!(directive, Directive::Unknown { .. }));
    }

    #[test]
    fn test_parse_csv_table_rejects_a_directive_with_no_data_at_all() {
        // Given
        let body_lines: Vec<&str> = Vec::new();

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].contains("no table data"),
            "{}",
            diagnostics[0]
        );
        assert!(matches!(directive, Directive::Unknown { .. }));
    }

    #[test]
    fn test_parse_csv_table_accepts_a_utf8_encoding() {
        // Given
        let loader = FakeCsvFiles::with("data/fruits.csv", "Äpfel, Rot\n");
        let ctx = ParseCtx {
            default_domain: Domain::Py,
            csv_files: &loader,
        };
        let body_lines = vec!["   :file: data/fruits.csv", "   :encoding: UTF-8"];

        // When
        let (directive, diagnostics) = parse_with(&body_lines, &ctx);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(cell_texts(&directive), vec![vec!["Äpfel", "Rot"]]);
    }

    #[test]
    fn test_parse_csv_table_rejects_a_non_utf8_encoding() {
        // Given
        let loader = FakeCsvFiles::with("data/fruits.csv", "Apple, Red\n");
        let ctx = ParseCtx {
            default_domain: Domain::Py,
            csv_files: &loader,
        };
        let body_lines = vec!["   :file: data/fruits.csv", "   :encoding: latin-1"];

        // When
        let (directive, diagnostics) = parse_with(&body_lines, &ctx);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("latin-1"), "{}", diagnostics[0]);
        assert!(matches!(directive, Directive::Unknown { .. }));
    }

    #[test]
    fn test_parse_csv_table_rejects_more_header_rows_than_rows() {
        // Given
        let body_lines = vec!["   :header-rows: 3", "", "   Apple, Red"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("header row"), "{}", diagnostics[0]);
        assert!(matches!(directive, Directive::Unknown { .. }));
    }

    #[test]
    fn test_parse_csv_table_rejects_more_stub_columns_than_columns() {
        // Given
        let body_lines = vec!["   :stub-columns: 3", "", "   Apple, Red"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].contains(":stub-columns:"),
            "{}",
            diagnostics[0]
        );
        assert!(matches!(directive, Directive::Unknown { .. }));
    }

    #[test]
    fn test_parse_csv_table_reports_malformed_data() {
        // Given — a quoted field that is never closed.
        let body_lines = vec!["   \"unterminated, Red", "   Apple, Red"];

        // When
        let (directive, diagnostics) = parse(&body_lines);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].contains("malformed CSV"),
            "{}",
            diagnostics[0]
        );
        assert!(matches!(directive, Directive::Unknown { .. }));
    }

    #[test]
    fn test_parse_csv_table_reports_malformed_header_data() {
        // Given
        let body_lines = vec!["   :header: \"unterminated", "", "   Apple, Red"];

        // When
        let (_, diagnostics) = parse(&body_lines);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains(":header:"), "{}", diagnostics[0]);
    }

    #[test]
    fn test_parse_csv_table_emits_diagnostic_for_unknown_option() {
        // Given
        let body_lines = vec!["   :bogus: x", "", "   Apple, Red"];

        // When
        let (_, diagnostics) = parse(&body_lines);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].contains("Invalid or non-standard Sphinx csv-table option"),
            "{}",
            diagnostics[0]
        );
    }

    #[test]
    fn test_parse_csv_table_via_full_parse_pipeline() {
        // Given
        let input = "\
.. csv-table:: Fruit
   :header-rows: 1

   Fruit, Colour
   Apple, Red
";

        // When
        let doc = crate::parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::DataTable { source, title, header_rows, rows, .. })
                if *source == TableSource::Csv
                    && title.as_deref() == Some("Fruit")
                    && *header_rows == 1
                    && rows.len() == 2
        ));
    }

    #[test]
    fn test_check_encoding_accepts_the_utf8_family() {
        // Given
        let mut diagnostics = Vec::new();

        // When / Then
        for encoding in ["utf-8", "UTF8", "us_ascii", "ascii"] {
            assert_eq!(
                check_encoding(Some(encoding), &mut diagnostics),
                Some(()),
                "{encoding} should be accepted"
            );
        }
        assert_eq!(check_encoding(None, &mut diagnostics), Some(()));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_collect_csv_rows_pads_every_row_to_the_widest() {
        // Given
        let mut diagnostics = Vec::new();

        // When
        let rows = collect_csv_rows("a,b,c\nd\n", None, &CsvDialect::default(), &mut diagnostics)
            .expect("well-formed CSV");

        // Then
        assert_eq!(rows, vec![vec!["a", "b", "c"], vec!["d", "", ""]]);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_collect_csv_rows_puts_header_rows_first() {
        // Given
        let mut diagnostics = Vec::new();

        // When
        let rows = collect_csv_rows(
            "Apple,Red\n",
            Some("Fruit,Colour"),
            &CsvDialect::default(),
            &mut diagnostics,
        )
        .expect("well-formed CSV");

        // Then
        assert_eq!(rows[0], vec!["Fruit", "Colour"]);
        assert_eq!(rows[1], vec!["Apple", "Red"]);
    }

    #[test]
    fn test_csv_source_apply_option_claims_only_its_own_options() {
        // Given
        let mut source = CsvSource::default();

        // When
        let claimed_file = source.apply_option(&OptionLine {
            name: "file".to_string(),
            value: "data/fruits.csv".to_string(),
            raw: ":file: data/fruits.csv".to_string(),
        });
        let claimed_other = source.apply_option(&OptionLine {
            name: "delim".to_string(),
            value: ";".to_string(),
            raw: ":delim: ;".to_string(),
        });

        // Then
        assert!(claimed_file);
        assert!(!claimed_other);
        assert_eq!(source.file.as_deref(), Some("data/fruits.csv"));
    }

    #[test]
    fn test_unknown_csv_table_carries_argument_and_body() {
        // Given
        let body_lines = vec!["   Apple, Red"];

        // When
        let directive = unknown_csv_table("My Title".to_string(), &body_lines);

        // Then
        assert_eq!(
            directive,
            Directive::Unknown {
                name: "csv-table".to_string(),
                argument: "My Title".to_string(),
                body: "Apple, Red".to_string(),
            }
        );
    }
}
