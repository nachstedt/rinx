//! Scanning and interpreting the `:option:` lines of a data-table directive.
//!
//! Both `.. list-table::` and `.. csv-table::` open with a run of
//! `:name: value` lines before their real body. [`scan_option_lines`] turns
//! that run into name/value pairs without judging them, and
//! [`parse_shared_table_options`] consumes the seven options the two
//! directives have in common — handing back whatever is left so each
//! directive can claim its own options and diagnose the rest.

use rusty_sphinx_ast::{TableAlign, TableSource, TargetName};

/// One `:name: value` line, with the source text kept so a diagnostic can
/// quote the line exactly as the author wrote it.
pub(super) struct OptionLine {
    pub name: String,
    pub value: String,
    pub raw: String,
}

/// Splits the leading `:option:` lines off an already-unindented directive
/// body, returning them plus the index of the first line that isn't an option
/// (where the real body content starts).
///
/// A line whose closing colon is missing (`:oops`) is still returned, with the
/// whole remainder as its `name`, so that it reaches the caller's
/// unknown-option diagnostic rather than being silently swallowed as body.
pub(super) fn scan_option_lines(unindented_lines: &[String]) -> (Vec<OptionLine>, usize) {
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
        });
        index += 1;
    }
    (options, index)
}

/// The options `.. list-table::` and `.. csv-table::` share.
///
/// `widths` is kept as its raw string (not yet resolved to a
/// [`rusty_sphinx_ast::TableWidths`]) because validating it needs the table's
/// column count, which isn't known until the rows have been built.
pub(super) struct SharedTableOptions {
    pub header_rows: usize,
    pub stub_columns: usize,
    pub widths_raw: Option<String>,
    pub width: Option<String>,
    pub align: Option<TableAlign>,
    pub classes: Vec<String>,
    pub name: Option<TargetName>,
}

impl SharedTableOptions {
    fn empty() -> Self {
        Self {
            header_rows: 0,
            stub_columns: 0,
            widths_raw: None,
            width: None,
            align: None,
            classes: Vec::new(),
            name: None,
        }
    }
}

/// Consumes the shared options out of `option_lines`, returning them together
/// with the lines it did not recognize, in source order.
///
/// `source` only shapes the diagnostics' wording, so a `csv-table` author is
/// never told about a `list-table` option.
pub(super) fn parse_shared_table_options<'a>(
    option_lines: &'a [OptionLine],
    source: TableSource,
    diagnostics: &mut Vec<String>,
) -> (SharedTableOptions, Vec<&'a OptionLine>) {
    let mut options = SharedTableOptions::empty();
    let mut unrecognized = Vec::new();
    let directive = source.as_str();

    for line in option_lines {
        match line.name.as_str() {
            "header-rows" => {
                options.header_rows =
                    parse_nonneg_int_option(&line.value, "header-rows", directive, diagnostics);
            }
            "stub-columns" => {
                options.stub_columns =
                    parse_nonneg_int_option(&line.value, "stub-columns", directive, diagnostics);
            }
            "widths" => options.widths_raw = Some(line.value.clone()),
            "width" => options.width = Some(line.value.clone()),
            "align" => match line.value.parse::<TableAlign>() {
                Ok(parsed) => options.align = Some(parsed),
                Err(()) => diagnostics.push(format!(
                    "{directive}: invalid :align: value '{}', expected 'left', 'center', or 'right'",
                    line.value
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
pub(super) fn report_unknown_options(
    unrecognized: &[&OptionLine],
    source: TableSource,
    diagnostics: &mut Vec<String>,
) {
    for line in unrecognized {
        diagnostics.push(format!(
            "Invalid or non-standard Sphinx {} option encountered: {}",
            source.as_str(),
            line.raw
        ));
    }
}

/// Parses a nonnegative-integer option value (`:header-rows:`/
/// `:stub-columns:`), pushing a diagnostic and defaulting to 0 on a malformed
/// value rather than failing the whole directive.
pub(super) fn parse_nonneg_int_option(
    raw: &str,
    option_name: &str,
    directive: &str,
    diagnostics: &mut Vec<String>,
) -> usize {
    if let Ok(value) = raw.parse::<usize>() {
        value
    } else {
        diagnostics.push(format!(
            "{directive}: :{option_name}: value '{raw}' is not a nonnegative integer"
        ));
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut diagnostics = Vec::new();

        // When
        let (options, unrecognized) =
            parse_shared_table_options(&option_lines, TableSource::List, &mut diagnostics);

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
        let mut diagnostics = Vec::new();

        // When
        let (_, unrecognized) =
            parse_shared_table_options(&option_lines, TableSource::Csv, &mut diagnostics);

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
        let mut diagnostics = Vec::new();

        // When
        let (options, _) =
            parse_shared_table_options(&option_lines, TableSource::Csv, &mut diagnostics);

        // Then
        assert_eq!(options.align, None);
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].starts_with("csv-table: invalid :align:"),
            "{}",
            diagnostics[0]
        );
    }

    #[test]
    fn test_parse_shared_table_options_names_the_directive_in_its_diagnostics() {
        // Given
        let body = lines(&[":header-rows: many"]);
        let (option_lines, _) = scan_option_lines(&body);
        let mut diagnostics = Vec::new();

        // When
        parse_shared_table_options(&option_lines, TableSource::Csv, &mut diagnostics);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].starts_with("csv-table:"),
            "{}",
            diagnostics[0]
        );
    }

    #[test]
    fn test_report_unknown_options_quotes_the_source_line() {
        // Given
        let body = lines(&[":bogus: value"]);
        let (option_lines, _) = scan_option_lines(&body);
        let unrecognized: Vec<&OptionLine> = option_lines.iter().collect();
        let mut diagnostics = Vec::new();

        // When
        report_unknown_options(&unrecognized, TableSource::List, &mut diagnostics);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].contains(
                "Invalid or non-standard Sphinx list-table option encountered: :bogus: value"
            ),
            "{}",
            diagnostics[0]
        );
    }

    #[test]
    fn test_parse_nonneg_int_option_accepts_a_number() {
        // Given
        let mut diagnostics = Vec::new();

        // When
        let value = parse_nonneg_int_option("3", "header-rows", "list-table", &mut diagnostics);

        // Then
        assert_eq!(value, 3);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_nonneg_int_option_rejects_non_numeric_value() {
        // Given
        let mut diagnostics = Vec::new();

        // When
        let value = parse_nonneg_int_option("abc", "header-rows", "list-table", &mut diagnostics);

        // Then
        assert_eq!(value, 0);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("header-rows"));
    }
}
