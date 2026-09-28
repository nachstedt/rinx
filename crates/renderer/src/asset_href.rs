//! Turning a bundled asset's path into an `href`/`src` relative to the page
//! being rendered.
//!
//! The site serves two directories of files it copied or generated: every
//! image — a compiled `.. plantuml::` diagram and an authored `.. image::`
//! alike — from `_images/`, and every `:download:` file from `_downloads/`.
//! Every page links into them from wherever that page happens to sit, and all
//! of those links must agree: they did not, once, and the diagram renderer's
//! private copy of this arithmetic was the only correct one. Flat at the crate
//! root because `blocks/` (images) and `inline/` (downloads) both reach it.

use std::path::Path;

/// A site directory bundled assets are copied into.
///
/// Both names match Sphinx's own, so a hand-written stylesheet or a
/// deployment script that already knows Sphinx's layout keeps working. Unlike
/// Sphinx, an authored file keeps its source-root-relative path inside either
/// one — see `docs/decisions/007-image-assets.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssetDir {
    /// `_images/`: compiled diagrams and authored images.
    Images,
    /// `_downloads/`: the files `:download:` links.
    Downloads,
}

impl AssetDir {
    /// The directory's name at the site root.
    const fn name(self) -> &'static str {
        match self {
            Self::Images => "_images",
            Self::Downloads => "_downloads",
        }
    }
}

/// The relative href from `doc_path`'s directory to `asset_path` under the
/// site directory `dir`.
///
/// `asset_path` is the asset's path *within* that directory — a diagram's
/// `<hash>.svg`, or an authored file's source-root-relative path. `doc_path`
/// is the source path of the page doing the linking.
///
/// Falls back to the un-relativized path when no relative route exists, which
/// keeps the link pointing somewhere plausible rather than dropping it —
/// the same fallback `rinx_index::relative_doc_href` makes, for the same
/// reason.
pub(crate) fn relative_asset_href(dir: AssetDir, asset_path: &Path, doc_path: &str) -> String {
    let current_dir = Path::new(doc_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let in_site = Path::new(dir.name()).join(asset_path);
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
        let href = relative_asset_href(AssetDir::Images, &asset, "index.rst");

        // Then
        assert_eq!(href, "_images/examples/logo.svg");
    }

    #[test]
    fn test_climbs_out_of_a_nested_page_directory() {
        // Given
        let asset = PathBuf::from("logo.svg");

        // When
        let href = relative_asset_href(AssetDir::Images, &asset, "guide/intro.rst");

        // Then
        assert_eq!(href, "../_images/logo.svg");
    }

    #[test]
    fn test_climbs_out_of_a_deeply_nested_page_directory() {
        // Given
        let asset = PathBuf::from("team/logo.svg");

        // When
        let href = relative_asset_href(AssetDir::Images, &asset, "a/b/c/page.rst");

        // Then
        assert_eq!(href, "../../../_images/team/logo.svg");
    }

    #[test]
    fn test_emits_forward_slashes_for_a_nested_asset() {
        // Given
        let asset = PathBuf::from("a/b/logo.svg");

        // When
        let href = relative_asset_href(AssetDir::Images, &asset, "index.rst");

        // Then — a URL, so never a backslash, whatever the host separator is
        assert!(!href.contains('\\'), "unexpected backslash in {href}");
        assert_eq!(href, "_images/a/b/logo.svg");
    }

    #[test]
    fn test_links_into_the_downloads_directory() {
        // Given
        let asset = PathBuf::from("examples/data/sample.csv");

        // When
        let href = relative_asset_href(AssetDir::Downloads, &asset, "examples/nested/page.rst");

        // Then
        assert_eq!(href, "../../_downloads/examples/data/sample.csv");
    }
}
