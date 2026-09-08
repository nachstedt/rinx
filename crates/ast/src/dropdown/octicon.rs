//! The name of the octicon a `.. dropdown::` shows beside its title.
//!
//! A name, not an icon: this type validates the *shape* of what was written
//! (one non-empty slug, normalized) and says nothing about whether an icon by
//! that name exists. That question belongs to the icon set, and the parser
//! answers it while reading the option — where the `:icon:` line itself can
//! be pointed at — reporting `dropdown.unknown-icon` for a name nothing
//! matches.
//!
//! Deliberately *not* enforced by this type, nor by its `Deserialize`. The
//! icon set is a dependency that changes when it is upgraded, and making a
//! name's validity part of the AST invariant would mean an `.ast` written by
//! one build could fail to load into the next — the same reason
//! [`crate::LanguageName`] holds any name and lets the backend decide, per
//! `docs/decisions/004-math-rendering.md`.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A normalized octicon name, e.g. `light-bulb`.
///
/// Opaque with a smart constructor because the name is a lookup key, for the
/// same reason [`crate::LanguageName`] is: a name written `Light-Bulb` and one
/// written `light-bulb` must not become two different icons downstream.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct OcticonName(String);

/// Why an octicon name could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidOcticonName {
    /// Nothing but whitespace was written.
    Empty,
    /// Several words were written where one slug was expected.
    NotASlug(String),
}

impl fmt::Display for InvalidOcticonName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("an octicon name cannot be empty"),
            Self::NotASlug(written) => {
                write!(formatter, "'{written}' is not a single octicon name")
            }
        }
    }
}

impl OcticonName {
    /// Creates a name from raw source text, trimming it and lowercasing it.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidOcticonName::Empty`] for a blank value, and
    /// [`InvalidOcticonName::NotASlug`] when the value holds internal
    /// whitespace — an octicon name is one slug, and collapsing two words into
    /// one would invent a name the author did not write.
    pub fn new(raw: &str) -> Result<Self, InvalidOcticonName> {
        let normalized = raw.trim().to_lowercase();
        if normalized.is_empty() {
            return Err(InvalidOcticonName::Empty);
        }
        if normalized.split_whitespace().count() > 1 {
            return Err(InvalidOcticonName::NotASlug(normalized));
        }
        Ok(Self(normalized))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OcticonName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for OcticonName {
    type Error = InvalidOcticonName;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::new(&raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_keeps_a_plain_slug() {
        // Given
        let raw = "light-bulb";

        // When
        let name = OcticonName::new(raw);

        // Then
        assert_eq!(name.map(|name| name.as_str().to_string()), Ok(raw.into()));
    }

    #[test]
    fn test_new_normalizes_case_and_surrounding_space() {
        // Given — docutils' own `choice` validator strips and lowercases
        let raw = "  Light-Bulb  ";

        // When
        let name = OcticonName::new(raw).expect("a slug with padding is valid");

        // Then
        assert_eq!(name.as_str(), "light-bulb");
    }

    #[test]
    fn test_new_rejects_a_blank_value() {
        // Given
        let raw = "   ";

        // When
        let name = OcticonName::new(raw);

        // Then
        assert_eq!(name, Err(InvalidOcticonName::Empty));
    }

    #[test]
    fn test_new_rejects_two_words() {
        // Given
        let raw = "light bulb";

        // When
        let name = OcticonName::new(raw);

        // Then — collapsing these would invent a name nobody wrote
        assert_eq!(
            name,
            Err(InvalidOcticonName::NotASlug("light bulb".to_string()))
        );
    }

    #[test]
    fn test_new_accepts_a_name_no_icon_set_defines() {
        // Given — whether an icon exists is the parser's question, not this
        // type's invariant, so an old `.ast` always reloads
        let raw = "not-an-octicon";

        // When
        let name = OcticonName::new(raw);

        // Then
        assert!(name.is_ok());
    }

    #[test]
    fn test_deserialize_revalidates_the_invariant() {
        // Given — an `.ast` file hand-edited to hold an unnormalized name
        let json = "\"Light Bulb\"";

        // When
        let name: Result<OcticonName, _> = serde_json::from_str(json);

        // Then
        assert!(name.is_err());
    }

    #[test]
    fn test_serializes_as_the_bare_name() {
        // Given
        let name = OcticonName::new("beaker").expect("a slug is valid");

        // When
        let json = serde_json::to_string(&name).expect("serializing a name cannot fail");

        // Then
        assert_eq!(json, "\"beaker\"");
    }

    #[test]
    fn test_display_writes_the_normalized_name() {
        // Given
        let name = OcticonName::new("  Beaker ").expect("a slug with padding is valid");

        // When
        let rendered = name.to_string();

        // Then
        assert_eq!(rendered, "beaker");
    }

    #[test]
    fn test_empty_message_names_the_problem() {
        // Given / When / Then
        assert_eq!(
            InvalidOcticonName::Empty.to_string(),
            "an octicon name cannot be empty"
        );
    }

    #[test]
    fn test_not_a_slug_message_quotes_what_was_written() {
        // Given
        let error = InvalidOcticonName::NotASlug("light bulb".to_string());

        // When
        let message = error.to_string();

        // Then
        assert_eq!(message, "'light bulb' is not a single octicon name");
    }
}
