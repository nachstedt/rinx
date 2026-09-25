//! Turning a document (or one of its sections) into the title and href a
//! navigation entry shows.
//!
//! One implementation, called by both the in-page toctree renderer and the
//! sidebar. Before this existed the two disagreed: the block renderer took an
//! entry's text from `index.document_titles` while the sidebar took it from
//! the navigation node's own `title` field, so the same document could be
//! labelled differently in the two places on one page.

use rinx_ast::SectionId;
use rinx_index::ProjectIndex;

/// The `.html` href for `docname`, relative to the document being rendered,
/// with `anchor` appended when one is given.
///
/// `from_doc` is the site-relative logical path of the page being rendered
/// (`guide/intro`), whose *directory* the result is relative to. The href is
/// build-controlled rather than author-written, so it is safe to hand to
/// `MiniJinja` as a safe string — which is what keeps its `/` separators from
/// being entity-escaped into unreadability.
#[must_use]
pub(crate) fn document_href(
    docname: &str,
    anchor: Option<&SectionId>,
    from_doc: &str,
) -> minijinja::Value {
    let from_dir = std::path::Path::new(from_doc)
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""));

    let without_ext = docname.strip_suffix(".rst").unwrap_or(docname);
    let target = format!("{without_ext}.html");
    let relative = pathdiff::diff_paths(std::path::Path::new(&target), from_dir).map_or_else(
        || target.clone(),
        |p| p.to_string_lossy().replace('\\', "/"),
    );

    let href = match anchor {
        Some(anchor) => format!("{relative}#{}", anchor.as_str()),
        None => relative,
    };
    minijinja::Value::from_safe_string(href)
}

/// The text a navigation entry for `docname` shows.
///
/// Precedence, highest first: the title the author wrote on the toctree entry
/// itself (`Getting started <intro>`); the document's own `H1`; and finally
/// the docname, so an untitled document is still identifiable rather than
/// blank.
#[must_use]
pub(crate) fn document_title(
    docname: &str,
    explicit: Option<&str>,
    index: &ProjectIndex,
) -> String {
    if let Some(title) = explicit {
        return title.to_string();
    }
    index
        .document_titles
        .get(docname)
        .cloned()
        .unwrap_or_else(|| docname.strip_suffix(".rst").unwrap_or(docname).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_href_is_relative_to_the_rendering_page() {
        // Given / When
        let href = document_href("guide/setup.rst", None, "index");

        // Then
        assert_eq!(href.to_string(), "guide/setup.html");
    }

    #[test]
    fn test_document_href_climbs_out_of_a_nested_directory() {
        // Given / When
        let href = document_href("intro.rst", None, "guide/setup");

        // Then
        assert_eq!(href.to_string(), "../intro.html");
    }

    #[test]
    fn test_document_href_appends_a_section_anchor() {
        // Given / When
        let href = document_href(
            "guide/setup.rst",
            Some(&SectionId::from_title("From Source")),
            "index",
        );

        // Then
        assert_eq!(href.to_string(), "guide/setup.html#from-source");
    }

    #[test]
    fn test_document_href_keeps_separators_unescaped() {
        // Given — a safe string, so MiniJinja does not turn `/` into `&#x2f;`.
        let href = document_href("a/b/c.rst", None, "index");

        // When / Then
        assert!(!href.to_string().contains("&#x2f;"), "{href}");
    }

    #[test]
    fn test_document_title_prefers_the_authors_explicit_title() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .document_titles
            .insert("intro.rst".to_string(), "Introduction".to_string());

        // When
        let title = document_title("intro.rst", Some("Getting started"), &index);

        // Then
        assert_eq!(title, "Getting started");
    }

    #[test]
    fn test_document_title_falls_back_to_the_documents_own_heading() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .document_titles
            .insert("intro.rst".to_string(), "Introduction".to_string());

        // When
        let title = document_title("intro.rst", None, &index);

        // Then
        assert_eq!(title, "Introduction");
    }

    #[test]
    fn test_document_title_falls_back_to_the_docname_for_an_untitled_document() {
        // Given
        let index = ProjectIndex::default();

        // When
        let title = document_title("guide/setup.rst", None, &index);

        // Then — identifiable rather than blank.
        assert_eq!(title, "guide/setup");
    }
}
