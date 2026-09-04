use rusty_sphinx_ast::{SectionId, Toctree};
use serde::{Deserialize, Serialize};

/// One document's heading hierarchy, as a `.. toctree::` needs to see it.
///
/// Sphinx's toctree lists the sections *inside* each document it references,
/// down to `:maxdepth:` — which is exactly what `:titlesonly:` switches off.
/// Rendering either option faithfully needs to know a referenced document's
/// headings while rendering a *different* document, so the outline has to
/// travel through the project index.
///
/// This is per-document data, built by `analyze()` and merged like
/// `document_titles`, which is what keeps the live-preview path correct: a
/// fresh local analysis merged onto a stale global index still yields a usable
/// outline. That is the reason sections are stored per document here and
/// expanded at render time, rather than being baked into a single
/// section-expanded navigation tree — such a tree would repeat every section
/// under every toctree that reaches it, and could not be merged per document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentOutline {
    /// The document's top-level sections, in document order.
    ///
    /// When a document's headings hang off a single level-1 heading, that
    /// heading is the *document title* rather than a section — it is already
    /// represented by the document's own entry in a toctree — so this holds
    /// its children instead of the heading itself.
    pub sections: Vec<OutlineSection>,
}

/// One section within a document, and the sections nested under it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineSection {
    /// The heading's plain text, with inline markup already flattened.
    pub title: String,
    /// The anchor the heading renders with, from the same
    /// `rusty_sphinx_ast::allocate_section_ids` call the renderer uses — so a
    /// link built from this can never miss the heading it points at.
    pub id: SectionId,
    pub children: Vec<Self>,
}

impl DocumentOutline {
    /// Whether the document has no sections at all, so a toctree entry for it
    /// can skip descending.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(title: &str, children: Vec<OutlineSection>) -> OutlineSection {
        OutlineSection {
            title: title.to_string(),
            id: SectionId::from_title(title),
            children,
        }
    }

    #[test]
    fn test_default_outline_is_empty() {
        // Given / When
        let outline = DocumentOutline::default();

        // Then
        assert!(outline.is_empty());
    }

    #[test]
    fn test_outline_with_a_section_is_not_empty() {
        // Given
        let outline = DocumentOutline {
            sections: vec![section("Overview", vec![])],
        };

        // When / Then
        assert!(!outline.is_empty());
    }

    #[test]
    fn test_outline_round_trips_through_json() {
        // Given — a nested outline, since the recursion is what serializes.
        let outline = DocumentOutline {
            sections: vec![
                section("Overview", vec![section("Details", vec![])]),
                section("Reference", vec![]),
            ],
        };

        // When
        let json = serde_json::to_string(&outline).expect("serializes");
        let restored: DocumentOutline = serde_json::from_str(&json).expect("deserializes");

        // Then
        assert_eq!(restored, outline);
    }

    #[test]
    fn test_outline_rejects_a_section_id_that_is_not_normalized() {
        // Given — `SectionId` re-validates on load, so a hand-edited index
        // fails loudly rather than emitting a link to a nonexistent anchor.
        let json = r#"{"sections":[{"title":"Overview","id":"Not A Slug","children":[]}]}"#;

        // When
        let restored: Result<DocumentOutline, _> = serde_json::from_str(json);

        // Then
        assert!(restored.is_err());
    }
}

/// One `.. toctree::` directive, with where in its document it was written.
///
/// Sphinx splices a toctree's entries into the document's own section tree at
/// the position of the directive, so a toctree written under "Advanced" lists
/// its documents *under* Advanced rather than beside it. `section` is what
/// lets the renderer reproduce that; `None` means the directive was written
/// before any section, which is where the overwhelmingly common
/// `index.rst`-with-a-toctree puts it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentToctree {
    pub toctree: Toctree,
    #[serde(default)]
    pub section: Option<SectionId>,
}

#[cfg(test)]
mod document_toctree_tests {
    use super::*;

    #[test]
    fn test_document_toctree_round_trips_through_json() {
        // Given
        let placed = DocumentToctree {
            toctree: Toctree::default(),
            section: Some(SectionId::from_title("Advanced")),
        };

        // When
        let json = serde_json::to_string(&placed).expect("serializes");
        let restored: DocumentToctree = serde_json::from_str(&json).expect("deserializes");

        // Then
        assert_eq!(restored, placed);
    }

    #[test]
    fn test_document_toctree_defaults_its_section_when_absent() {
        // Given — an index written before placement was recorded.
        let json = r#"{"toctree":{"entries":[]}}"#;

        // When
        let restored: DocumentToctree = serde_json::from_str(json).expect("deserializes");

        // Then
        assert_eq!(restored.section, None);
    }
}
