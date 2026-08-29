use serde::{Deserialize, Serialize};

/// Which directive produced a [`crate::Directive::DataTable`].
///
/// Both `.. list-table::` and `.. csv-table::` lower to the same AST node —
/// they differ only in how their *source text* spells out the rows, and that
/// difference is fully consumed while parsing. This enum records which one it
/// was, purely so the renderer can emit the matching CSS class and the parser
/// can name the right directive in its diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TableSource {
    /// `.. list-table::` — rows given as a nested bullet list.
    List,
    /// `.. csv-table::` — rows given as CSV data.
    Csv,
}

impl TableSource {
    /// The directive's own name, used both as its rendered CSS class and in
    /// the parser's diagnostic messages.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::List => "list-table",
            Self::Csv => "csv-table",
        }
    }
}

impl std::str::FromStr for TableSource {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "list-table" => Ok(Self::List),
            "csv-table" => Ok(Self::Csv),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for TableSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_source_serialization_roundtrip() {
        // Given
        let source = TableSource::Csv;

        // When
        let json = serde_json::to_string(&source).expect("Failed to serialize");
        let deserialized: TableSource = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(json, "\"csv\"");
        assert_eq!(source, deserialized);
    }

    #[test]
    fn test_table_source_as_str_gives_the_directive_name() {
        // Given / When / Then
        assert_eq!(TableSource::List.as_str(), "list-table");
        assert_eq!(TableSource::Csv.as_str(), "csv-table");
    }

    #[test]
    fn test_table_source_from_str_parses_all_variants() {
        // Given / When / Then
        assert_eq!("list-table".parse(), Ok(TableSource::List));
        assert_eq!("csv-table".parse(), Ok(TableSource::Csv));
    }

    #[test]
    fn test_table_source_from_str_rejects_unknown_value() {
        // Given
        let input = "grid-table";

        // When
        let result = input.parse::<TableSource>();

        // Then
        assert_eq!(result, Err(()));
    }

    #[test]
    fn test_table_source_as_str_round_trips_through_from_str() {
        // Given
        let variants = [TableSource::List, TableSource::Csv];

        // When / Then
        for variant in variants {
            assert_eq!(variant.as_str().parse(), Ok(variant));
        }
    }

    #[test]
    fn test_table_source_display_matches_as_str() {
        // Given
        let source = TableSource::List;

        // When
        let displayed = source.to_string();

        // Then
        assert_eq!(displayed, "list-table");
    }
}
