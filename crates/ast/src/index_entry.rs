use serde::{Deserialize, Serialize};

/// A single entry produced by a `.. index::` directive or an `:index:` role.
///
/// `pair:`/`triple:` entries are expanded into multiple [`Self::Term`]
/// entries at parse time (mirroring Sphinx's own behavior), so downstream
/// code only ever has to handle one linkable entry shape. `See`/`SeeAlso`
/// are kept as their own variants — despite the name, they are unrelated to
/// `Directive::SeeAlso` (the `.. seealso::` admonition box); these redirect
/// the reader to another entry rather than linking to content, so the
/// general index lists them without a link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexEntry {
    /// A directly linkable entry, optionally nested under `subentry`.
    Term {
        primary: String,
        subentry: Option<String>,
        main: bool,
    },
    /// `see: entry; target` — redirects the reader to `target` instead of
    /// linking to this location.
    See { entry: String, target: String },
    /// `seealso: entry; target` — same as `See`, but rendered as a
    /// supplementary "see also" reference rather than a replacement.
    SeeAlso { entry: String, target: String },
}

/// The five entry types an index entry may be written with, as
/// `single: …`, `pair: …` and so on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexEntryType {
    Single,
    Pair,
    Triple,
    See,
    SeeAlso,
}

impl IndexEntryType {
    /// Every entry type, in the order Sphinx lists them.
    pub const ALL: [Self; 5] = [
        Self::Single,
        Self::Pair,
        Self::Triple,
        Self::See,
        Self::SeeAlso,
    ];

    /// The keyword the type is written as, before its `:`.
    #[must_use]
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Pair => "pair",
            Self::Triple => "triple",
            Self::See => "see",
            Self::SeeAlso => "seealso",
        }
    }

    /// The type written as `keyword`, if it names one.
    #[must_use]
    pub fn from_keyword(keyword: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|ty| ty.keyword() == keyword)
    }
}

/// An index entry whose value its type cannot split into the parts it
/// needs — a `pair:` without two `;`-separated parts, an empty `single:`.
///
/// Shared by the `.. index::` directive and the `:index:` role, which write
/// entries in one grammar but report under their own diagnostic codes; it
/// lives here because a refused `:index:` role carries it in the AST until
/// the parser's refusal pass reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvalidIndexEntry {
    /// The type the entry was written with.
    pub entry_type: IndexEntryType,
    /// The value after the type keyword, as written.
    pub value: String,
}

impl std::fmt::Display for InvalidIndexEntry {
    /// Sphinx's wording: `invalid pair index entry 'a'`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid {} index entry '{}'",
            self.entry_type.keyword(),
            self.value
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_entry_type_from_keyword_inverts_keyword() {
        // Given / When / Then
        for ty in IndexEntryType::ALL {
            assert_eq!(IndexEntryType::from_keyword(ty.keyword()), Some(ty));
        }
    }

    #[test]
    fn test_index_entry_type_from_keyword_rejects_an_unknown_keyword() {
        // Given / When / Then
        assert_eq!(IndexEntryType::from_keyword("double"), None);
        assert_eq!(IndexEntryType::from_keyword("Single"), None);
    }

    #[test]
    fn test_invalid_index_entry_display_uses_sphinx_wording() {
        // Given
        let error = InvalidIndexEntry {
            entry_type: IndexEntryType::Pair,
            value: "loop".to_string(),
        };

        // When
        let message = error.to_string();

        // Then
        assert_eq!(message, "invalid pair index entry 'loop'");
    }

    #[test]
    fn test_index_entry_term_serialization_roundtrip() {
        // Given
        let entry = IndexEntry::Term {
            primary: "foo".to_string(),
            subentry: Some("bar".to_string()),
            main: true,
        };

        // When
        let json = serde_json::to_string(&entry).expect("Failed to serialize");
        let deserialized: IndexEntry = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(entry, deserialized);
    }

    #[test]
    fn test_index_entry_see_serialization_roundtrip() {
        // Given
        let entry = IndexEntry::See {
            entry: "foo".to_string(),
            target: "bar".to_string(),
        };

        // When
        let json = serde_json::to_string(&entry).expect("Failed to serialize");
        let deserialized: IndexEntry = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(entry, deserialized);
    }

    #[test]
    fn test_index_entry_seealso_serialization_roundtrip() {
        // Given
        let entry = IndexEntry::SeeAlso {
            entry: "foo".to_string(),
            target: "bar".to_string(),
        };

        // When
        let json = serde_json::to_string(&entry).expect("Failed to serialize");
        let deserialized: IndexEntry = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(entry, deserialized);
    }
}
