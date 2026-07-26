use serde::{Deserialize, Serialize};

/// The `:widths:` option of `.. list-table::`. Unlike most directive-kind
/// fields in this codebase, this has no `FromStr` impl of its own: resolving
/// the explicit-integer-list form needs the table's actual column count (to
/// diagnose a mismatch), which isn't available to a context-free `FromStr`
/// call — so parsing is done by a dedicated helper in
/// `rusty_sphinx_parser::list_table` instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListTableWidths {
    /// `:widths: auto` — let the renderer decide column widths.
    Auto,
    /// `:widths: grid` — size columns from the source's bullet-list
    /// indentation. rusty-sphinx has no such layout signal to measure, so
    /// this is accepted (to avoid rejecting valid Sphinx documents) and
    /// rendered identically to `Auto`.
    Grid,
    /// An explicit list of relative column-width weights, one per column.
    Explicit(Vec<u32>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_table_widths_auto_serialization_roundtrip() {
        // Given
        let widths = ListTableWidths::Auto;

        // When
        let json = serde_json::to_string(&widths).expect("Failed to serialize");
        let deserialized: ListTableWidths =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(widths, deserialized);
    }

    #[test]
    fn test_list_table_widths_grid_serialization_roundtrip() {
        // Given
        let widths = ListTableWidths::Grid;

        // When
        let json = serde_json::to_string(&widths).expect("Failed to serialize");
        let deserialized: ListTableWidths =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(widths, deserialized);
    }

    #[test]
    fn test_list_table_widths_explicit_serialization_roundtrip() {
        // Given
        let widths = ListTableWidths::Explicit(vec![30, 70]);

        // When
        let json = serde_json::to_string(&widths).expect("Failed to serialize");
        let deserialized: ListTableWidths =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(widths, deserialized);
    }
}
