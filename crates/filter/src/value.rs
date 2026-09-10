use std::fmt;

use serde::{Deserialize, Serialize};

/// A constant written in a filter expression.
///
/// Deliberately narrow: a filter compares against text, whole numbers and the
/// two booleans, and nothing else. There is no `None` literal, because Python
/// spells that test `is None` and this grammar keeps that spelling — see
/// [`Expr::IsNone`](crate::Expr::IsNone).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Literal {
    Text(String),
    Int(i64),
    Bool(bool),
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(text) => f.write_str(text),
            Self::Int(number) => write!(f, "{number}"),
            Self::Bool(flag) => write!(f, "{}", if *flag { "True" } else { "False" }),
        }
    }
}

/// What a field is worth on the thing being filtered.
///
/// [`Missing`](FieldValue::Missing) is Python's `None`, which is what a
/// sphinx-needs filter means by `docname is not None`. It is a value rather
/// than an absence so that every operator has a defined answer for it, instead
/// of each one branching on an `Option`.
///
/// Not `Serialize`: this is what a [`FilterSubject`](crate::FilterSubject)
/// produces while evaluating, never anything stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    /// The subject has no such field, or it was never set.
    Missing,
    Text(String),
    Int(i64),
    Bool(bool),
    /// A list-valued field — an entity's `tags`, or a relation's targets.
    List(Vec<String>),
}

impl FieldValue {
    /// Whether this value equals `literal`.
    ///
    /// Types do not convert: the text `"3"` and the number `3` are different,
    /// as they are in Python. [`Missing`](FieldValue::Missing) equals nothing
    /// at all, which is what makes `status == "open"` false rather than an
    /// error on an entity whose type declares no `status`.
    #[must_use]
    pub fn equals(&self, literal: &Literal) -> bool {
        match (self, literal) {
            (Self::Text(value), Literal::Text(expected)) => value == expected,
            (Self::Int(value), Literal::Int(expected)) => value == expected,
            (Self::Bool(value), Literal::Bool(expected)) => value == expected,
            _ => false,
        }
    }

    /// This value as the text an `in` test compares, if it has one.
    ///
    /// A list has none: `"a" in tags` asks about the *items*, so a list is
    /// handled by [`contains`](Self::contains) rather than flattened to a
    /// string here. Missing has none either.
    #[must_use]
    pub fn as_text(&self) -> Option<String> {
        match self {
            Self::Text(text) => Some(text.clone()),
            Self::Int(number) => Some(number.to_string()),
            Self::Bool(flag) => Some(if *flag { "True" } else { "False" }.to_string()),
            Self::Missing | Self::List(_) => None,
        }
    }

    /// Whether `needle` is in this value.
    ///
    /// Substring for text, membership for a list — the two meanings Python's
    /// `in` already has, and both appear in real filters: `"safety" in docname`
    /// selects by path, `"api" in tags` selects by tag.
    #[must_use]
    pub fn contains(&self, needle: &str) -> bool {
        match self {
            Self::List(items) => items.iter().any(|item| item == needle),
            Self::Missing => false,
            other => other.as_text().is_some_and(|text| text.contains(needle)),
        }
    }

    /// Whether this value counts as true when written on its own.
    ///
    /// Python's own rule, so a bare `:filter: tags` selects the entities that
    /// have any.
    #[must_use]
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Missing => false,
            Self::Text(text) => !text.is_empty(),
            Self::Int(number) => *number != 0,
            Self::Bool(flag) => *flag,
            Self::List(items) => !items.is_empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equals_matches_text_of_the_same_value() {
        // Given
        let value = FieldValue::Text("open".to_string());

        // When
        let matched = value.equals(&Literal::Text("open".to_string()));

        // Then
        assert!(matched);
    }

    #[test]
    fn test_equals_rejects_a_different_text() {
        // Given
        let value = FieldValue::Text("open".to_string());

        // When
        let matched = value.equals(&Literal::Text("closed".to_string()));

        // Then
        assert!(!matched);
    }

    #[test]
    fn test_equals_matches_an_integer() {
        // Given
        let value = FieldValue::Int(3);

        // When
        let matched = value.equals(&Literal::Int(3));

        // Then
        assert!(matched);
    }

    #[test]
    fn test_equals_matches_a_boolean() {
        // Given
        let value = FieldValue::Bool(true);

        // When
        let matched = value.equals(&Literal::Bool(true));

        // Then
        assert!(matched);
    }

    #[test]
    fn test_equals_does_not_convert_between_types() {
        // Given — Python does not equate these either
        let value = FieldValue::Int(3);

        // When
        let matched = value.equals(&Literal::Text("3".to_string()));

        // Then
        assert!(!matched);
    }

    #[test]
    fn test_equals_is_false_for_a_missing_field() {
        // Given — a type that declares no `status` at all
        let value = FieldValue::Missing;

        // When
        let matched = value.equals(&Literal::Text("open".to_string()));

        // Then
        assert!(!matched);
    }

    #[test]
    fn test_equals_is_false_for_a_list() {
        // Given — Python compares a list to a string as unequal, not as an error
        let value = FieldValue::List(vec!["boot".to_string()]);

        // When
        let matched = value.equals(&Literal::Text("boot".to_string()));

        // Then
        assert!(!matched);
    }

    #[test]
    fn test_as_text_renders_the_scalar_kinds() {
        // Given
        let values = [
            FieldValue::Text("boot".to_string()),
            FieldValue::Int(7),
            FieldValue::Bool(false),
        ];

        // When
        let rendered: Vec<Option<String>> = values.iter().map(FieldValue::as_text).collect();

        // Then
        assert_eq!(
            rendered,
            [
                Some("boot".to_string()),
                Some("7".to_string()),
                Some("False".to_string())
            ]
        );
    }

    #[test]
    fn test_as_text_has_none_for_missing_and_lists() {
        // Given
        let values = [FieldValue::Missing, FieldValue::List(Vec::new())];

        // When
        let rendered: Vec<Option<String>> = values.iter().map(FieldValue::as_text).collect();

        // Then
        assert_eq!(rendered, [None, None]);
    }

    #[test]
    fn test_contains_is_substring_for_text() {
        // Given — the shape every corpus filter uses to select by path
        let value = FieldValue::Text("safety_example/analysis".to_string());

        // When
        let matched = value.contains("safety_example");

        // Then
        assert!(matched);
    }

    #[test]
    fn test_contains_is_membership_for_a_list() {
        // Given
        let value = FieldValue::List(vec!["boot".to_string(), "kernel".to_string()]);

        // When
        let matched = value.contains("boot");

        // Then
        assert!(matched);
    }

    #[test]
    fn test_contains_does_not_match_a_list_item_by_substring() {
        // Given — `"boo" in tags` asks whether `boo` is a tag, not whether one
        // starts with it
        let value = FieldValue::List(vec!["boot".to_string()]);

        // When
        let matched = value.contains("boo");

        // Then
        assert!(!matched);
    }

    #[test]
    fn test_contains_is_false_for_a_missing_field() {
        // Given
        let value = FieldValue::Missing;

        // When
        let matched = value.contains("anything");

        // Then
        assert!(!matched);
    }

    #[test]
    fn test_is_truthy_follows_pythons_rule() {
        // Given
        let cases = [
            (FieldValue::Missing, false),
            (FieldValue::Text(String::new()), false),
            (FieldValue::Text("x".to_string()), true),
            (FieldValue::Int(0), false),
            (FieldValue::Int(1), true),
            (FieldValue::Bool(false), false),
            (FieldValue::Bool(true), true),
            (FieldValue::List(Vec::new()), false),
            (FieldValue::List(vec!["a".to_string()]), true),
        ];

        // When
        let results: Vec<bool> = cases.iter().map(|(value, _)| value.is_truthy()).collect();

        // Then
        let expected: Vec<bool> = cases.iter().map(|(_, expected)| *expected).collect();
        assert_eq!(results, expected);
    }

    #[test]
    fn test_literal_display_uses_pythons_boolean_spelling() {
        // Given
        let literals = [
            Literal::Text("open".to_string()),
            Literal::Int(-2),
            Literal::Bool(true),
        ];

        // When
        let shown: Vec<String> = literals.iter().map(ToString::to_string).collect();

        // Then
        assert_eq!(shown, ["open", "-2", "True"]);
    }
}
