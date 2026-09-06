use std::fmt;

use serde::{Deserialize, Serialize};

/// The identifier an entity is referenced by.
///
/// Ids are compared and stored exactly as written — unlike
/// [`crate::TargetName`], which lowercases and collapses whitespace. An author
/// writing `REQ_001` sees `REQ_001` in every rendered link and in every
/// diagnostic, and `req_001` is a different entity. That matches sphinx-needs,
/// whose ids are case-sensitive, and it keeps generated ids (which may embed a
/// hash) from being silently folded together.
///
/// The invariant is the character set: an id must be non-empty and contain
/// only ASCII alphanumerics, `_`, `-`, `.` and `:`. Everything else — spaces
/// above all — is rejected at construction rather than producing a link that
/// silently never resolves. Deserialization re-validates, following the
/// "parse, don't validate" pattern [`crate::HashedContent`] sets.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct EntityId(String);

/// Why a string could not become an [`EntityId`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntityIdError {
    /// The candidate was empty or contained only whitespace.
    Empty,
    /// The candidate contained a character outside the permitted set.
    IllegalCharacter(char),
}

impl fmt::Display for EntityIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "entity id is empty"),
            Self::IllegalCharacter(c) => write!(
                f,
                "entity id contains the illegal character {c:?}; only letters, digits, '_', '-', '.' and ':' are allowed"
            ),
        }
    }
}

impl std::error::Error for EntityIdError {}

impl EntityId {
    /// Creates an `EntityId`, rejecting anything outside the permitted set.
    ///
    /// # Errors
    ///
    /// Returns [`EntityIdError`] when `raw` is empty or holds an illegal
    /// character.
    pub fn new(raw: &str) -> Result<Self, EntityIdError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(EntityIdError::Empty);
        }
        if let Some(illegal) = trimmed.chars().find(|c| !Self::is_legal_char(*c)) {
            return Err(EntityIdError::IllegalCharacter(illegal));
        }
        Ok(Self(trimmed.to_string()))
    }

    /// Rewrites `raw` so every character an id forbids becomes `_`.
    ///
    /// For the *derived* and *generated* id forms only, which routinely build
    /// on text that was never meant to be an identifier — a requirement's
    /// title, say. An id the author wrote is never put through this: silently
    /// repairing it would leave their `:id:` and their `:links:` naming
    /// different things, so an illegal one is reported instead.
    #[must_use]
    pub fn sanitize_fragment(raw: &str) -> String {
        raw.trim()
            .chars()
            .map(|c| if Self::is_legal_char(c) { c } else { '_' })
            .collect()
    }

    /// Reports whether `c` may appear in an id.
    fn is_legal_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for EntityId {
    type Error = EntityIdError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::new(&raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_id_accepts_the_permitted_character_set() {
        // Given
        let raw = "REQ_001-a.b:c";

        // When
        let id = EntityId::new(raw);

        // Then
        assert_eq!(id.unwrap().as_str(), "REQ_001-a.b:c");
    }

    #[test]
    fn test_entity_id_preserves_case() {
        // Given
        let upper = EntityId::new("REQ_001").unwrap();

        // When
        let lower = EntityId::new("req_001").unwrap();

        // Then — case is meaningful, so these are different entities
        assert_eq!(upper.as_str(), "REQ_001");
        assert_ne!(upper, lower);
    }

    #[test]
    fn test_entity_id_trims_surrounding_whitespace() {
        // Given — an option value arrives with the separator's padding
        let raw = "  REQ_001  ";

        // When
        let id = EntityId::new(raw).unwrap();

        // Then
        assert_eq!(id.as_str(), "REQ_001");
    }

    #[test]
    fn test_entity_id_rejects_an_empty_candidate() {
        // Given
        let raw = "   ";

        // When
        let result = EntityId::new(raw);

        // Then
        assert_eq!(result, Err(EntityIdError::Empty));
    }

    #[test]
    fn test_entity_id_rejects_an_internal_space() {
        // Given — the mistake a comma-separated list without commas produces
        let raw = "REQ_001 REQ_002";

        // When
        let result = EntityId::new(raw);

        // Then
        assert_eq!(result, Err(EntityIdError::IllegalCharacter(' ')));
    }

    #[test]
    fn test_entity_id_rejects_punctuation_outside_the_set() {
        // Given
        let raw = "REQ/001";

        // When
        let result = EntityId::new(raw);

        // Then
        assert_eq!(result, Err(EntityIdError::IllegalCharacter('/')));
    }

    #[test]
    fn test_entity_id_deserialization_revalidates_the_invariant() {
        // Given — a serialized id that was never built through new()
        let json = "\"REQ 001\"";

        // When
        let result: Result<EntityId, _> = serde_json::from_str(json);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_entity_id_survives_a_serialization_round_trip() {
        // Given
        let original = EntityId::new("REQ_001").unwrap();

        // When
        let json = serde_json::to_string(&original).unwrap();
        let restored: EntityId = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(original, restored);
    }

    #[test]
    fn test_entity_id_displays_as_written() {
        // Given
        let id = EntityId::new("REQ_001").unwrap();

        // When
        let shown = id.to_string();

        // Then
        assert_eq!(shown, "REQ_001");
    }

    #[test]
    fn test_sanitize_fragment_replaces_every_illegal_character() {
        // Given
        let raw = "  The system shall boot/now  ";

        // When
        let cleaned = EntityId::sanitize_fragment(raw);

        // Then
        assert_eq!(cleaned, "The_system_shall_boot_now");
    }

    #[test]
    fn test_sanitize_fragment_leaves_a_legal_fragment_alone() {
        // Given
        let raw = "os.system";

        // When
        let cleaned = EntityId::sanitize_fragment(raw);

        // Then
        assert_eq!(cleaned, "os.system");
    }

    #[test]
    fn test_entity_id_error_messages_name_the_problem() {
        // Given
        let empty = EntityIdError::Empty;
        let illegal = EntityIdError::IllegalCharacter('/');

        // When
        let empty_message = empty.to_string();
        let illegal_message = illegal.to_string();

        // Then
        assert_eq!(empty_message, "entity id is empty");
        assert!(illegal_message.contains('/'));
    }

    #[test]
    fn test_is_legal_char_accepts_the_set_and_rejects_its_neighbours() {
        // Given
        let legal = ['a', 'Z', '0', '_', '-', '.', ':'];
        let illegal = [' ', '/', '#', '\t', 'ä'];

        // When / Then
        for c in legal {
            assert!(EntityId::is_legal_char(c), "expected {c:?} to be legal");
        }
        for c in illegal {
            assert!(!EntityId::is_legal_char(c), "expected {c:?} to be illegal");
        }
    }
}
