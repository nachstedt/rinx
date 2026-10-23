//! The `:widths:` option shared by every option-bearing table directive —
//! `.. list-table::`, `.. csv-table::` and `.. table::` alike.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use rinx_ast::{Diagnostic, DiagnosticCode, TableWidths};

use super::option_value::{Located, OptionValueType};
use super::options::OptionLine;
use super::spec::OptionValue;

/// `:widths:` read on its own line: `auto`, `grid`, or a list of relative
/// widths. Whether a list has one width per column only the content says, so
/// that is [`widths_for_columns`]'s to check.
impl OptionValueType for TableWidths {
    const VALUE: OptionValue<'static> = OptionValue::Text;

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        match line.value.as_str() {
            "auto" => Ok(TableWidths::Auto),
            "grid" => Ok(TableWidths::Grid),
            raw => raw
                .split([',', ' '])
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::parse)
                .collect::<Result<Vec<u32>, _>>()
                .map(TableWidths::Explicit)
                .map_err(|_| format!("value '{raw}' is not 'auto', 'grid', or a list of integers")),
        }
    }
}

/// The widths once the table's `ncols` columns are known: an explicit list of
/// another length is reported on the `:widths:` line and dropped.
pub(in crate::directives) fn widths_for_columns(
    widths: Located<TableWidths>,
    ncols: usize,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<TableWidths> {
    if let TableWidths::Explicit(values) = &widths.value
        && values.len() != ncols
    {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::TableDataWidthsCountMismatch,
            format!(
                "{directive}: :widths: gives {} value(s), but the table has {ncols} column(s)",
                values.len()
            ),
            widths.span(ctx),
        ));
        return None;
    }
    Some(widths.value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::Domain;

    /// A `:widths:` line written `value`.
    fn widths(value: &str) -> OptionLine {
        OptionLine {
            name: "widths".to_string(),
            value: value.to_string(),
            raw: format!(":widths: {value}"),
            line_index: 0,
        }
    }

    fn read(value: &str) -> Result<TableWidths, String> {
        TableWidths::read(&widths(value), &ParseCtx::with_domain(Domain::Py))
    }

    #[test]
    fn test_read_accepts_auto_and_grid() {
        // Given / When / Then
        assert_eq!(read("auto"), Ok(TableWidths::Auto));
        assert_eq!(read("grid"), Ok(TableWidths::Grid));
    }

    #[test]
    fn test_read_accepts_a_space_or_comma_separated_list() {
        // Given / When / Then
        assert_eq!(read("30 70"), Ok(TableWidths::Explicit(vec![30, 70])));
        assert_eq!(read("30, 70"), Ok(TableWidths::Explicit(vec![30, 70])));
    }

    #[test]
    fn test_read_refuses_a_value_that_is_none_of_them() {
        // Given / When
        let problem = read("banana").unwrap_err();

        // Then
        assert!(problem.contains("'banana'"), "{problem}");
    }

    /// The widths `value` gives a table of `ncols` columns, and what it
    /// reported.
    fn for_columns(value: &str, ncols: usize) -> (Option<TableWidths>, Diagnostics) {
        let ctx = ParseCtx::with_domain(Domain::Py);
        let line = widths(value);
        let located = Located::read(&line, &ctx).expect("reads");
        let mut diagnostics = Diagnostics::default();
        let resolved = widths_for_columns(located, ncols, "list-table", &mut diagnostics, &ctx);
        (resolved, diagnostics)
    }

    #[test]
    fn test_widths_for_columns_keeps_a_list_of_the_right_length() {
        // Given / When
        let (resolved, diagnostics) = for_columns("30 70", 2);

        // Then
        assert_eq!(resolved, Some(TableWidths::Explicit(vec![30, 70])));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_widths_for_columns_reports_a_list_of_another_length() {
        // Given / When
        let (resolved, diagnostics) = for_columns("30 30 40", 2);

        // Then
        assert_eq!(resolved, None);
        let codes: Vec<DiagnosticCode> = diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(codes, [DiagnosticCode::TableDataWidthsCountMismatch]);
    }

    #[test]
    fn test_widths_for_columns_keeps_auto_whatever_the_columns() {
        // Given / When / Then
        assert_eq!(for_columns("auto", 5).0, Some(TableWidths::Auto));
    }
}
