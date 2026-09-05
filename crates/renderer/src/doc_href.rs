//! Turning a target document's source path into an href relative to the page
//! being rendered.
//!
//! Every internal cross-reference needs this and they must all agree, since a
//! page's links are resolved against its own location — so `:ref:` and `:eq:`
//! share one implementation rather than each computing the same `pathdiff`
//! from their own copy of it.
//!
//! Flat at the crate root rather than under `inline/`, because `blocks/` needs
//! it too: an image's `:target:` may name an internal target, and its link has
//! to be computed exactly the way a `:ref:`'s is.

/// The relative href from `doc_path`'s directory to `target_doc`'s rendered
/// HTML page.
///
/// Both paths are *source* paths (`guide/math.rst`); the extension swap to
/// `.html` happens here, because the caller is always linking to a rendered
/// page rather than to the source.
///
/// Falls back to the target's own path when no relative route exists, which
/// keeps the link pointing somewhere plausible rather than dropping it.
pub(crate) fn relative_doc_href(target_doc: &str, doc_path: &str) -> String {
    let current_dir = std::path::Path::new(doc_path)
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""));
    let target_html_path = std::path::Path::new(target_doc).with_extension("html");
    let relative_path =
        pathdiff::diff_paths(&target_html_path, current_dir).unwrap_or(target_html_path);
    relative_path.display().to_string()
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
}
