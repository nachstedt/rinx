//! Resolving a document name — what `:doc:` writes, and one of the kinds
//! `:any:` searches — to a document of this site.
//!
//! The name is relative to the referencing document, or to the source root
//! with a leading `/`, which is [`rinx_toctree::resolve_docname`]'s rule — the
//! one a `.. toctree::` entry follows too, so a page a toctree reaches under a
//! name is linked by `:doc:` under the same name.

use rinx_index::ProjectIndex;

/// The `.rst` path of the document `target` names when written in the
/// document at `doc_path`, borrowed from `index`, or `None` when this site
/// has no such document.
///
/// A document with no title is found as well as a titled one: a `:doc:` to
/// it still links, showing `<no title>` as Sphinx does.
pub(crate) fn resolve_document<'a>(
    index: &'a ProjectIndex,
    doc_path: &str,
    target: &str,
) -> Option<&'a str> {
    index.find_document(&rinx_toctree::resolve_docname(doc_path, target))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_of(documents: &[&str]) -> ProjectIndex {
        let mut index = ProjectIndex::default();
        index
            .documents
            .extend(documents.iter().map(ToString::to_string));
        index
    }

    #[test]
    fn test_resolve_document_is_relative_to_the_referencing_document() {
        // Given
        let index = index_of(&["guide/install.rst", "install.rst"]);

        // When
        let found = resolve_document(&index, "guide/intro.rst", "install");

        // Then
        assert_eq!(found, Some("guide/install.rst"));
    }

    #[test]
    fn test_resolve_document_climbs_out_of_a_directory() {
        // Given
        let index = index_of(&["notes.rst"]);

        // When
        let found = resolve_document(&index, "guide/intro.rst", "../notes");

        // Then
        assert_eq!(found, Some("notes.rst"));
    }

    #[test]
    fn test_resolve_document_reads_a_leading_slash_from_the_source_root() {
        // Given
        let index = index_of(&["index.rst", "guide/index.rst"]);

        // When
        let found = resolve_document(&index, "guide/intro.rst", "/index");

        // Then
        assert_eq!(found, Some("index.rst"));
    }

    #[test]
    fn test_resolve_document_finds_a_titled_document_of_an_older_index() {
        // Given — an index that recorded the document only by its title
        let mut index = ProjectIndex::default();
        index
            .document_titles
            .insert("guide.rst".to_string(), "Guide".to_string());

        // When
        let found = resolve_document(&index, "index.rst", "guide");

        // Then
        assert_eq!(found, Some("guide.rst"));
    }

    #[test]
    fn test_resolve_document_finds_nothing_for_an_unknown_name() {
        // Given
        let index = index_of(&["index.rst"]);

        // When
        let found = resolve_document(&index, "index.rst", "missing");

        // Then
        assert_eq!(found, None);
    }
}
