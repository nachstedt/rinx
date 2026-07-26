use serde::{Deserialize, Serialize};

/// The `:align:` option of `.. list-table::` — the table's horizontal
/// alignment on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TableAlign {
    Left,
    Center,
    Right,
}

impl TableAlign {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
}

impl std::str::FromStr for TableAlign {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "left" => Ok(Self::Left),
            "center" => Ok(Self::Center),
            "right" => Ok(Self::Right),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for TableAlign {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_align_serialization_roundtrip() {
        // Given
        let align = TableAlign::Center;

        // When
        let json = serde_json::to_string(&align).expect("Failed to serialize");
        let deserialized: TableAlign = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(json, "\"center\"");
        assert_eq!(align, deserialized);
    }

    #[test]
    fn test_table_align_from_str_parses_all_variants() {
        // Given / When / Then
        assert_eq!("left".parse(), Ok(TableAlign::Left));
        assert_eq!("center".parse(), Ok(TableAlign::Center));
        assert_eq!("right".parse(), Ok(TableAlign::Right));
    }

    #[test]
    fn test_table_align_from_str_rejects_unknown_value() {
        // Given
        let input = "diagonal";

        // When
        let result = input.parse::<TableAlign>();

        // Then
        assert_eq!(result, Err(()));
    }

    #[test]
    fn test_table_align_as_str_round_trips_through_from_str() {
        // Given
        let variants = [TableAlign::Left, TableAlign::Center, TableAlign::Right];

        // When / Then
        for variant in variants {
            assert_eq!(variant.as_str().parse(), Ok(variant));
        }
    }

    #[test]
    fn test_table_align_display_matches_as_str() {
        // Given
        let align = TableAlign::Right;

        // When
        let displayed = align.to_string();

        // Then
        assert_eq!(displayed, "right");
    }
}
