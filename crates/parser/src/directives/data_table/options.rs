//! The two options `.. list-table::` and `.. csv-table::` have beyond the
//! five every option-bearing table directive shares (see
//! `crate::directives::table_options`): `:header-rows:` and `:stub-columns:`.
//! [`DataTableOption`] is their group, read beside the presentation options'
//! by a [`SharedTableReader`], since `.. table::` has no use for either — its
//! table already comes pre-split into header/body rows by its own grid or
//! simple table syntax.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::option_value::Located;
use crate::directives::options::{OptionLine, classify_options, directive_options};
use crate::directives::table_options::{CommonTableOptions, TableOption, read_table_option};
use rinx_ast::{TableAlign, TableSource, TableWidths, TargetName, Written};

/// The seven options `.. list-table::` and `.. csv-table::` share — the five
/// common options plus `header-rows`/`stub-columns`.
pub(super) struct SharedTableOptions {
    pub header_rows: usize,
    pub stub_columns: usize,
    pub widths: Option<Located<TableWidths>>,
    pub width: Option<String>,
    pub align: Option<TableAlign>,
    pub classes: Vec<String>,
    pub name: Option<Written<TargetName>>,
}

directive_options! {
    /// The two options `.. list-table::` and `.. csv-table::` take beyond the
    /// five presentation ones.
    pub(super) enum DataTableOption {
        /// `:header-rows:`.
        HeaderRows(usize) => (["header-rows"], TableDataIntegerInvalid, "How many rows are the table's header."),
        /// `:stub-columns:`.
        StubColumns(usize) => (["stub-columns"], TableDataIntegerInvalid, "How many columns are row headers."),
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
    pub(super) fn read_data(&mut self, option: &DataTableOption) {
        match *option {
            DataTableOption::HeaderRows(rows) => self.header_rows = rows,
            DataTableOption::StubColumns(columns) => self.stub_columns = columns,
        }
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
            widths: self.common.widths,
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
                reader.read_data(&option);
            }
            ListTableOption::Table(option) => {
                reader.read_table(option, line, directive, diagnostics, ctx);
            }
        }
    }
    reader.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directives::options::{DirectiveOptions, OptionRead};

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
            match ListTableOption::read(line, directive, diagnostics, ctx) {
                OptionRead::Read(ListTableOption::Data(option)) => {
                    reader.read_data(&option);
                }
                OptionRead::Read(ListTableOption::Table(option)) => {
                    reader.read_table(option, line, directive, diagnostics, ctx);
                }
                OptionRead::Refused => {}
                OptionRead::NotTaken => unrecognized.push(line),
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
        assert_eq!(
            options.widths.map(|widths| widths.value),
            Some(rinx_ast::TableWidths::Explicit(vec![30, 70]))
        );
        assert_eq!(options.width.as_deref(), Some("50%"));
        assert_eq!(options.align, Some(TableAlign::Right));
        assert_eq!(options.classes, vec!["foo".to_string(), "bar".to_string()]);
        assert_eq!(options.name, Some(TargetName::new("My Table").into()));
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
                .starts_with("csv-table: :align: expects one of"),
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
}
