//! One document's contribution to the project index, kept apart from the
//! document itself.
//!
//! The project-wide phases of [`crate::build_project_index_from_analyses`]
//! never need a whole AST: they need what [`crate::analyze`] extracts, plus
//! two facts read off the document. Holding exactly that is what lets the
//! language server keep one of these per workspace document, re-analyse only
//! the document an edit touched, and fold the map again — so that a label
//! deleted in the editor really is gone, which a merge into a stale index
//! cannot do (ADR-038 §3).

use rinx_ast::Document;
use rinx_index::ProjectIndex;

use super::document_index::analyze;

/// The field a document starts with to say no toctree has to reach it.
const ORPHAN_FIELD: &str = "orphan";

/// What one document contributes to the project index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentAnalysis {
    /// The document's own index fragment, as [`analyze`] builds it.
    pub(crate) index: ProjectIndex,
    /// Whether the document opens with `:orphan:`.
    pub(crate) orphan: bool,
    /// The fingerprint of the entity schema the document was parsed against.
    pub(crate) entity_schema_hash: Option<String>,
}

impl DocumentAnalysis {
    /// The analysis of `doc`.
    #[must_use]
    pub fn of(doc: &Document) -> Self {
        Self {
            index: analyze(doc),
            orphan: doc.metadata.contains_key(ORPHAN_FIELD),
            entity_schema_hash: doc.entity_schema_hash.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{InlineNode, Node};

    #[test]
    fn test_of_carries_the_index_fragment_and_the_orphan_flag() {
        // Given
        let mut doc = Document::new(
            "notes.rst".to_string(),
            vec![Node::Heading {
                level: 1,
                text: vec![InlineNode::Text("Notes".to_string())],
            }],
        );
        doc.metadata.insert(ORPHAN_FIELD.to_string(), String::new());

        // When
        let analysis = DocumentAnalysis::of(&doc);

        // Then
        assert!(analysis.orphan);
        assert_eq!(analysis.index, analyze(&doc));
        assert_eq!(analysis.entity_schema_hash, None);
    }

    #[test]
    fn test_of_is_not_an_orphan_without_the_field() {
        // Given
        let doc = Document::new("index.rst".to_string(), Vec::new());

        // When / Then
        assert!(!DocumentAnalysis::of(&doc).orphan);
    }
}
