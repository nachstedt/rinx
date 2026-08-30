//! The `:widths:` option shared by both data-table directives.

use rusty_sphinx_ast::TableWidths;

/// Resolves a `:widths:` option's raw string into a [`TableWidths`],
/// validating an explicit integer list against the table's actual column
/// count. Not a `FromStr` impl since that validation needs `ncols`, which
/// isn't known until every row has been built.
pub(super) fn parse_widths_option(
    raw: &str,
    ncols: usize,
    directive: &str,
    diagnostics: &mut Vec<String>,
) -> Option<TableWidths> {
    match raw {
        "auto" => Some(TableWidths::Auto),
        "grid" => Some(TableWidths::Grid),
        _ => {
            let values: Result<Vec<u32>, _> = raw
                .split([',', ' '])
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::parse)
                .collect();
            match values {
                Ok(values) if values.len() == ncols => Some(TableWidths::Explicit(values)),
                Ok(values) => {
                    diagnostics.push(format!(
                        "{directive}: :widths: gives {} value(s), but the table has {ncols} column(s)",
                        values.len()
                    ));
                    None
                }
                Err(_) => {
                    diagnostics.push(format!(
                        "{directive}: :widths: value '{raw}' is not 'auto', 'grid', or a list of integers"
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

    #[test]
    fn test_parse_widths_option_accepts_auto() {
        // Given
        let mut diagnostics = Vec::new();

        // When
        let result = parse_widths_option("auto", 2, "csv-table", &mut diagnostics);

        // Then
        assert_eq!(result, Some(TableWidths::Auto));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_widths_option_accepts_grid() {
        // Given
        let mut diagnostics = Vec::new();

        // When
        let result = parse_widths_option("grid", 2, "list-table", &mut diagnostics);

        // Then
        assert_eq!(result, Some(TableWidths::Grid));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_widths_option_accepts_a_space_separated_list() {
        // Given
        let mut diagnostics = Vec::new();

        // When
        let result = parse_widths_option("30 70", 2, "list-table", &mut diagnostics);

        // Then
        assert_eq!(result, Some(TableWidths::Explicit(vec![30, 70])));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_widths_option_accepts_a_comma_separated_list() {
        // Given
        let mut diagnostics = Vec::new();

        // When
        let result = parse_widths_option("30, 70", 2, "csv-table", &mut diagnostics);

        // Then
        assert_eq!(result, Some(TableWidths::Explicit(vec![30, 70])));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_widths_option_rejects_a_count_mismatch() {
        // Given
        let mut diagnostics = Vec::new();

        // When
        let result = parse_widths_option("30 30 40", 2, "csv-table", &mut diagnostics);

        // Then
        assert_eq!(result, None);
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].starts_with("csv-table: :widths: gives 3"),
            "{}",
            diagnostics[0]
        );
    }

    #[test]
    fn test_parse_widths_option_rejects_unparseable_value() {
        // Given
        let mut diagnostics = Vec::new();

        // When
        let result = parse_widths_option("banana", 2, "list-table", &mut diagnostics);

        // Then
        assert_eq!(result, None);
        assert_eq!(diagnostics.len(), 1);
    }
}
