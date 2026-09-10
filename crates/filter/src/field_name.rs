use std::fmt;

use serde::{Deserialize, Serialize};

/// The name of a value a filter reads from the thing it is filtering.
///
/// A field name is what appears on either side of a comparison (`status`,
/// `docname`), and it is the same type a listing directive's `:columns:` and
/// `:sort:` options hold — one type for the three, so a name that is legal in
/// a filter cannot be illegal as a column.
///
/// The invariant is the character set: a name starts with a letter or `_` and
/// continues with letters, digits, `_` or `-`. The hyphen is deliberate.
/// Python's own identifiers forbid it, but this grammar has no arithmetic at
/// all, so `-` can never be a subtraction here — and an entity schema may
/// legitimately declare an attribute called `safety-level`, which would
/// otherwise be unreachable from a filter.
///
/// Deserialization re-validates, following the "parse, don't validate" pattern
/// `rusty_sphinx_ast::HashedContent` sets: an [`Expr`](crate::Expr) is stored
/// in a `.ast` file, so a hand-edited one cannot smuggle in a name the parser
/// would have rejected.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct FieldName(String);

/// Why a string could not become a [`FieldName`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldNameError {
    /// The candidate was empty or contained only whitespace.
    Empty,
    /// The candidate began with a character that may not start a name.
    IllegalStart(char),
    /// The candidate contained a character outside the permitted set.
    IllegalCharacter(char),
}

impl fmt::Display for FieldNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "field name is empty"),
            Self::IllegalStart(c) => write!(
                f,
                "field name starts with {c:?}; it must start with a letter or '_'"
            ),
            Self::IllegalCharacter(c) => write!(
                f,
                "field name contains the illegal character {c:?}; only letters, digits, '_' and '-' are allowed"
            ),
        }
    }
}

impl std::error::Error for FieldNameError {}

impl FieldName {
    /// Creates a `FieldName`, rejecting anything outside the permitted set.
    ///
    /// # Errors
    ///
    /// Returns [`FieldNameError`] when `raw` is empty, starts with a character
    /// that may not open a name, or holds an illegal one.
    pub fn new(raw: &str) -> Result<Self, FieldNameError> {
        let trimmed = raw.trim();
        let mut chars = trimmed.chars();
        let Some(first) = chars.next() else {
            return Err(FieldNameError::Empty);
        };
        if !Self::is_legal_start(first) {
            return Err(FieldNameError::IllegalStart(first));
        }
        if let Some(illegal) = chars.find(|c| !Self::is_legal_char(*c)) {
            return Err(FieldNameError::IllegalCharacter(illegal));
        }
        Ok(Self(trimmed.to_string()))
    }

    /// Reports whether `c` may open a field name.
    fn is_legal_start(c: char) -> bool {
        c.is_alphabetic() || c == '_'
    }

    /// Reports whether `c` may appear after a field name's first character.
    fn is_legal_char(c: char) -> bool {
        c.is_alphanumeric() || matches!(c, '_' | '-')
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FieldName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for FieldName {
    type Error = FieldNameError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::new(&raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_name_accepts_a_plain_identifier() {
        // Given
        let raw = "status";

        // When
        let name = FieldName::new(raw);

        // Then
        assert_eq!(name.unwrap().as_str(), "status");
    }

    #[test]
    fn test_field_name_accepts_underscores_and_digits() {
        // Given — the derived back-link spelling a converted schema produces
        let raw = "_implements_back2";

        // When
        let name = FieldName::new(raw);

        // Then
        assert!(name.is_ok());
    }

    #[test]
    fn test_field_name_accepts_a_hyphen_after_the_first_character() {
        // Given — a schema may declare an attribute called `safety-level`
        let raw = "safety-level";

        // When
        let name = FieldName::new(raw);

        // Then
        assert_eq!(name.unwrap().as_str(), "safety-level");
    }

    #[test]
    fn test_field_name_trims_surrounding_whitespace() {
        // Given — `:columns: id, title` hands each name over with padding
        let raw = "  title  ";

        // When
        let name = FieldName::new(raw);

        // Then
        assert_eq!(name.unwrap().as_str(), "title");
    }

    #[test]
    fn test_field_name_rejects_an_empty_candidate() {
        // Given
        let raw = "   ";

        // When
        let name = FieldName::new(raw);

        // Then
        assert_eq!(name, Err(FieldNameError::Empty));
    }

    #[test]
    fn test_field_name_rejects_a_leading_digit() {
        // Given
        let raw = "2fast";

        // When
        let name = FieldName::new(raw);

        // Then
        assert_eq!(name, Err(FieldNameError::IllegalStart('2')));
    }

    #[test]
    fn test_field_name_rejects_a_leading_hyphen() {
        // Given — a hyphen is legal inside a name but may not open one
        let raw = "-status";

        // When
        let name = FieldName::new(raw);

        // Then
        assert_eq!(name, Err(FieldNameError::IllegalStart('-')));
    }

    #[test]
    fn test_field_name_rejects_an_embedded_space() {
        // Given
        let raw = "safety level";

        // When
        let name = FieldName::new(raw);

        // Then
        assert_eq!(name, Err(FieldNameError::IllegalCharacter(' ')));
    }

    #[test]
    fn test_field_name_rejects_a_dotted_path() {
        // Given — sphinx-needs has no attribute paths, and accepting one would
        // promise a lookup this crate does not perform
        let raw = "links.id";

        // When
        let name = FieldName::new(raw);

        // Then
        assert_eq!(name, Err(FieldNameError::IllegalCharacter('.')));
    }

    #[test]
    fn test_deserialization_revalidates_the_character_set() {
        // Given — a hand-edited `.ast` naming a field the parser would refuse
        let json = "\"safety level\"";

        // When
        let decoded: Result<FieldName, _> = serde_json::from_str(json);

        // Then
        assert!(decoded.is_err());
    }

    #[test]
    fn test_deserialization_accepts_a_name_the_constructor_accepts() {
        // Given
        let json = "\"implements_back\"";

        // When
        let decoded: FieldName = serde_json::from_str(json).unwrap();

        // Then
        assert_eq!(decoded.as_str(), "implements_back");
    }

    #[test]
    fn test_display_writes_the_name_as_written() {
        // Given
        let name = FieldName::new("docname").unwrap();

        // When
        let shown = name.to_string();

        // Then
        assert_eq!(shown, "docname");
    }

    #[test]
    fn test_errors_describe_what_is_wrong() {
        // Given
        let errors = [
            FieldNameError::Empty,
            FieldNameError::IllegalStart('2'),
            FieldNameError::IllegalCharacter(' '),
        ];

        // When
        let messages: Vec<String> = errors.iter().map(ToString::to_string).collect();

        // Then
        assert!(messages[0].contains("empty"));
        assert!(messages[1].contains("must start with"));
        assert!(messages[2].contains("illegal character"));
    }
}
