use std::fmt;

use rusty_sphinx_ast::AttributeValue;
use serde::{Deserialize, Serialize};

/// The type an attribute's value is parsed and validated against.
///
/// Each kind carries only the data it needs — the enum kinds their permitted
/// values, the others nothing — rather than one struct holding the union of
/// every kind's options, most of them meaningless on most attributes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttributeType {
    /// A single line of plain text.
    String,
    /// Free text, which may span several lines. Still never parsed as RST —
    /// that is what a section is for.
    Text,
    /// A whole number.
    Int,
    /// A flag. Written `:deprecated:` with no value, or with an explicit
    /// `true`/`false`.
    Bool,
    /// One of a fixed set of values.
    Enum { values: Vec<String> },
    /// A comma-separated list of plain strings.
    StringList,
    /// A comma-separated list drawn from a fixed set of values.
    EnumList { values: Vec<String> },
}

impl AttributeType {
    /// Returns the permitted values, for the two kinds that constrain them.
    #[must_use]
    pub fn permitted_values(&self) -> Option<&[String]> {
        match self {
            Self::Enum { values } | Self::EnumList { values } => Some(values),
            _ => None,
        }
    }

    /// Reports whether a value of this type is a list of items.
    #[must_use]
    pub fn is_list(&self) -> bool {
        matches!(self, Self::StringList | Self::EnumList { .. })
    }

    /// The spelling used in a schema file's `type = "..."` key.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Text => "text",
            Self::Int => "int",
            Self::Bool => "bool",
            Self::Enum { .. } => "enum",
            Self::StringList => "list<string>",
            Self::EnumList { .. } => "list<enum>",
        }
    }
}

/// A single attribute declared by an entity type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeSchema {
    /// The option spelling, e.g. `status` for `:status:`.
    pub name: String,
    /// Human-readable label for the default rendering; falls back to `name`.
    pub label: Option<String>,
    /// The value type.
    pub value_type: AttributeType,
    /// Whether the option must be given. An attribute with a `default` is
    /// never missing, so the two are mutually exclusive.
    pub required: bool,
    /// The value used when the option is absent.
    pub default: Option<String>,
}

impl AttributeSchema {
    /// The label to display, falling back to the option name.
    #[must_use]
    pub fn display_label(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.name)
    }
}

/// Why an option's text could not become an [`AttributeValue`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeParseError {
    /// The text was not a whole number.
    NotAnInteger { text: String },
    /// The text was not a recognised boolean spelling.
    NotABoolean { text: String },
    /// The text was outside an enum's permitted values.
    NotPermitted {
        text: String,
        permitted: Vec<String>,
    },
    /// An empty value was given for a type that needs one.
    Missing,
}

impl fmt::Display for AttributeParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAnInteger { text } => write!(f, "expected a whole number, found {text:?}"),
            Self::NotABoolean { text } => {
                write!(f, "expected `true` or `false`, found {text:?}")
            }
            Self::NotPermitted { text, permitted } => write!(
                f,
                "{text:?} is not one of the permitted values: {}",
                permitted.join(", ")
            ),
            Self::Missing => write!(f, "a value is required"),
        }
    }
}

impl std::error::Error for AttributeParseError {}

/// Parses an option's raw text into a value of `value_type`.
///
/// # Errors
///
/// Returns [`AttributeParseError`] when the text does not fit the type.
pub fn parse_attribute_value(
    value_type: &AttributeType,
    raw: &str,
) -> Result<AttributeValue, AttributeParseError> {
    let text = raw.trim();
    match value_type {
        AttributeType::String | AttributeType::Text => {
            if text.is_empty() {
                Err(AttributeParseError::Missing)
            } else {
                Ok(AttributeValue::String(text.to_string()))
            }
        }
        AttributeType::Int => text.parse::<i64>().map(AttributeValue::Int).map_err(|_| {
            AttributeParseError::NotAnInteger {
                text: text.to_string(),
            }
        }),
        // A bare `:deprecated:` is the ordinary RST spelling of a flag, so an
        // empty value means true rather than being a missing value.
        AttributeType::Bool => match text {
            "" | "true" => Ok(AttributeValue::Bool(true)),
            "false" => Ok(AttributeValue::Bool(false)),
            other => Err(AttributeParseError::NotABoolean {
                text: other.to_string(),
            }),
        },
        AttributeType::Enum { values } => {
            if text.is_empty() {
                return Err(AttributeParseError::Missing);
            }
            if values.iter().any(|v| v == text) {
                Ok(AttributeValue::String(text.to_string()))
            } else {
                Err(AttributeParseError::NotPermitted {
                    text: text.to_string(),
                    permitted: values.clone(),
                })
            }
        }
        AttributeType::StringList => Ok(AttributeValue::List(split_list(text))),
        AttributeType::EnumList { values } => {
            let items = split_list(text);
            if let Some(bad) = items.iter().find(|item| !values.contains(item)) {
                return Err(AttributeParseError::NotPermitted {
                    text: bad.clone(),
                    permitted: values.clone(),
                });
            }
            Ok(AttributeValue::List(items))
        }
    }
}

/// Splits a comma-separated option value, trimming items and dropping empties.
///
/// Shared by both list types and by relation option values, so the three
/// cannot disagree about whether `a, , b` holds two items or three.
pub fn split_list(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status_enum() -> AttributeType {
        AttributeType::Enum {
            values: vec!["open".to_string(), "closed".to_string()],
        }
    }

    #[test]
    fn test_parse_attribute_value_reads_a_string() {
        // Given
        let value_type = AttributeType::String;

        // When
        let value = parse_attribute_value(&value_type, "  hello  ").unwrap();

        // Then
        assert_eq!(value, AttributeValue::String("hello".to_string()));
    }

    #[test]
    fn test_parse_attribute_value_rejects_an_empty_string() {
        // Given
        let value_type = AttributeType::String;

        // When
        let result = parse_attribute_value(&value_type, "   ");

        // Then
        assert_eq!(result, Err(AttributeParseError::Missing));
    }

    #[test]
    fn test_parse_attribute_value_reads_an_integer() {
        // Given
        let value_type = AttributeType::Int;

        // When
        let value = parse_attribute_value(&value_type, "-42").unwrap();

        // Then
        assert_eq!(value, AttributeValue::Int(-42));
    }

    #[test]
    fn test_parse_attribute_value_rejects_a_non_integer() {
        // Given
        let value_type = AttributeType::Int;

        // When
        let result = parse_attribute_value(&value_type, "3.5");

        // Then
        assert_eq!(
            result,
            Err(AttributeParseError::NotAnInteger {
                text: "3.5".to_string()
            })
        );
    }

    #[test]
    fn test_parse_attribute_value_treats_a_bare_flag_as_true() {
        // Given — the ordinary RST spelling of a flag option
        let value_type = AttributeType::Bool;

        // When
        let value = parse_attribute_value(&value_type, "").unwrap();

        // Then
        assert_eq!(value, AttributeValue::Bool(true));
    }

    #[test]
    fn test_parse_attribute_value_reads_an_explicit_boolean() {
        // Given
        let value_type = AttributeType::Bool;

        // When
        let yes = parse_attribute_value(&value_type, "true").unwrap();
        let no = parse_attribute_value(&value_type, "false").unwrap();

        // Then
        assert_eq!(yes, AttributeValue::Bool(true));
        assert_eq!(no, AttributeValue::Bool(false));
    }

    #[test]
    fn test_parse_attribute_value_rejects_an_unrecognised_boolean() {
        // Given
        let value_type = AttributeType::Bool;

        // When
        let result = parse_attribute_value(&value_type, "yes");

        // Then
        assert_eq!(
            result,
            Err(AttributeParseError::NotABoolean {
                text: "yes".to_string()
            })
        );
    }

    #[test]
    fn test_parse_attribute_value_accepts_a_permitted_enum_value() {
        // Given
        let value_type = status_enum();

        // When
        let value = parse_attribute_value(&value_type, "open").unwrap();

        // Then
        assert_eq!(value, AttributeValue::String("open".to_string()));
    }

    #[test]
    fn test_parse_attribute_value_rejects_an_unpermitted_enum_value() {
        // Given
        let value_type = status_enum();

        // When
        let result = parse_attribute_value(&value_type, "pending");

        // Then
        assert_eq!(
            result,
            Err(AttributeParseError::NotPermitted {
                text: "pending".to_string(),
                permitted: vec!["open".to_string(), "closed".to_string()],
            })
        );
    }

    #[test]
    fn test_parse_attribute_value_splits_a_string_list() {
        // Given
        let value_type = AttributeType::StringList;

        // When
        let value = parse_attribute_value(&value_type, "boot, kernel").unwrap();

        // Then
        assert_eq!(
            value,
            AttributeValue::List(vec!["boot".to_string(), "kernel".to_string()])
        );
    }

    #[test]
    fn test_parse_attribute_value_reports_the_offending_item_of_an_enum_list() {
        // Given
        let value_type = AttributeType::EnumList {
            values: vec!["open".to_string(), "closed".to_string()],
        };

        // When
        let result = parse_attribute_value(&value_type, "open, pending");

        // Then — the diagnostic names the item, not the whole list
        assert_eq!(
            result,
            Err(AttributeParseError::NotPermitted {
                text: "pending".to_string(),
                permitted: vec!["open".to_string(), "closed".to_string()],
            })
        );
    }

    #[test]
    fn test_parse_attribute_value_reads_an_empty_list_as_empty() {
        // Given
        let value_type = AttributeType::StringList;

        // When
        let value = parse_attribute_value(&value_type, "").unwrap();

        // Then
        assert_eq!(value, AttributeValue::List(Vec::new()));
    }

    #[test]
    fn test_split_list_trims_items_and_drops_empties() {
        // Given
        let text = " a , , b ,";

        // When
        let items = split_list(text);

        // Then
        assert_eq!(items, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn test_attribute_type_reports_its_permitted_values() {
        // Given
        let constrained = status_enum();
        let unconstrained = AttributeType::String;

        // When
        let some = constrained.permitted_values();
        let none = unconstrained.permitted_values();

        // Then
        assert_eq!(some, Some(&["open".to_string(), "closed".to_string()][..]));
        assert_eq!(none, None);
    }

    #[test]
    fn test_attribute_type_reports_which_kinds_are_lists() {
        // Given
        let kinds = [
            (AttributeType::String, false),
            (AttributeType::Text, false),
            (AttributeType::Int, false),
            (AttributeType::Bool, false),
            (status_enum(), false),
            (AttributeType::StringList, true),
            (AttributeType::EnumList { values: Vec::new() }, true),
        ];

        // When / Then
        for (kind, expected) in kinds {
            assert_eq!(kind.is_list(), expected, "for {}", kind.as_str());
        }
    }

    #[test]
    fn test_attribute_type_spells_each_kind_as_the_schema_writes_it() {
        // Given / When / Then
        assert_eq!(AttributeType::String.as_str(), "string");
        assert_eq!(AttributeType::Text.as_str(), "text");
        assert_eq!(AttributeType::Int.as_str(), "int");
        assert_eq!(AttributeType::Bool.as_str(), "bool");
        assert_eq!(status_enum().as_str(), "enum");
        assert_eq!(AttributeType::StringList.as_str(), "list<string>");
        assert_eq!(
            AttributeType::EnumList { values: Vec::new() }.as_str(),
            "list<enum>"
        );
    }

    #[test]
    fn test_attribute_schema_falls_back_to_the_name_as_its_label() {
        // Given
        let unlabelled = AttributeSchema {
            name: "status".to_string(),
            label: None,
            value_type: AttributeType::String,
            required: false,
            default: None,
        };
        let labelled = AttributeSchema {
            label: Some("Current status".to_string()),
            ..unlabelled.clone()
        };

        // When / Then
        assert_eq!(unlabelled.display_label(), "status");
        assert_eq!(labelled.display_label(), "Current status");
    }

    #[test]
    fn test_attribute_parse_error_messages_name_the_offending_text() {
        // Given
        let errors = [
            AttributeParseError::NotAnInteger {
                text: "3.5".to_string(),
            },
            AttributeParseError::NotABoolean {
                text: "yes".to_string(),
            },
            AttributeParseError::NotPermitted {
                text: "pending".to_string(),
                permitted: vec!["open".to_string()],
            },
        ];
        let expected_fragments = ["3.5", "yes", "pending"];

        // When / Then
        for (error, fragment) in errors.iter().zip(expected_fragments) {
            assert!(
                error.to_string().contains(fragment),
                "{error:?} should mention {fragment}"
            );
        }
        assert_eq!(
            AttributeParseError::Missing.to_string(),
            "a value is required"
        );
    }
}
