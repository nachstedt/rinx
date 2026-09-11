//! Turning a document path or an entity id into the href a page links it by.
//!
//! Every internal cross-reference needs this and they must all agree, since a
//! page's links are resolved against its own location — so `:ref:`, `:eq:`, an
//! image's `:target:` and a diagram's generated node links share one
//! implementation rather than each computing the same `pathdiff` from their own
//! copy of it.
//!
//! In this crate rather than in the renderer, where it began, because the
//! renderer is no longer the only phase that builds a link: a templated
//! diagram's `flow()` and `ref()` emit `PlantUML` node links into text compiled
//! by a build action, and a link that disagreed with the page's own would send
//! a reader somewhere else. Both callers already depend on this crate, and
//! [`ProjectIndex`](crate::ProjectIndex) is what either one resolves against.

/// The relative href from `doc_path`'s directory to `target_doc`'s rendered
/// HTML page.
///
/// Both paths are *source* paths (`guide/math.rst`); the extension swap to
/// `.html` happens here, because the caller is always linking to a rendered
/// page rather than to the source.
///
/// Falls back to the target's own path when no relative route exists, which
/// keeps the link pointing somewhere plausible rather than dropping it.
#[must_use]
pub fn relative_doc_href(target_doc: &str, doc_path: &str) -> String {
    let current_dir = std::path::Path::new(doc_path)
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""));
    let target_html_path = std::path::Path::new(target_doc).with_extension("html");
    let relative_path =
        pathdiff::diff_paths(&target_html_path, current_dir).unwrap_or(target_html_path);
    relative_path.display().to_string()
}

/// The `id` an entity's anchor carries, and the fragment a link to it uses.
///
/// One function rather than two spellings, so a reference and the definition
/// it points at cannot drift — the same reason `build_domain_object_key`
/// exists for domain objects. It sits beside [`relative_doc_href`] because a
/// complete link to an entity is the two joined, and three phases now build
/// one.
#[must_use]
pub fn entity_anchor(id: &str) -> String {
    format!("entity-{id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relative_doc_href_within_the_same_directory() {
        // Given two documents side by side
        // When
        let href = relative_doc_href("other.rst", "index.rst");

        // Then
        assert_eq!(href, "other.html");
    }

    #[test]
    fn test_relative_doc_href_from_a_subdirectory_upwards() {
        // Given a page nested one level below its target
        // When
        let href = relative_doc_href("index.rst", "team_a/index.rst");

        // Then
        assert_eq!(href, "../index.html");
    }

    #[test]
    fn test_relative_doc_href_across_sibling_directories() {
        // Given two pages in different subdirectories
        // When
        let href = relative_doc_href("team_b/index.rst", "team_a/index.rst");

        // Then
        assert_eq!(href, "../team_b/index.html");
    }

    #[test]
    fn test_relative_doc_href_downwards_into_a_subdirectory() {
        // Given a root page linking into a subdirectory
        // When
        let href = relative_doc_href("team_a/index.rst", "index.rst");

        // Then
        assert_eq!(href, "team_a/index.html");
    }

    #[test]
    fn test_entity_anchor_prefixes_the_id() {
        // Given / When
        let anchor = entity_anchor("REQ_001");

        // Then
        assert_eq!(anchor, "entity-REQ_001");
    }

    #[test]
    fn test_entity_anchor_is_the_same_for_a_link_and_a_definition() {
        // Given — the whole point of one function: a reference and the anchor
        // it lands on cannot drift
        let id = "REQ_001";

        // When / Then
        assert_eq!(entity_anchor(id), entity_anchor(id));
    }
}
