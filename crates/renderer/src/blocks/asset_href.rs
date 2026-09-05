//! Turning a bundled asset's path into an `src` relative to the page being
//! rendered.
//!
//! Every image on a page — a compiled `.. plantuml::` diagram and an authored
//! `.. image::` alike — is served from the site's single `_images/` directory,
//! and every page links to it from wherever that page happens to sit. The two
//! must agree: they did not, once, and the diagram renderer's private copy of
//! this arithmetic was the only correct one.

use std::path::Path;

/// The site directory every bundled asset is copied into.
///
/// Matches Sphinx's own `_images/`, so a hand-written stylesheet or a
/// deployment script that already knows Sphinx's layout keeps working.
pub(super) const ASSET_DIR: &str = "_images";

/// The relative href from `doc_path`'s directory to `asset_path` under the
/// site's `_images/` directory.
///
/// `asset_path` is the asset's path *within* `_images/` — a diagram's
/// `<hash>.svg`, or an authored image's source-root-relative path. `doc_path`
/// is the source path of the page doing the linking.
///
/// Falls back to the un-relativized path when no relative route exists, which
/// keeps the `src` pointing somewhere plausible rather than dropping it —
/// the same fallback `crate::doc_href` makes for the same reason.
pub(super) fn relative_asset_href(asset_path: &Path, doc_path: &str) -> String {
    let current_dir = Path::new(doc_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let in_site = Path::new(ASSET_DIR).join(asset_path);
    let relative = pathdiff::diff_paths(&in_site, current_dir).unwrap_or(in_site);
    // Always emit forward slashes: this is a URL, not a filesystem path, and a
    // build on Windows must still produce a href a browser can follow.
    relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_links_from_a_root_level_page() {
        // Given
        let asset = PathBuf::from("examples/logo.svg");

        // When
        let href = relative_asset_href(&asset, "index.rst");

        // Then
        assert_eq!(href, "_images/examples/logo.svg");
    }

    #[test]
    fn test_climbs_out_of_a_nested_page_directory() {
        // Given
        let asset = PathBuf::from("logo.svg");

        // When
        let href = relative_asset_href(&asset, "guide/intro.rst");

        // Then
        assert_eq!(href, "../_images/logo.svg");
    }

    #[test]
    fn test_climbs_out_of_a_deeply_nested_page_directory() {
        // Given
        let asset = PathBuf::from("team/logo.svg");

        // When
        let href = relative_asset_href(&asset, "a/b/c/page.rst");

        // Then
        assert_eq!(href, "../../../_images/team/logo.svg");
    }

    #[test]
    fn test_emits_forward_slashes_for_a_nested_asset() {
        // Given
        let asset = PathBuf::from("a/b/logo.svg");

        // When
        let href = relative_asset_href(&asset, "index.rst");

        // Then — a URL, so never a backslash, whatever the host separator is
        assert!(!href.contains('\\'), "unexpected backslash in {href}");
        assert_eq!(href, "_images/a/b/logo.svg");
    }
}
