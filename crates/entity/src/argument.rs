use std::fmt;

use serde::{Deserialize, Serialize};

/// How a directive's argument text is divided before being mapped onto
/// attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ArgumentSplit {
    /// The whole argument is one value. The sphinx-needs shape, where
    /// `.. req:: The system shall boot` is a title.
    #[default]
    Whole,
    /// The argument is a comma-separated signature. `CPython`'s shape, where
    /// `.. audit-event:: name, args, version` is three values.
    Comma,
}

/// How `.. <type>:: <argument>` maps onto the type's attributes.
///
/// A type with no `argument` declaration takes no argument at all, and one
/// given is diagnosed rather than silently dropped.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ArgumentSpec {
    /// Attribute names, in the order the argument's parts fill them.
    pub fields: Vec<String>,
    /// How the argument text is divided.
    pub split: ArgumentSplit,
}

/// Why an argument could not be mapped onto a type's fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgumentError {
    /// An argument was given to a type that declares none.
    Unexpected { text: String },
    /// The argument held more comma-separated parts than the type declares.
    TooManyParts { expected: usize, found: usize },
}

impl fmt::Display for ArgumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unexpected { text } => write!(
                f,
                "this entity type takes no argument, but {text:?} was given"
            ),
            Self::TooManyParts { expected, found } => write!(
                f,
                "the argument has {found} comma-separated parts, but this entity type declares {expected}"
            ),
        }
    }
}

impl std::error::Error for ArgumentError {}

impl ArgumentSpec {
    /// Reports whether this type takes an argument at all.
    #[must_use]
    pub fn takes_argument(&self) -> bool {
        !self.fields.is_empty()
    }

    /// Maps `raw` onto `(attribute name, value text)` pairs.
    ///
    /// Fewer parts than declared fields is not an error here: the trailing
    /// fields are simply unset, and whichever of them is `required` is caught
    /// by the ordinary required-attribute check, which produces a diagnostic
    /// naming the attribute rather than one about comma counting. More parts
    /// than declared *is* an error, since there is nowhere to put them and
    /// dropping them silently is the degradation these diagnostics exist to
    /// catch.
    ///
    /// # Errors
    ///
    /// Returns [`ArgumentError`] when the type takes no argument but one was
    /// given, or when the argument has more parts than the type declares.
    pub fn map_argument(&self, raw: &str) -> Result<Vec<(String, String)>, ArgumentError> {
        let text = raw.trim();
        if !self.takes_argument() {
            return if text.is_empty() {
                Ok(Vec::new())
            } else {
                Err(ArgumentError::Unexpected {
                    text: text.to_string(),
                })
            };
        }
        if text.is_empty() {
            return Ok(Vec::new());
        }

        let parts: Vec<&str> = match self.split {
            ArgumentSplit::Whole => vec![text],
            ArgumentSplit::Comma => text.split(',').map(str::trim).collect(),
        };
        if parts.len() > self.fields.len() {
            return Err(ArgumentError::TooManyParts {
                expected: self.fields.len(),
                found: parts.len(),
            });
        }

        Ok(self
            .fields
            .iter()
            .zip(parts)
            .filter(|(_, part)| !part.is_empty())
            .map(|(field, part)| (field.clone(), part.to_string()))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(fields: &[&str], split: ArgumentSplit) -> ArgumentSpec {
        ArgumentSpec {
            fields: fields.iter().map(|f| (*f).to_string()).collect(),
            split,
        }
    }

    #[test]
    fn test_map_argument_puts_the_whole_text_in_a_single_field() {
        // Given — the sphinx-needs shape
        let argument = spec(&["title"], ArgumentSplit::Whole);

        // When
        let mapped = argument
            .map_argument("The system shall boot, quickly")
            .unwrap();

        // Then — a comma in the title is text, not a separator
        assert_eq!(
            mapped,
            vec![(
                "title".to_string(),
                "The system shall boot, quickly".to_string()
            )]
        );
    }

    #[test]
    fn test_map_argument_splits_a_comma_separated_signature() {
        // Given — CPython's audit-event shape
        let argument = spec(&["name", "args", "version"], ArgumentSplit::Comma);

        // When
        let mapped = argument.map_argument("os.system, command, 3.8").unwrap();

        // Then
        assert_eq!(
            mapped,
            vec![
                ("name".to_string(), "os.system".to_string()),
                ("args".to_string(), "command".to_string()),
                ("version".to_string(), "3.8".to_string()),
            ]
        );
    }

    #[test]
    fn test_map_argument_leaves_trailing_fields_unset_when_parts_are_missing() {
        // Given — audit events in the wild often omit the version
        let argument = spec(&["name", "args", "version"], ArgumentSplit::Comma);

        // When
        let mapped = argument.map_argument("os.system, command").unwrap();

        // Then — the required-attribute check, not this one, decides if that is wrong
        assert_eq!(
            mapped,
            vec![
                ("name".to_string(), "os.system".to_string()),
                ("args".to_string(), "command".to_string()),
            ]
        );
    }

    #[test]
    fn test_map_argument_skips_an_empty_part_rather_than_storing_a_blank() {
        // Given
        let argument = spec(&["name", "args", "version"], ArgumentSplit::Comma);

        // When
        let mapped = argument.map_argument("os.system, , 3.8").unwrap();

        // Then
        assert_eq!(
            mapped,
            vec![
                ("name".to_string(), "os.system".to_string()),
                ("version".to_string(), "3.8".to_string()),
            ]
        );
    }

    #[test]
    fn test_map_argument_rejects_more_parts_than_declared_fields() {
        // Given
        let argument = spec(&["name", "version"], ArgumentSplit::Comma);

        // When
        let result = argument.map_argument("os.system, command, 3.8");

        // Then — dropping the extra silently is exactly what must not happen
        assert_eq!(
            result,
            Err(ArgumentError::TooManyParts {
                expected: 2,
                found: 3
            })
        );
    }

    #[test]
    fn test_map_argument_rejects_an_argument_a_type_does_not_take() {
        // Given
        let argument = ArgumentSpec::default();

        // When
        let result = argument.map_argument("unexpected title");

        // Then
        assert_eq!(
            result,
            Err(ArgumentError::Unexpected {
                text: "unexpected title".to_string()
            })
        );
    }

    #[test]
    fn test_map_argument_accepts_no_argument_for_a_type_that_takes_none() {
        // Given
        let argument = ArgumentSpec::default();

        // When
        let mapped = argument.map_argument("   ").unwrap();

        // Then
        assert!(mapped.is_empty());
    }

    #[test]
    fn test_map_argument_accepts_an_omitted_argument_for_a_type_that_takes_one() {
        // Given — the required check reports the missing title, not this code
        let argument = spec(&["title"], ArgumentSplit::Whole);

        // When
        let mapped = argument.map_argument("").unwrap();

        // Then
        assert!(mapped.is_empty());
    }

    #[test]
    fn test_takes_argument_reflects_whether_fields_were_declared() {
        // Given
        let none = ArgumentSpec::default();
        let some = spec(&["title"], ArgumentSplit::Whole);

        // When / Then
        assert!(!none.takes_argument());
        assert!(some.takes_argument());
    }

    #[test]
    fn test_argument_split_defaults_to_whole() {
        // Given / When
        let split = ArgumentSplit::default();

        // Then
        assert_eq!(split, ArgumentSplit::Whole);
    }

    #[test]
    fn test_argument_error_messages_describe_the_mismatch() {
        // Given
        let unexpected = ArgumentError::Unexpected {
            text: "title".to_string(),
        };
        let too_many = ArgumentError::TooManyParts {
            expected: 2,
            found: 3,
        };

        // When / Then
        assert!(unexpected.to_string().contains("takes no argument"));
        assert!(too_many.to_string().contains('3'));
        assert!(too_many.to_string().contains('2'));
    }
}
