//! The local name a project gives an inventory it links into.

use serde::{Deserialize, Serialize};

/// The name an inventory is declared under — `python` in
/// `` :external+python:ref:`tut` `` or `` :ref:`python:tut` `` — which is
/// Sphinx's `intersphinx_mapping` key.
///
/// Opaque, so every name can actually be written in both of those spellings:
/// non-empty, and free of whitespace, of `:` (which ends the name in both),
/// of `+` (which starts it in the role form) and of the backtick and angle
/// brackets a role's content is delimited by.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct InventoryName(String);

/// Why a string is not an [`InventoryName`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidInventoryName(pub String);

impl std::fmt::Display for InvalidInventoryName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "'{}' is not a valid inventory name: it must be non-empty and contain no \
             whitespace, ':', '+', '`', '<' or '>'",
            self.0
        )
    }
}

impl std::error::Error for InvalidInventoryName {}

impl InventoryName {
    /// Validates `raw` as an inventory name.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidInventoryName`] when `raw` is empty or holds a
    /// character that would end the name early in a role.
    pub fn new(raw: &str) -> Result<Self, InvalidInventoryName> {
        let forbidden = |c: char| c.is_whitespace() || matches!(c, ':' | '+' | '`' | '<' | '>');
        if raw.is_empty() || raw.chars().any(forbidden) {
            Err(InvalidInventoryName(raw.to_string()))
        } else {
            Ok(Self(raw.to_string()))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for InventoryName {
    type Error = InvalidInventoryName;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::new(&raw)
    }
}

impl From<InventoryName> for String {
    fn from(name: InventoryName) -> Self {
        name.0
    }
}

impl std::fmt::Display for InventoryName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_accepts_an_ordinary_name() {
        // Given / When
        let name = InventoryName::new("python-3.12").unwrap();

        // Then
        assert_eq!(name.as_str(), "python-3.12");
        assert_eq!(name.to_string(), "python-3.12");
    }

    #[test]
    fn test_new_refuses_an_empty_name() {
        // Given / When / Then
        assert_eq!(
            InventoryName::new(""),
            Err(InvalidInventoryName(String::new()))
        );
    }

    #[test]
    fn test_new_refuses_every_delimiter_a_role_uses() {
        // Given / When / Then
        for raw in ["a:b", "a+b", "a b", "a`b", "a<b", "a>b"] {
            assert!(InventoryName::new(raw).is_err(), "{raw} was accepted");
        }
    }

    #[test]
    fn test_deserialize_revalidates() {
        // Given
        let json = "\"py:thon\"";

        // When
        let result: Result<InventoryName, _> = serde_json::from_str(json);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_inventory_name_message_quotes_the_value() {
        // Given
        let error = InvalidInventoryName("a:b".to_string());

        // When
        let message = error.to_string();

        // Then
        assert!(message.contains("'a:b'"));
    }
}
