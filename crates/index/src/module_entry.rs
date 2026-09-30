use serde::{Deserialize, Serialize};

/// What the index remembers about one documented Python module, for the two
/// places Sphinx shows it: the Python Module Index and the tooltip of a
/// `:mod:` link (ADR-032). Sphinx's `ModuleEntry`, minus the anchor, which
/// follows from the name.
///
/// Every value is kept as written — Sphinx never inline-parses a synopsis —
/// so a later phase only has to escape it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleEntry {
    /// The document whose `.. py:module::` defines the module.
    pub doc_path: String,
    /// The `:synopsis:`, if one was written.
    #[serde(default)]
    pub synopsis: Option<String>,
    /// The `:platform:`, if one was written.
    #[serde(default)]
    pub platform: Option<String>,
    /// Whether `:deprecated:` was written.
    #[serde(default)]
    pub deprecated: bool,
}

impl ModuleEntry {
    /// An entry for a module defined in `doc_path`, with nothing else known
    /// about it yet.
    #[must_use]
    pub fn new(doc_path: impl Into<String>) -> Self {
        Self {
            doc_path: doc_path.into(),
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_records_only_the_document() {
        // Given / When
        let entry = ModuleEntry::new("library/abc.rst");

        // Then
        assert_eq!(entry.doc_path, "library/abc.rst");
        assert_eq!(entry.synopsis, None);
        assert_eq!(entry.platform, None);
        assert!(!entry.deprecated);
    }

    #[test]
    fn test_an_entry_written_without_its_optional_fields_loads() {
        // Given — only the document, as for a module with no options.
        let json = r#"{"doc_path":"library/abc.rst"}"#;

        // When
        let entry: ModuleEntry = serde_json::from_str(json).expect("loads");

        // Then
        assert_eq!(entry, ModuleEntry::new("library/abc.rst"));
    }
}
