use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::diagnostic::Diagnostic;
use crate::node::Node;
use crate::span::{FileId, Span};
use crate::suppression::Suppression;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub path: String,
    pub nodes: Vec<Node>,
    /// What went wrong while parsing, recorded rather than raised — the
    /// parser degrades bad input and lets the build step decide whether any
    /// of this is fatal.
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
    /// The `.. noqa:` comments this document carries, already resolved to the
    /// line ranges they cover.
    ///
    /// Serialized with the AST because the diagnostics they silence are not
    /// all raised in the same process: a broken link is found at *render*
    /// time, long after the comment that excuses it was parsed.
    #[serde(default)]
    pub suppressions: Vec<Suppression>,
    /// The document-level field list written before the title, if any —
    /// Sphinx's file-wide metadata.
    ///
    /// Deliberately *not* general field-list support: only a contiguous run of
    /// `:name: value` lines at the very top of a document is read, values stay
    /// raw strings, and nothing renders it. That is where Sphinx requires
    /// `:orphan:`, which is the one field that currently means anything, so
    /// this is the minimum needed rather than a half-built version of a
    /// construct that deserves its own implementation.
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    /// The files whose text was spliced into this document by `.. include::`
    /// or `.. literalinclude::`, indexed by [`FileId`].
    ///
    /// [`Self::path`] is deliberately *not* entry zero: a [`Span`] with no
    /// file already means this document, so reserving an id for it would give
    /// the same place two spellings. Empty for the overwhelming majority of
    /// documents, which include nothing.
    ///
    /// Paths are relative to the source root, as [`Self::path`] is, so a
    /// warning can print one without further resolution.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_files: Vec<String>,
    /// The hash of the entity schema this document was parsed against.
    ///
    /// Recorded because the schema reaches the *parse* action, so a library
    /// can be parsed against one schema while the site indexing it uses
    /// another — a build misconfiguration that would otherwise surface as a
    /// cascade of baffling unknown-directive diagnostics rather than as the
    /// one real problem. The index phase compares this against its own and
    /// reports `entity.schema-mismatch`.
    ///
    /// `None` for a document parsed with no schema at all, which is every
    /// document in a project that does not use entities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_schema_hash: Option<String>,
}

impl Document {
    #[must_use]
    pub const fn new(path: String, nodes: Vec<Node>) -> Self {
        Self {
            path,
            nodes,
            diagnostics: Vec::new(),
            suppressions: Vec::new(),
            metadata: BTreeMap::new(),
            source_files: Vec::new(),
            entity_schema_hash: None,
        }
    }

    /// The path a [`Span`]'s [`file`](Span::file) names, or this document's own
    /// path when the span carries none.
    ///
    /// The one place an id becomes something a human can read, so every
    /// warning that mentions a position goes through it. An id with no entry
    /// yields `None` rather than a panic: an `.ast` is a file on disk that a
    /// build may have written with a different version of this crate, and a
    /// mangled one should degrade a warning's precision, not abort the render.
    #[must_use]
    pub fn span_path(&self, span: Option<Span>) -> Option<&str> {
        match span.and_then(|span| span.file) {
            None => Some(&self.path),
            Some(file) => self.source_files.get(file.index()).map(String::as_str),
        }
    }

    /// Records `path` as an included file and returns its id, reusing the id
    /// of a path already recorded.
    ///
    /// Deduplicating matters: a document that includes the same fragment in
    /// twenty places must not carry twenty copies of its path, and two spans
    /// in the same fragment must compare equal on their file.
    pub fn intern_source_file(&mut self, path: impl Into<String>) -> FileId {
        let path = path.into();
        let index = self
            .source_files
            .iter()
            .position(|known| known == &path)
            .unwrap_or_else(|| {
                self.source_files.push(path);
                self.source_files.len() - 1
            });
        FileId::new(u32::try_from(index).unwrap_or(u32::MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline_node::InlineNode;

    #[test]
    fn test_new_creates_document_with_given_nodes() {
        // Given
        let nodes = vec![
            Node::Heading {
                level: 1,
                text: vec![InlineNode::Text("Title".to_string())],
            },
            Node::Paragraph(vec![InlineNode::Text("Body".to_string())]),
        ];

        // When
        let doc = Document::new("test.rst".to_string(), nodes);

        // Then
        assert_eq!(doc.nodes.len(), 2);
    }

    #[test]
    fn test_a_new_document_includes_nothing() {
        // Given / When
        let document = Document::new("guide.rst".to_string(), Vec::new());

        // Then
        assert!(document.source_files.is_empty());
    }

    #[test]
    fn test_intern_source_file_assigns_ids_in_order() {
        // Given
        let mut document = Document::new("guide.rst".to_string(), Vec::new());

        // When
        let first = document.intern_source_file("shared/params.rst");
        let second = document.intern_source_file("shared/returns.rst");

        // Then
        assert_eq!(first, FileId::new(0));
        assert_eq!(second, FileId::new(1));
        assert_eq!(
            document.source_files,
            vec![
                "shared/params.rst".to_string(),
                "shared/returns.rst".to_string()
            ]
        );
    }

    #[test]
    fn test_intern_source_file_reuses_the_id_of_a_known_path() {
        // Given a fragment already included once
        let mut document = Document::new("guide.rst".to_string(), Vec::new());
        let first = document.intern_source_file("shared/params.rst");

        // When the same fragment is included again
        let again = document.intern_source_file("shared/params.rst");

        // Then — one entry, one id, so spans in it compare equal
        assert_eq!(again, first);
        assert_eq!(document.source_files.len(), 1);
    }

    #[test]
    fn test_span_path_of_a_span_without_a_file_is_the_document() {
        // Given
        let document = Document::new("guide.rst".to_string(), Vec::new());
        let span = Span::whole_line(3, "abc");

        // When / Then
        assert_eq!(document.span_path(Some(span)), Some("guide.rst"));
    }

    #[test]
    fn test_span_path_of_no_span_at_all_is_the_document() {
        // Given a positionless diagnostic's span
        let document = Document::new("guide.rst".to_string(), Vec::new());

        // When / Then — the document is still the right thing to name
        assert_eq!(document.span_path(None), Some("guide.rst"));
    }

    #[test]
    fn test_span_path_of_an_included_span_is_that_file() {
        // Given
        let mut document = Document::new("guide.rst".to_string(), Vec::new());
        let file = document.intern_source_file("shared/params.rst");
        let span = Span::whole_line(3, "abc").with_file(Some(file));

        // When / Then
        assert_eq!(document.span_path(Some(span)), Some("shared/params.rst"));
    }

    #[test]
    fn test_span_path_of_an_unknown_file_id_is_none() {
        // Given an `.ast` whose span names a file its table does not have
        let document = Document::new("guide.rst".to_string(), Vec::new());
        let span = Span::whole_line(3, "abc").with_file(Some(FileId::new(9)));

        // When / Then — a mangled file degrades precision, it does not panic
        assert_eq!(document.span_path(Some(span)), None);
    }

    #[test]
    fn test_serialization_omits_an_empty_source_file_table() {
        // Given a document that includes nothing
        let document = Document::new("guide.rst".to_string(), Vec::new());

        // When
        let json = serde_json::to_string(&document).expect("Failed to serialize");

        // Then — the common case must not grow the `.ast` wire form
        assert!(!json.contains("source_files"), "{json}");
    }

    #[test]
    fn test_serialization_roundtrip_with_included_files() {
        // Given
        let mut document = Document::new("guide.rst".to_string(), Vec::new());
        document.intern_source_file("shared/params.rst");

        // When
        let json = serde_json::to_string(&document).expect("Failed to serialize");
        let deserialized: Document = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(document, deserialized);
    }
}
