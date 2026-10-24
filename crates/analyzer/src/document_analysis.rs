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
//!
//! It also holds where each of the document's definitions was written, which
//! the index itself must not (`docs/decisions/045-definition-spans.md`): the
//! language server keeps no closed document's AST, so this is the one place
//! a definition's line survives for it.

use rinx_ast::Document;
use rinx_index::{DefinitionSites, ProjectIndex};

use super::document_index::analyze_with_sites;

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
    /// Where each definition `index` names was written. Read by no global
    /// phase, so no fold can make a line part of the project's index.
    sites: DefinitionSites,
}

impl DocumentAnalysis {
    /// The analysis of `doc`.
    #[must_use]
    pub fn of(doc: &Document) -> Self {
        let (index, sites) = analyze_with_sites(doc);
        Self {
            index,
            orphan: doc.metadata.contains_key(ORPHAN_FIELD),
            entity_schema_hash: doc.entity_schema_hash.clone(),
            sites,
        }
    }

    /// Where each definition of the document was written, by what the index
    /// names it.
    #[must_use]
    pub fn definition_sites(&self) -> &DefinitionSites {
        &self.sites
    }

    /// Whether `other` contributes to a fold exactly what this does — equal
    /// but, perhaps, for where its definitions were written, which no fold
    /// reads. A document whose lines only moved folds into the same index.
    #[must_use]
    pub fn folds_like(&self, other: &Self) -> bool {
        self.index == other.index
            && self.orphan == other.orphan
            && self.entity_schema_hash == other.entity_schema_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze;
    use rinx_ast::{InlineNode, Node, Position, SectionId, Span};
    use rinx_index::DefinitionKey;

    #[test]
    fn test_of_carries_the_index_fragment_and_the_orphan_flag() {
        // Given
        let mut doc = Document::new(
            "notes.rst".to_string(),
            vec![Node::Heading {
                level: 1,
                text: vec![InlineNode::Text("Notes".to_string())],
                span: None,
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
    fn test_of_carries_where_each_definition_was_written() {
        // Given
        let doc = Document::new(
            "notes.rst".to_string(),
            vec![Node::Heading {
                level: 1,
                text: vec![InlineNode::Text("Notes".to_string())],
                span: Some(Span::new(Position::new(1, 1), Position::new(1, 6))),
            }],
        );

        // When
        let analysis = DocumentAnalysis::of(&doc);

        // Then
        let site = analysis
            .definition_sites()
            .get(&DefinitionKey::Section(SectionId::from_title("Notes")));
        assert_eq!(site.map(|site| site.start), Some(Position::new(1, 1)));
    }

    fn noted(line: u32) -> Document {
        Document::new(
            "notes.rst".to_string(),
            vec![Node::Heading {
                level: 1,
                text: vec![InlineNode::Text("Notes".to_string())],
                span: Some(Span::new(Position::new(line, 1), Position::new(line, 6))),
            }],
        )
    }

    #[test]
    fn test_folds_like_ignores_where_the_definitions_were_written() {
        // Given
        let (original, moved) = (
            DocumentAnalysis::of(&noted(1)),
            DocumentAnalysis::of(&noted(4)),
        );

        // When / Then
        assert_ne!(original, moved);
        assert!(original.folds_like(&moved));
    }

    #[test]
    fn test_folds_like_tells_apart_what_a_fold_reads() {
        // Given
        let mut orphan = noted(1);
        orphan
            .metadata
            .insert(ORPHAN_FIELD.to_string(), String::new());
        let retitled = Document::new(
            "notes.rst".to_string(),
            vec![Node::Heading {
                level: 1,
                text: vec![InlineNode::Text("Diary".to_string())],
                span: None,
            }],
        );
        let original = DocumentAnalysis::of(&noted(1));

        // When / Then
        assert!(!original.folds_like(&DocumentAnalysis::of(&orphan)));
        assert!(!original.folds_like(&DocumentAnalysis::of(&retitled)));
    }

    #[test]
    fn test_of_is_not_an_orphan_without_the_field() {
        // Given
        let doc = Document::new("index.rst".to_string(), Vec::new());

        // When / Then
        assert!(!DocumentAnalysis::of(&doc).orphan);
    }
}
