//! The `:name: value` option lines any directive may open its body with, and
//! the diagnostic for the ones nobody claimed.
//!
//! Nothing here knows what any particular option *means*: [`scan_option_lines`]
//! splits the leading run of `:name: value` lines off a body without judging
//! them, and [`report_unknown_options`] turns whatever the directive's own
//! parser handed back into diagnostics. Interpretation belongs to the
//! directive — [`super::table_options`] layers the five presentation options
//! the table directives share on top of this, and `super::math` reads
//! `:label:`/`:nowrap:`/`:class:` off the same scan.
//!
//! Split out of `table_options` once `.. math::` needed the scanning without
//! any of the table options, so the generic half stops being named after one
//! caller's construct.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use rusty_sphinx_ast::{Diagnostic, DiagnosticCode};

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

/// Reports every option line neither a shared parser nor the directive's own
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
