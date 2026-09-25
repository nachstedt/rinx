//! The `domain:role` pair every inventory entry is filed under.

use serde::{Deserialize, Serialize};

/// An inventory entry's type, e.g. `py:class` or `std:label`.
///
/// Opaque, so an entry can never carry a type Sphinx's reader would skip: a
/// value without a colon, with an empty half, or with whitespace (which would
/// shift every later field of the line it is written on). Only the *first*
/// colon separates the two halves, as in Sphinx — a role may itself contain
/// one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EntryType(String);

/// Why a string is not an [`EntryType`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidEntryType(pub String);

impl std::fmt::Display for InvalidEntryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "'{}' is not a `domain:role` inventory type: it needs a colon between two \
             non-empty halves and no whitespace",
            self.0
        )
    }
}

impl std::error::Error for InvalidEntryType {}

impl EntryType {
    /// Validates `raw` as a `domain:role` pair.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidEntryType`] when `raw` has no colon, an empty domain
    /// or role, or any whitespace.
    pub fn new(raw: &str) -> Result<Self, InvalidEntryType> {
        let valid = !raw.chars().any(char::is_whitespace)
            && raw
                .split_once(':')
                .is_some_and(|(domain, role)| !domain.is_empty() && !role.is_empty());
        if valid {
            Ok(Self(raw.to_string()))
        } else {
            Err(InvalidEntryType(raw.to_string()))
        }
    }

    /// The domain half, e.g. `py`.
    #[must_use]
    pub fn domain(&self) -> &str {
        self.0.split_once(':').map_or("", |(domain, _)| domain)
    }

    /// The role half, e.g. `class`.
    #[must_use]
    pub fn role(&self) -> &str {
        self.0.split_once(':').map_or("", |(_, role)| role)
    }

    /// The whole `domain:role` pair as written in the file.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for EntryType {
    type Error = InvalidEntryType;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::new(&raw)
    }
}

impl From<EntryType> for String {
    fn from(entry_type: EntryType) -> Self {
        entry_type.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_splits_domain_and_role_at_the_first_colon() {
        // Given
        let raw = "cpp:function:param";

        // When
        let entry_type = EntryType::new(raw).unwrap();

        // Then
        assert_eq!(entry_type.domain(), "cpp");
        assert_eq!(entry_type.role(), "function:param");
        assert_eq!(entry_type.as_str(), raw);
    }

    #[test]
    fn test_new_refuses_a_value_without_a_colon() {
        // Given / When
        let result = EntryType::new("label");

        // Then
        assert_eq!(result, Err(InvalidEntryType("label".to_string())));
    }

    #[test]
    fn test_new_refuses_an_empty_half() {
        // Given / When / Then
        assert!(EntryType::new(":label").is_err());
        assert!(EntryType::new("std:").is_err());
    }

    #[test]
    fn test_new_refuses_whitespace() {
        // Given / When / Then
        assert!(EntryType::new("std :label").is_err());
    }

    #[test]
    fn test_deserialize_revalidates() {
        // Given
        let json = "\"nocolon\"";

        // When
        let result: Result<EntryType, _> = serde_json::from_str(json);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_serialization_roundtrip() {
        // Given
        let entry_type = EntryType::new("py:class").unwrap();

        // When
        let json = serde_json::to_string(&entry_type).unwrap();
        let back: EntryType = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(json, "\"py:class\"");
        assert_eq!(back, entry_type);
    }

    #[test]
    fn test_invalid_entry_type_message_quotes_the_value() {
        // Given
        let error = InvalidEntryType("label".to_string());

        // When
        let message = error.to_string();

        // Then
        assert!(message.contains("'label'"));
    }
}
