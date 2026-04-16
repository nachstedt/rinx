//! The analyzer module represents the global indexing phase.
//!
//! For simple single-file documents with basic structure, this is currently a pass-through
//! step that yields an empty `ProjectIndex`.

use crate::ast::Document;
use serde::{Deserialize, Serialize};

use std::collections::HashMap;

/// A global symbol table built from all documents in the project.
/// Extended with cross-reference data as the parser gains more RST features.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProjectIndex {
    /// Maps target names to document paths.
    pub targets: HashMap<String, String>,
    /// Maps document paths to their top-level title.
    pub document_titles: HashMap<String, String>,
}

impl ProjectIndex {
    /// Merge another `ProjectIndex` into this one.
    pub fn merge(&mut self, other: ProjectIndex) {
        self.targets.extend(other.targets);
        self.document_titles.extend(other.document_titles);
    }
}

/// Analyzes a single `Document` and returns a local `ProjectIndex`.
#[must_use]
pub fn analyze(doc: &Document) -> ProjectIndex {
    let mut index = ProjectIndex::default();
    let mut found_title = false;
    for node in &doc.nodes {
        if let crate::ast::Node::Target(name) = node {
            index.targets.insert(name.clone(), doc.path.clone());
        }
        if !found_title && let crate::ast::Node::Heading { level: 1, text } = node {
            index.document_titles.insert(doc.path.clone(), text.clone());
            found_title = true;
        }
    }
    index
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
        let doc = Document::new("test.rst".to_string(), vec![]);

        // When
        let index = analyze(&doc);

        // Then
        let _ = format!("{index:?}"); // Ensures it doesn't panic
    }

    #[test]
    fn test_analyze_returns_default_index_for_populated_document() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Heading {
                level: 1,
                text: "Title".to_string(),
            }],
        );

        // When
        let index = analyze(&doc);

        // Then
        // Currently analyze does not populate anything, but it shouldn't panic
        let _ = format!("{index:?}");
    }

    #[test]
    fn test_analyze_many_returns_default_index_for_multiple_documents() {
        // Given
        let docs = vec![
            Document::new("test1.rst".to_string(), vec![]),
            Document::new("test2.rst".to_string(), vec![]),
        ];

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
    #[test]
    fn test_analyze_populates_targets_for_target_nodes() {
        // Given
        let doc = Document::new(
            "docs/my-file.rst".to_string(),
            vec![
                Node::Target("section-1".to_string()),
                Node::Paragraph(vec![crate::ast::InlineNode::Text("some text".to_string())]),
            ],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.targets.len(), 1);
        assert_eq!(index.targets.get("section-1").unwrap(), "docs/my-file.rst");
    }

    #[test]
    fn test_analyze_extracts_h1_title() {
        // Given
        let doc = Document::new(
            "docs/my-file.rst".to_string(),
            vec![
                Node::Paragraph(vec![crate::ast::InlineNode::Text("some text".to_string())]),
                Node::Heading {
                    level: 1,
                    text: "My Title".to_string(),
                },
                Node::Heading {
                    level: 1,
                    text: "Ignored Second H1".to_string(),
                },
            ],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.document_titles.len(), 1);
        assert_eq!(
            index.document_titles.get("docs/my-file.rst").unwrap(),
            "My Title"
        );
    }
}
