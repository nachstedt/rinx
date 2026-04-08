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

    #[test]
    fn test_analyze_empty_document() {
        let doc = Document::new(vec![]);
        let index = analyze(&doc);
        // We just ensure it returns successfully as a stub for now.
        let _ = format!("{index:?}");
    }

    #[test]
    fn test_analyze_many() {
        let docs = vec![Document::new(vec![]), Document::new(vec![])];
        let index = analyze_many(&docs);
        let _ = format!("{index:?}");
    }
}
