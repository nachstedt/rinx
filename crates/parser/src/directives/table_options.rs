//! The presentation options shared by every option-bearing table directive —
//! `.. list-table::`, `.. csv-table::` and `.. table::` alike.
//!
//! All three open with a run of `:name: value` lines before their real body.
//! [`super::options::scan_option_lines`] turns that run into name/value pairs
//! without judging them, and [`parse_common_table_options`] consumes the five
//! presentation options all three share (`widths`, `width`, `align`, `class`,
//! `name`) — handing back whatever it didn't recognize so each directive can
//! layer its own options on top (`.. list-table::`/`.. csv-table::` add
//! `header-rows`/`stub-columns` this way, see
//! `data_table::options::parse_shared_table_options`) or diagnose the rest via
//! [`super::options::report_unknown_options`], keyed by directive name rather
//! than `TableSource` so `.. table::` — which produces no
//! `Directive::DataTable` at all — can call it too.
//!
//! This lives as a flat sibling of `data_table` rather than inside it
//! precisely because it is no longer a data-table concept: `.. table::`
//! wraps an existing grid/simple table and needs these same five options
//! without either of `data_table`'s own two extras.

use super::options::OptionLine;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use rusty_sphinx_ast::{Diagnostic, DiagnosticCode, TableAlign, TargetName};

/// The five presentation options every option-bearing table directive shares:
/// `.. list-table::`, `.. csv-table::` and `.. table::` alike.
///
/// `widths` is kept as its raw string (not yet resolved to a
/// [`rusty_sphinx_ast::TableWidths`]) because validating it needs the table's
/// column count, which isn't known until the rows have been built.
pub(in crate::directives) struct CommonTableOptions {
    pub widths_raw: Option<String>,
    pub width: Option<String>,
    pub align: Option<TableAlign>,
    pub classes: Vec<String>,
    pub name: Option<TargetName>,
}

impl CommonTableOptions {
    fn empty() -> Self {
        Self {
            widths_raw: None,
            width: None,
            align: None,
            classes: Vec::new(),
            name: None,
        }
    }
}

/// Consumes the five common options out of `option_lines`, returning them
/// together with the lines it did not recognize, in source order.
///
/// `directive` only shapes the diagnostics' wording, so a `csv-table` author
/// is never told about a `list-table` option, and a `table` author about
/// either.
pub(in crate::directives) fn parse_common_table_options<'a>(
    option_lines: &[&'a OptionLine],
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> (CommonTableOptions, Vec<&'a OptionLine>) {
    let mut options = CommonTableOptions::empty();
    let mut unrecognized = Vec::new();

    for &line in option_lines {
        match line.name.as_str() {
            "widths" => options.widths_raw = Some(line.value.clone()),
            "width" => options.width = Some(line.value.clone()),
            "align" => match line.value.parse::<TableAlign>() {
                Ok(parsed) => options.align = Some(parsed),
                Err(()) => diagnostics.push(Diagnostic::at(
                    DiagnosticCode::TableDataAlignInvalid,
                    format!(
                        "{directive}: invalid :align: value '{}', expected 'left', 'center', or 'right'",
                        line.value
                    ),
                    ctx.line_span(line.line_index, &line.raw),
                )),
            },
            "class" => options.classes = line.value.split_whitespace().map(str::to_string).collect(),
            "name" => options.name = Some(TargetName::new(&line.value)),
            _ => unrecognized.push(line),
        }
    }

    (options, unrecognized)
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
    fn test_parse_common_table_options_reads_every_common_option() {
        // Given — the five options `.. table::` has, called directly rather
        // than through `parse_shared_table_options`'s header-rows/stub-columns
        // layer.
        let body = lines(&[
            ":widths: 30 70",
            ":width: 50%",
            ":align: right",
            ":class: foo bar",
            ":name: My Table",
        ]);
        let (option_lines, _) = scan_option_lines(&body);
        let refs: Vec<&OptionLine> = option_lines.iter().collect();
        let mut diagnostics = Diagnostics::default();

        // When
        let (options, unrecognized) = parse_common_table_options(
            &refs,
            "table",
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert!(diagnostics.is_empty());
        assert!(unrecognized.is_empty());
        assert_eq!(options.widths_raw.as_deref(), Some("30 70"));
        assert_eq!(options.width.as_deref(), Some("50%"));
        assert_eq!(options.align, Some(TableAlign::Right));
        assert_eq!(options.classes, vec!["foo".to_string(), "bar".to_string()]);
        assert_eq!(options.name, Some(TargetName::new("My Table")));
    }

    #[test]
    fn test_parse_common_table_options_hands_back_a_header_rows_line_as_unrecognized() {
        // Given — `.. table::` has no `:header-rows:` option, unlike
        // `list-table`/`csv-table`, so the common parser must not claim it.
        let body = lines(&[":header-rows: 1"]);
        let (option_lines, _) = scan_option_lines(&body);
        let refs: Vec<&OptionLine> = option_lines.iter().collect();
        let mut diagnostics = Diagnostics::default();

        // When
        let (_, unrecognized) = parse_common_table_options(
            &refs,
            "table",
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(unrecognized.len(), 1);
        assert_eq!(unrecognized[0].name, "header-rows");
    }

    #[test]
    fn test_parse_common_table_options_rejects_an_invalid_align_value() {
        // Given
        let body = lines(&[":align: diagonal"]);
        let (option_lines, _) = scan_option_lines(&body);
        let refs: Vec<&OptionLine> = option_lines.iter().collect();
        let mut diagnostics = Diagnostics::default();

        // When
        let (options, _) = parse_common_table_options(
            &refs,
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
}
