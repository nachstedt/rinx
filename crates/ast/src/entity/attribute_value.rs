use std::fmt;

use serde::{Deserialize, Serialize};

/// A validated attribute value, as stored in the AST and the project index.
///
/// Enum-ness is deliberately not represented: an enum value is a `String` that
/// was checked against its schema when parsed. The schema is available
/// wherever a value is read, so re-encoding the constraint in the value would
/// be a second source of truth free to disagree with the first.
///
/// This is the *value* half of the model — the half that is indexed and, in a
/// later increment, filtered. A section's prose is a node tree instead, and
/// never becomes one of these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttributeValue {
    String(String),
    Int(i64),
    Bool(bool),
    List(Vec<String>),
}

impl AttributeValue {
    /// The items of a list value, or the single item of a scalar one.
    ///
    /// Lets a caller that treats one and many alike — rendering a cell,
    /// matching a filter — avoid branching on the shape.
    #[must_use]
    pub fn items(&self) -> Vec<String> {
        match self {
            Self::List(items) => items.clone(),
            other => vec![other.to_string()],
        }
    }
}

impl fmt::Display for AttributeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(s) => f.write_str(s),
            Self::Int(i) => write!(f, "{i}"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::List(items) => f.write_str(&items.join(", ")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attribute_value_displays_a_list_as_a_comma_separated_line() {
        // Given
        let value = AttributeValue::List(vec!["boot".to_string(), "kernel".to_string()]);

        // When
        let shown = value.to_string();

        // Then
        assert_eq!(shown, "boot, kernel");
    }

    #[test]
    fn test_attribute_value_displays_each_scalar_kind() {
        // Given / When / Then
        assert_eq!(
            AttributeValue::String("open".to_string()).to_string(),
            "open"
        );
        assert_eq!(AttributeValue::Int(7).to_string(), "7");
        assert_eq!(AttributeValue::Bool(true).to_string(), "true");
    }

    #[test]
    fn test_items_returns_a_list_value_as_it_stands() {
        // Given
        let value = AttributeValue::List(vec!["boot".to_string(), "kernel".to_string()]);

        // When
        let items = value.items();

        // Then
        assert_eq!(items, vec!["boot".to_string(), "kernel".to_string()]);
    }

    #[test]
    fn test_items_wraps_a_scalar_value_as_a_single_item() {
        // Given
        let value = AttributeValue::Int(7);

        // When
        let items = value.items();

        // Then
        assert_eq!(items, vec!["7".to_string()]);
    }

    #[test]
    fn test_attribute_value_survives_a_serialization_round_trip() {
        // Given
        let values = [
            AttributeValue::String("open".to_string()),
            AttributeValue::Int(-3),
            AttributeValue::Bool(false),
            AttributeValue::List(vec!["a".to_string()]),
        ];

        // When / Then
        for original in values {
            let json = serde_json::to_string(&original).unwrap();
            let restored: AttributeValue = serde_json::from_str(&json).unwrap();
            assert_eq!(original, restored);
        }
    }
}
