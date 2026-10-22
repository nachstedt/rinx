//! The two options `.. list-table::` and `.. csv-table::` have beyond the
//! five every option-bearing table directive shares (see
//! `crate::directives::table_options`): `:header-rows:` and `:stub-columns:`.
//! [`DataTableOption`] is their group, read beside the presentation options'
//! by a [`SharedTableReader`], since `.. table::` has no use for either — its
//! table already comes pre-split into header/body rows by its own grid or
//! simple table syntax.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::options::{OptionLine, classify_options, directive_options};
use crate::directives::table_options::{CommonTableOptions, TableOption, read_table_option};
use rinx_ast::{Diagnostic, DiagnosticCode, Span, TableAlign, TableSource, TargetName};

/// The seven options `.. list-table::` and `.. csv-table::` share — the five
/// common options plus `header-rows`/`stub-columns`.
pub(super) struct SharedTableOptions {
    pub header_rows: usize,
    pub stub_columns: usize,
    pub widths_raw: Option<String>,
    pub width: Option<String>,
    pub align: Option<TableAlign>,
    pub classes: Vec<String>,
    pub name: Option<TargetName>,
}

directive_options! {
    /// The two options `.. list-table::` and `.. csv-table::` take beyond the
    /// five presentation ones.
    pub(super) enum DataTableOption {
        /// `:header-rows:`.
        HeaderRows => (["header-rows"], Integer, "How many rows are the table's header."),
        /// `:stub-columns:`.
        StubColumns => (["stub-columns"], Integer, "How many columns are row headers."),
    }
}

directive_options! {
    /// The options `.. list-table::` takes: docutils' seven.
    pub(super) enum ListTableOption {
        ;
        Data(DataTableOption),
        Table(TableOption),
    }
}

/// The shared options while they are read: the two data-table ones and the
/// five presentation ones.
pub(super) struct SharedTableReader {
    header_rows: usize,
    stub_columns: usize,
    common: CommonTableOptions,
}

impl SharedTableReader {
    /// Nothing read yet.
    pub(super) fn new() -> Self {
        Self {
            header_rows: 0,
            stub_columns: 0,
            common: CommonTableOptions::empty(),
        }
    }

    /// Records one of the two data-table options.
    pub(super) fn read_data(
        &mut self,
        option: DataTableOption,
        line: &OptionLine,
        directive: &str,
        diagnostics: &mut Diagnostics,
        ctx: &ParseCtx<'_>,
    ) {
        let (field, name) = match option {
            DataTableOption::HeaderRows => (&mut self.header_rows, "header-rows"),
            DataTableOption::StubColumns => (&mut self.stub_columns, "stub-columns"),
        };
        *field = parse_nonneg_int_option(
            &line.value,
            name,
            directive,
            diagnostics,
            ctx.line_span(line.line_index, &line.raw),
        );
    }

    /// Records one of the five presentation options.
    pub(super) fn read_table(
        &mut self,
        option: TableOption,
        line: &OptionLine,
        directive: &str,
        diagnostics: &mut Diagnostics,
        ctx: &ParseCtx<'_>,
    ) {
        read_table_option(option, line, &mut self.common, directive, diagnostics, ctx);
    }

    /// What was read.
    pub(super) fn finish(self) -> SharedTableOptions {
        SharedTableOptions {
            header_rows: self.header_rows,
            stub_columns: self.stub_columns,
            widths_raw: self.common.widths_raw,
            width: self.common.width,
            align: self.common.align,
            classes: self.common.classes,
            name: self.common.name,
        }
    }
}

/// Reads a `.. list-table::`'s options, reporting every one it does not take.
pub(super) fn parse_list_table_options(
    option_lines: &[OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> SharedTableOptions {
    let directive = TableSource::List.as_str();
    let mut reader = SharedTableReader::new();
    for (option, line) in
        classify_options::<ListTableOption>(option_lines, directive, diagnostics, ctx)
    {
        match option {
            ListTableOption::Data(option) => {
                reader.read_data(option, line, directive, diagnostics, ctx);
            }
            ListTableOption::Table(option) => {
                reader.read_table(option, line, directive, diagnostics, ctx);
            }
        }
    }
    reader.finish()
}

/// Parses a nonnegative-integer option value (`:header-rows:`/
/// `:stub-columns:`), pushing a diagnostic and defaulting to 0 on a malformed
/// value rather than failing the whole directive.
pub(super) fn parse_nonneg_int_option(
    raw: &str,
    option_name: &str,
    directive: &str,
    diagnostics: &mut Diagnostics,
    span: Option<Span>,
) -> usize {
    if let Ok(value) = raw.parse::<usize>() {
        value
    } else {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::TableDataIntegerInvalid,
            format!("{directive}: :{option_name}: value '{raw}' is not a nonnegative integer"),
            span,
        ));
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directives::options::DirectiveOptions;
    use crate::directives::options::scan_option_lines;
    use rinx_ast::Domain;

    /// The shared options read off `option_lines` as a list table reads
    /// them, and the lines that are none of them.
    fn parse_shared_table_options<'a>(
        option_lines: &'a [OptionLine],
        directive: &str,
        diagnostics: &mut Diagnostics,
        ctx: &ParseCtx<'_>,
    ) -> (SharedTableOptions, Vec<&'a OptionLine>) {
        let mut reader = SharedTableReader::new();
        let mut unrecognized = Vec::new();
        for line in option_lines {
            match ListTableOption::from_name(&line.name) {
                Some(ListTableOption::Data(option)) => {
                    reader.read_data(option, line, directive, diagnostics, ctx);
                }
                Some(ListTableOption::Table(option)) => {
                    reader.read_table(option, line, directive, diagnostics, ctx);
                }
                None => unrecognized.push(line),
            }
        }
        (reader.finish(), unrecognized)
    }

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn test_parse_shared_table_options_reads_every_shared_option() {
        // Given
        let body = lines(&[
            ":header-rows: 1",
            ":stub-columns: 2",
            ":widths: 30 70",
            ":width: 50%",
            ":align: right",
            ":class: foo bar",
            ":name: My Table",
        ]);
        let (option_lines, _) = scan_option_lines(&body);
        let mut diagnostics = Diagnostics::default();

        // When
        let (options, unrecognized) = parse_shared_table_options(
            &option_lines,
            "list-table",
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert!(diagnostics.is_empty());
        assert!(unrecognized.is_empty());
        assert_eq!(options.header_rows, 1);
        assert_eq!(options.stub_columns, 2);
        assert_eq!(options.widths_raw.as_deref(), Some("30 70"));
        assert_eq!(options.width.as_deref(), Some("50%"));
        assert_eq!(options.align, Some(TableAlign::Right));
        assert_eq!(options.classes, vec!["foo".to_string(), "bar".to_string()]);
        assert_eq!(options.name, Some(TargetName::new("My Table")));
    }

    #[test]
    fn test_parse_shared_table_options_hands_back_unrecognized_lines() {
        // Given
        let body = lines(&[":header-rows: 1", ":delim: tab", ":bogus: x"]);
        let (option_lines, _) = scan_option_lines(&body);
        let mut diagnostics = Diagnostics::default();

        // When
        let (_, unrecognized) = parse_shared_table_options(
            &option_lines,
            "csv-table",
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert!(diagnostics.is_empty());
        let names: Vec<&str> = unrecognized.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, vec!["delim", "bogus"]);
    }

    #[test]
    fn test_parse_shared_table_options_rejects_an_invalid_align_value() {
        // Given
        let body = lines(&[":align: diagonal"]);
        let (option_lines, _) = scan_option_lines(&body);
        let mut diagnostics = Diagnostics::default();

        // When
        let (options, _) = parse_shared_table_options(
            &option_lines,
            "csv-table",
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(options.align, None);
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .starts_with("csv-table: invalid :align:"),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn test_parse_shared_table_options_names_the_directive_in_its_diagnostics() {
        // Given
        let body = lines(&[":header-rows: many"]);
        let (option_lines, _) = scan_option_lines(&body);
        let mut diagnostics = Diagnostics::default();

        // When
        parse_shared_table_options(
            &option_lines,
            "csv-table",
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].message.starts_with("csv-table:"),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn test_parse_nonneg_int_option_accepts_a_number() {
        // Given
        let mut diagnostics = Diagnostics::default();

        // When
        let value =
            parse_nonneg_int_option("3", "header-rows", "list-table", &mut diagnostics, None);

        // Then
        assert_eq!(value, 3);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_nonneg_int_option_rejects_non_numeric_value() {
        // Given
        let mut diagnostics = Diagnostics::default();

        // When
        let value =
            parse_nonneg_int_option("abc", "header-rows", "list-table", &mut diagnostics, None);

        // Then
        assert_eq!(value, 0);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("header-rows"));
    }
}
