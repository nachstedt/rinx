//! Where clicking an image goes: its `:target:` option.
//!
//! Where its bytes come from is [`crate::AssetUri`], shared with the
//! `:download:` role.

use serde::{Deserialize, Serialize};

use crate::target_name::TargetName;

/// What an image's `:target:` points at.
///
/// docutils decides between the two with `parse_target`: a value ending in a
/// single `_` is an indirect reference to a named target, anything else is a
/// URI. The trailing marker is markup rather than part of the name, so it is
/// stripped here and the intent recorded as the variant instead.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImageTarget {
    /// A plain URI, used as the link's `href` verbatim.
    Uri(String),
    /// A named target elsewhere in the project, resolved against the index at
    /// render time. Normalized through [`TargetName`] — docutils'
    /// `fully_normalize_name` — so it matches a target however it was cased.
    Reference(TargetName),
}

impl ImageTarget {
    /// Reads a `:target:` value.
    #[must_use]
    pub fn new(raw: &str) -> Self {
        let trimmed = raw.trim();
        // A trailing `__` is an *anonymous* reference in reST. docutils'
        // `parse_target` does not accept one here, so it stays a URI rather
        // than silently consuming an anonymous target this directive never
        // declared.
        if trimmed.ends_with('_') && !trimmed.ends_with("__") && trimmed.len() > 1 {
            let name = trimmed.trim_end_matches('_');
            return Self::Reference(TargetName::new(name));
        }
        Self::Uri(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_new_reads_a_trailing_underscore_as_a_reference() {
        // Given
        let raw = "My Target_";

        // When
        let target = ImageTarget::new(raw);

        // Then
        assert_eq!(target, ImageTarget::Reference(TargetName::new("My Target")));
    }

    #[test]
    fn test_target_new_reads_a_url_as_a_uri() {
        // Given
        let raw = "https://example.com/";

        // When
        let target = ImageTarget::new(raw);

        // Then
        assert_eq!(target, ImageTarget::Uri("https://example.com/".to_string()));
    }

    #[test]
    fn test_target_new_reads_a_double_underscore_as_a_uri() {
        // Given — an anonymous reference, which docutils does not accept here
        let raw = "something__";

        // When
        let target = ImageTarget::new(raw);

        // Then
        assert_eq!(target, ImageTarget::Uri("something__".to_string()));
    }

    #[test]
    fn test_target_new_reads_a_bare_underscore_as_a_uri() {
        // Given — nothing is left once the marker is stripped
        let raw = "_";

        // When
        let target = ImageTarget::new(raw);

        // Then
        assert_eq!(target, ImageTarget::Uri("_".to_string()));
    }

    #[test]
    fn test_target_serialization_round_trips() {
        // Given
        let target = ImageTarget::new("some name_");

        // When
        let json = serde_json::to_string(&target).expect("should serialize");
        let restored: ImageTarget = serde_json::from_str(&json).expect("should deserialize");

        // Then
        assert_eq!(restored, target);
    }
}
