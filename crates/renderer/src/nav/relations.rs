//! The previous/next page links, derived from the project's reading order.
//!
//! The order itself is computed once, project-wide, by the analyzer (see
//! `page_order` there); this only looks a document up in it. Storing the
//! sequence rather than each page's two neighbours keeps one source of truth —
//! two stored strings per document could disagree with the sequence they came
//! from, a position in it cannot.

use rinx_index::ProjectIndex;

use super::resolve::{document_href, document_title};

/// A link to an adjacent page, ready for the page template.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PageLink {
    pub title: String,
    pub href: minijinja::Value,
}

/// The pages before and after `docname` in reading order.
///
/// A document the reading order does not contain — one no toctree reaches —
/// gets neither, rather than being wedged in at an arbitrary end. The first
/// page has no previous and the last no next.
#[must_use]
pub fn page_neighbors(
    index: &ProjectIndex,
    docname: &str,
    from_doc: &str,
) -> (Option<PageLink>, Option<PageLink>) {
    let Some(position) = index.page_order.iter().position(|path| path == docname) else {
        return (None, None);
    };

    let link = |path: &String| PageLink {
        title: document_title(path, None, index),
        href: document_href(path, None, from_doc),
    };

    let previous = position
        .checked_sub(1)
        .and_then(|index_before| index.page_order.get(index_before))
        .map(&link);
    let next = index.page_order.get(position + 1).map(&link);

    (previous, next)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_of(order: &[&str]) -> ProjectIndex {
        let mut index = ProjectIndex::default();
        for path in order {
            index
                .document_titles
                .insert((*path).to_string(), format!("Title of {path}"));
        }
        index.page_order = order.iter().map(|p| (*p).to_string()).collect();
        index
    }

    #[test]
    fn test_page_neighbors_returns_both_sides_in_the_middle() {
        // Given
        let index = index_of(&["a.rst", "b.rst", "c.rst"]);

        // When
        let (previous, next) = page_neighbors(&index, "b.rst", "b");

        // Then
        assert_eq!(
            previous.map(|link| link.title),
            Some("Title of a.rst".to_string())
        );
        assert_eq!(
            next.map(|link| link.title),
            Some("Title of c.rst".to_string())
        );
    }

    #[test]
    fn test_the_first_page_has_no_previous() {
        // Given
        let index = index_of(&["a.rst", "b.rst"]);

        // When
        let (previous, next) = page_neighbors(&index, "a.rst", "a");

        // Then
        assert!(previous.is_none());
        assert!(next.is_some());
    }

    #[test]
    fn test_the_last_page_has_no_next() {
        // Given
        let index = index_of(&["a.rst", "b.rst"]);

        // When
        let (previous, next) = page_neighbors(&index, "b.rst", "b");

        // Then
        assert!(previous.is_some());
        assert!(next.is_none());
    }

    #[test]
    fn test_a_document_outside_the_reading_order_has_no_neighbors() {
        // Given — an orphan, which no toctree reaches.
        let index = index_of(&["a.rst", "b.rst"]);

        // When
        let (previous, next) = page_neighbors(&index, "orphan.rst", "orphan");

        // Then — better than wedging it in at an arbitrary end.
        assert!(previous.is_none());
        assert!(next.is_none());
    }

    #[test]
    fn test_neighbor_hrefs_are_relative_to_the_rendering_page() {
        // Given
        let index = index_of(&["intro.rst", "guide/setup.rst", "outro.rst"]);

        // When
        let (previous, next) = page_neighbors(&index, "guide/setup.rst", "guide/setup");

        // Then
        assert_eq!(previous.unwrap().href.to_string(), "../intro.html");
        assert_eq!(next.unwrap().href.to_string(), "../outro.html");
    }

    #[test]
    fn test_a_lone_page_has_no_neighbors() {
        // Given
        let index = index_of(&["only.rst"]);

        // When
        let (previous, next) = page_neighbors(&index, "only.rst", "only");

        // Then
        assert!(previous.is_none());
        assert!(next.is_none());
    }
}
