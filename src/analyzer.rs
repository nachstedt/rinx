//! The analyzer module represents the global indexing phase.
//!
//! For simple single-file documents with basic structure, this is currently a pass-through
//! step that yields an empty `ProjectIndex`.

use crate::ast::Document;

#[derive(Debug)]
pub struct ProjectIndex {}

/// Analyzes a Document and generates a global `ProjectIndex`.
/// Currently a no-op that returns an empty `ProjectIndex`.
#[must_use]
pub fn analyze(_doc: &Document) -> ProjectIndex {
    ProjectIndex {}
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
}
