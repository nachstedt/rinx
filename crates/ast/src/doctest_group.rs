use serde::{Deserialize, Serialize};

/// The name of a doctest group.
///
/// Doctest groups partition a document's test blocks into independent units:
/// every block in a group shares one Python namespace, and the group's
/// `testsetup`/`testcleanup` bracket its tests. Groups are scoped to a single
/// document — there is deliberately no cross-document group, since that is what
/// keeps a document's tests independently cacheable.
///
/// A directive with no argument belongs to the group named `default`, so this
/// type has no "absent" state: [`Self::new`] substitutes the default rather
/// than yielding an empty name, and the invariant `name == name.trim()` and
/// `!name.is_empty()` is re-validated on deserialization.
///
/// `*` is *not* a group name — it is a selector meaning "every group", modeled
/// by [`crate::DocTestGroupSelector`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct DocTestGroup(String);

impl DocTestGroup {
    /// The group a directive belongs to when it carries no argument.
    pub const DEFAULT_NAME: &'static str = "default";

    /// Creates a group name from a directive argument token, trimming
    /// surrounding whitespace and substituting [`Self::DEFAULT_NAME`] when the
    /// token is empty.
    #[must_use]
    pub fn new(raw: &str) -> Self {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            Self::default()
        } else {
            Self(trimmed.to_string())
        }
    }

    /// The group's name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is the implicit `default` group.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.0 == Self::DEFAULT_NAME
    }
}

impl Default for DocTestGroup {
    fn default() -> Self {
        Self(Self::DEFAULT_NAME.to_string())
    }
}

impl TryFrom<String> for DocTestGroup {
    type Error = String;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        if raw.is_empty() {
            return Err("doctest group name must not be empty".to_string());
        }
        if raw.trim() != raw {
            return Err(format!(
                "doctest group name must not carry surrounding whitespace, got {raw:?}"
            ));
        }
        Ok(Self(raw))
    }
}

/// Which group(s) a doctest directive's argument selects.
///
/// Sphinx resolves `*` generically, at group-assignment time, for *every* one of
/// the five directives — not only `testsetup`/`testcleanup` — so this type
/// belongs on all of them.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DocTestGroupSelector {
    /// `*` — every group in the document.
    ///
    /// Note the distribution rule this implies: `*` blocks are handed out to
    /// the groups that already exist, so a `*` block reaches no group at all in
    /// a document that contains nothing else. Reproducing that matters, or
    /// setup code would run that Sphinx never runs.
    AllGroups,
    /// A single named group.
    Named(DocTestGroup),
}

impl DocTestGroupSelector {
    /// Creates a selector from one directive-argument token.
    ///
    /// The `*` sigil is markup rather than part of a name, so it is resolved
    /// here into typed intent instead of being left in the string for a later
    /// phase to re-interpret.
    #[must_use]
    pub fn new(raw: &str) -> Self {
        if raw.trim() == "*" {
            Self::AllGroups
        } else {
            Self::Named(DocTestGroup::new(raw))
        }
    }

    /// The group named by this selector, or `None` for [`Self::AllGroups`].
    #[must_use]
    pub fn named(&self) -> Option<&DocTestGroup> {
        match self {
            Self::AllGroups => None,
            Self::Named(group) => Some(group),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_keeps_an_explicit_group_name() {
        // Given
        let raw = "parsing";

        // When
        let group = DocTestGroup::new(raw);

        // Then
        assert_eq!(group.as_str(), "parsing");
    }

    #[test]
    fn test_new_trims_surrounding_whitespace() {
        // Given — comma-splitting a directive argument leaves padding behind.
        let raw = "  parsing  ";

        // When
        let group = DocTestGroup::new(raw);

        // Then
        assert_eq!(group.as_str(), "parsing");
    }

    #[test]
    fn test_new_substitutes_the_default_group_for_an_empty_argument() {
        // Given
        let raw = "";

        // When
        let group = DocTestGroup::new(raw);

        // Then
        assert_eq!(group.as_str(), DocTestGroup::DEFAULT_NAME);
        assert!(group.is_default());
    }

    #[test]
    fn test_new_substitutes_the_default_group_for_a_whitespace_only_argument() {
        // Given
        let raw = "   ";

        // When
        let group = DocTestGroup::new(raw);

        // Then
        assert!(group.is_default());
    }

    #[test]
    fn test_is_default_is_false_for_a_named_group() {
        // Given
        let group = DocTestGroup::new("parsing");

        // When
        let is_default = group.is_default();

        // Then
        assert!(!is_default);
    }

    #[test]
    fn test_default_impl_matches_the_empty_argument_construction() {
        // Given / When
        let from_empty = DocTestGroup::new("");
        let from_default = DocTestGroup::default();

        // Then
        assert_eq!(from_empty, from_default);
    }

    #[test]
    fn test_serialization_roundtrip() {
        // Given
        let group = DocTestGroup::new("parsing");

        // When
        let json = serde_json::to_string(&group).expect("Failed to serialize");
        let deserialized: DocTestGroup =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(group, deserialized);
    }

    #[test]
    fn test_serializes_transparently_as_a_string() {
        // Given
        let group = DocTestGroup::new("parsing");

        // When
        let json = serde_json::to_string(&group).expect("Failed to serialize");

        // Then
        assert_eq!(json, "\"parsing\"");
    }

    #[test]
    fn test_deserialization_rejects_an_empty_name() {
        // Given — a hand-edited or stale `.ast` file.
        let json = "\"\"";

        // When
        let result: Result<DocTestGroup, _> = serde_json::from_str(json);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialization_rejects_an_untrimmed_name() {
        // Given
        let json = "\" parsing \"";

        // When
        let result: Result<DocTestGroup, _> = serde_json::from_str(json);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_selector_new_resolves_the_star_sigil_to_all_groups() {
        // Given
        let raw = "*";

        // When
        let selector = DocTestGroupSelector::new(raw);

        // Then
        assert_eq!(selector, DocTestGroupSelector::AllGroups);
    }

    #[test]
    fn test_selector_new_resolves_a_padded_star_to_all_groups() {
        // Given
        let raw = "  *  ";

        // When
        let selector = DocTestGroupSelector::new(raw);

        // Then
        assert_eq!(selector, DocTestGroupSelector::AllGroups);
    }

    #[test]
    fn test_selector_new_resolves_a_name_to_a_named_group() {
        // Given
        let raw = "parsing";

        // When
        let selector = DocTestGroupSelector::new(raw);

        // Then
        assert_eq!(
            selector,
            DocTestGroupSelector::Named(DocTestGroup::new("parsing"))
        );
    }

    #[test]
    fn test_selector_new_resolves_an_empty_argument_to_the_default_group() {
        // Given
        let raw = "";

        // When
        let selector = DocTestGroupSelector::new(raw);

        // Then
        assert_eq!(
            selector,
            DocTestGroupSelector::Named(DocTestGroup::default())
        );
    }

    #[test]
    fn test_selector_named_returns_the_group_for_a_named_selector() {
        // Given
        let selector = DocTestGroupSelector::new("parsing");

        // When
        let named = selector.named();

        // Then
        assert_eq!(named, Some(&DocTestGroup::new("parsing")));
    }

    #[test]
    fn test_selector_named_returns_none_for_all_groups() {
        // Given
        let selector = DocTestGroupSelector::AllGroups;

        // When
        let named = selector.named();

        // Then
        assert_eq!(named, None);
    }

    #[test]
    fn test_selector_serialization_roundtrip_for_all_groups() {
        // Given
        let selector = DocTestGroupSelector::AllGroups;

        // When
        let json = serde_json::to_string(&selector).expect("Failed to serialize");
        let deserialized: DocTestGroupSelector =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(selector, deserialized);
    }

    #[test]
    fn test_selector_serialization_roundtrip_for_a_named_group() {
        // Given
        let selector = DocTestGroupSelector::new("parsing");

        // When
        let json = serde_json::to_string(&selector).expect("Failed to serialize");
        let deserialized: DocTestGroupSelector =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(selector, deserialized);
    }
}
