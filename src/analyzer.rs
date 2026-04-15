//! The analyzer module represents the global indexing phase.
//!
//! For simple single-file documents with basic structure, this is currently a pass-through
//! step that yields an empty `ProjectIndex`.

use crate::ast::Document;
use serde::{Deserialize, Serialize};

/// A global symbol table built from all documents in the project.
/// Extended with cross-reference data as the parser gains more RST features.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProjectIndex {}

impl ProjectIndex {
    /// Merge another `ProjectIndex` into this one.
    pub fn merge(&mut self, _other: ProjectIndex) {
        // No-op for now; will combine symbol tables when cross-refs are added.
    }
}

/// Analyzes a single `Document` and returns a local `ProjectIndex`.
/// Currently a no-op that returns an empty `ProjectIndex`.
#[must_use]
pub fn analyze(_doc: &Document) -> ProjectIndex {
    ProjectIndex::default()
}

/// Analyzes a collection of `Document`s and merges them into one `ProjectIndex`.
#[must_use]
pub fn analyze_many(docs: &[Document]) -> ProjectIndex {
    let mut index = ProjectIndex::default();
    for doc in docs {
        index.merge(analyze(doc));
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Node;

    #[test]
    fn test_analyze_returns_default_index_for_empty_document() {
        // Given
        let doc = Document::new(vec![]);

        // When
        let index = analyze(&doc);

        // Then
        let _ = format!("{index:?}"); // Ensures it doesn't panic
    }

    #[test]
    fn test_analyze_returns_default_index_for_populated_document() {
        // Given
        let doc = Document::new(vec![Node::Heading {
            level: 1,
            text: "Title".to_string(),
        }]);

        // When
        let index = analyze(&doc);

        // Then
        // Currently analyze does not populate anything, but it shouldn't panic
        let _ = format!("{index:?}");
    }

    #[test]
    fn test_analyze_many_returns_default_index_for_multiple_documents() {
        // Given
        let docs = vec![Document::new(vec![]), Document::new(vec![])];

        // When
        let index = analyze_many(&docs);

        // Then
        let _ = format!("{index:?}");
    }

    #[test]
    fn test_merge_combines_indices_without_error() {
        // Given
        let mut idx1 = ProjectIndex::default();
        let idx2 = ProjectIndex::default();

        // When
        idx1.merge(idx2);

        // Then
        // Since we don't have fields to assert equality on right now,
        // we just ensure the execution path is hit without issues.
        let _ = format!("{idx1:?}");
    }
}
