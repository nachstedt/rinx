use std::fmt;

use rinx_filter::FieldValue;
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

/// What an attribute is worth to a filter.
///
/// Here rather than beside either caller because there are two, in crates that
/// may not depend on each other: `rinx_index`'s `EntitySubject`
/// answers a filter over an entity already in the index, and the parser's
/// `.. needimport::` answers one over a need it is about to build. Two copies
/// of this mapping would let a filter select differently depending on where
/// the entity came from — which is the one thing an import must not do.
impl From<&AttributeValue> for FieldValue {
    fn from(value: &AttributeValue) -> Self {
        match value {
            AttributeValue::String(text) => Self::Text(text.clone()),
            AttributeValue::Int(number) => Self::Int(*number),
            AttributeValue::Bool(flag) => Self::Bool(*flag),
            AttributeValue::List(items) => Self::List(items.clone()),
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

    #[test]
    fn test_attribute_value_maps_onto_the_filter_value_of_the_same_shape() {
        // Given one value of each shape
        let cases = [
            (
                AttributeValue::String("open".to_string()),
                FieldValue::Text("open".to_string()),
            ),
            (AttributeValue::Int(3), FieldValue::Int(3)),
            (AttributeValue::Bool(true), FieldValue::Bool(true)),
            (
                AttributeValue::List(vec!["a".to_string(), "b".to_string()]),
                FieldValue::List(vec!["a".to_string(), "b".to_string()]),
            ),
        ];

        // When each is converted for a filter
        // Then it keeps its shape, so a filter cannot see a number as text
        for (value, expected) in cases {
            assert_eq!(FieldValue::from(&value), expected);
        }
    }
}
