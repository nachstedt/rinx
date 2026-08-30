//! Scanning and interpreting the `:option:` lines shared by every
//! option-bearing table directive — `.. list-table::`, `.. csv-table::` and
//! `.. table::` alike.
//!
//! All three open with a run of `:name: value` lines before their real body.
//! [`scan_option_lines`] turns that run into name/value pairs without judging
//! them, and [`parse_common_table_options`] consumes the five presentation
//! options all three share (`widths`, `width`, `align`, `class`, `name`) —
//! handing back whatever it didn't recognize so each directive can layer its
//! own options on top (`.. list-table::`/`.. csv-table::` add `header-rows`/
//! `stub-columns` this way, see `data_table::options::parse_shared_table_options`)
//! or diagnose the rest via [`report_unknown_options`], keyed by directive
//! name rather than `TableSource` so `.. table::` — which produces no
//! `Directive::DataTable` at all — can call it too.
//!
//! This lives as a flat sibling of `data_table` rather than inside it
//! precisely because it is no longer a data-table concept: `.. table::`
//! wraps an existing grid/simple table and needs these same five options
//! without either of `data_table`'s own two extras.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use rusty_sphinx_ast::{Diagnostic, DiagnosticCode, TableAlign, TargetName};

/// One `:name: value` line, with the source text kept so a diagnostic can
/// quote the line exactly as the author wrote it.
pub(in crate::directives) struct OptionLine {
    pub name: String,
    pub value: String,
    pub raw: String,
    /// This line's index within the directive body, so a diagnostic about the
    /// option can point at the line the author wrote rather than at the
    /// directive as a whole.
    pub line_index: usize,
}

/// Splits the leading `:option:` lines off an already-unindented directive
/// body, returning them plus the index of the first line that isn't an option
/// (where the real body content starts).
///
/// A line whose closing colon is missing (`:oops`) is still returned, with the
/// whole remainder as its `name`, so that it reaches the caller's
/// unknown-option diagnostic rather than being silently swallowed as body.
pub(in crate::directives) fn scan_option_lines(
    unindented_lines: &[String],
) -> (Vec<OptionLine>, usize) {
    let mut options = Vec::new();
    let mut index = 0;
    while index < unindented_lines.len() {
        let line = unindented_lines[index].trim();
        if line.is_empty() {
            index += 1;
            continue;
        }
        let Some(rest) = line.strip_prefix(':') else {
            break;
        };
        let (name, value) = match rest.split_once(':') {
            Some((name, value)) => (name, value.trim()),
            None => (rest, ""),
        };
        options.push(OptionLine {
            name: name.to_string(),
            value: value.to_string(),
            raw: line.to_string(),
            line_index: index,
        });
        index += 1;
    }
    (options, index)
}

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

/// Reports every option line neither the shared parser nor the directive's own
/// parser claimed.
pub(in crate::directives) fn report_unknown_options(
    unrecognized: &[&OptionLine],
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    for line in unrecognized {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::DirectiveUnknownOption,
            format!(
                "Invalid or non-standard Sphinx {directive} option encountered: {}",
                line.raw
            ),
            ctx.line_span(line.line_index, &line.raw),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::Domain;

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn test_scan_option_lines_splits_name_and_value() {
        // Given
        let body = lines(&[":header-rows: 1", ":widths: 30 70", "", "* - Cell"]);

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].name, "header-rows");
        assert_eq!(options[0].value, "1");
        assert_eq!(options[1].name, "widths");
        assert_eq!(options[1].value, "30 70");
        assert_eq!(body_start, 3);
    }

    #[test]
    fn test_scan_option_lines_accepts_a_value_with_no_space_after_the_colon() {
        // Given
        let body = lines(&[":header-rows:1"]);

        // When
        let (options, _) = scan_option_lines(&body);

        // Then
        assert_eq!(options[0].name, "header-rows");
        assert_eq!(options[0].value, "1");
    }

    #[test]
    fn test_scan_option_lines_treats_a_flag_option_as_an_empty_value() {
        // Given
        let body = lines(&[":keepspace:"]);

        // When
        let (options, _) = scan_option_lines(&body);

        // Then
        assert_eq!(options[0].name, "keepspace");
        assert_eq!(options[0].value, "");
    }

    #[test]
    fn test_scan_option_lines_keeps_an_unterminated_option_line() {
        // Given
        let body = lines(&[":oops"]);

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].name, "oops");
        assert_eq!(options[0].raw, ":oops");
        assert_eq!(body_start, 1);
    }

    #[test]
    fn test_scan_option_lines_stops_at_the_first_body_line() {
        // Given
        let body = lines(&[":widths: auto", "Apple, Red", ":not-an-option: x"]);

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert_eq!(options.len(), 1);
        assert_eq!(body_start, 1);
    }

    #[test]
    fn test_scan_option_lines_returns_nothing_for_an_empty_body() {
        // Given
        let body: Vec<String> = Vec::new();

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert!(options.is_empty());
        assert_eq!(body_start, 0);
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

    #[test]
    fn test_report_unknown_options_quotes_the_source_line() {
        // Given
        let body = lines(&[":bogus: value"]);
        let (option_lines, _) = scan_option_lines(&body);
        let unrecognized: Vec<&OptionLine> = option_lines.iter().collect();
        let mut diagnostics = Diagnostics::default();

        // When
        report_unknown_options(
            &unrecognized,
            "list-table",
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].message.contains(
                "Invalid or non-standard Sphinx list-table option encountered: :bogus: value"
            ),
            "{}",
            diagnostics[0].message
        );
    }
}
