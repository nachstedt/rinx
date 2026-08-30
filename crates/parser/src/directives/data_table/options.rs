//! The two options `.. list-table::` and `.. csv-table::` have beyond the
//! five every option-bearing table directive shares (see
//! `crate::directives::table_options`): `:header-rows:` and `:stub-columns:`.
//! [`parse_shared_table_options`] layers those on top of
//! [`crate::directives::table_options::parse_common_table_options`], since
//! `.. table::` has no use for either — its table already comes pre-split
//! into header/body rows by its own grid or simple table syntax.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::options::OptionLine;
use crate::directives::table_options::parse_common_table_options;
use rusty_sphinx_ast::{Diagnostic, DiagnosticCode, Span, TableAlign, TargetName};

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

/// Consumes the shared options out of `option_lines`, returning them together
/// with the lines it did not recognize, in source order.
///
/// `directive` only shapes the diagnostics' wording, so a `csv-table` author is
/// never told about a `list-table` option.
pub(super) fn parse_shared_table_options<'a>(
    option_lines: &'a [OptionLine],
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> (SharedTableOptions, Vec<&'a OptionLine>) {
    let mut header_rows = 0;
    let mut stub_columns = 0;
    let mut rest = Vec::new();

    for line in option_lines {
        match line.name.as_str() {
            "header-rows" => {
                header_rows = parse_nonneg_int_option(
                    &line.value,
                    "header-rows",
                    directive,
                    diagnostics,
                    ctx.line_span(line.line_index, &line.raw),
                );
            }
            "stub-columns" => {
                stub_columns = parse_nonneg_int_option(
                    &line.value,
                    "stub-columns",
                    directive,
                    diagnostics,
                    ctx.line_span(line.line_index, &line.raw),
                );
            }
            _ => rest.push(line),
        }
    }

    let (common, unrecognized) = parse_common_table_options(&rest, directive, diagnostics, ctx);
    let options = SharedTableOptions {
        header_rows,
        stub_columns,
        widths_raw: common.widths_raw,
        width: common.width,
        align: common.align,
        classes: common.classes,
        name: common.name,
    };

    (options, unrecognized)
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
    use crate::directives::options::scan_option_lines;
    use rusty_sphinx_ast::Domain;

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
