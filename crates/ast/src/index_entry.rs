use serde::{Deserialize, Serialize};

/// A single entry produced by a `.. index::` directive.
///
/// `pair:`/`triple:` entries are expanded into multiple [`Self::Term`]
/// entries at parse time (mirroring Sphinx's own behavior), so downstream
/// code only ever has to handle one linkable entry shape. `See`/`SeeAlso`
/// are kept as their own variants — despite the name, they are unrelated to
/// `Directive::SeeAlso` (the `.. seealso::` admonition box); these redirect
/// the reader to another entry rather than linking to content, so they are
/// not resolved into `genindex_entries` yet (see `docs/dev/spec_gaps.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexEntry {
    /// A directly linkable entry, optionally nested under `subentry`.
    Term {
        primary: String,
        subentry: Option<String>,
        main: bool,
    },
    /// `see: entry <target>` — redirects the reader to `target` instead of
    /// linking to this location.
    See { entry: String, target: String },
    /// `seealso: entry <target>` — same as `See`, but rendered as a
    /// supplementary "see also" reference rather than a replacement.
    SeeAlso { entry: String, target: String },
}

#[cfg(test)]
mod tests {
    use super::*;

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
