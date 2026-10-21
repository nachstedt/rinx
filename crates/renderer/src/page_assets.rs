//! Where a page finds the files it shows: the `data:` URIs the build embedded,
//! and — for a page no site directory is written beside — where each authored
//! image lies.
//!
//! The build links an image into the site's `_images/` directory, which its
//! bundling action fills. The language server's preview writes no site, so it
//! says where an image is instead, through an injected [`ImageLocator`]: the
//! renderer still performs no I/O and knows no address scheme, the same split
//! that makes `rinx_filter` learn about a subject through a trait.

use std::path::Path;

use crate::EmbeddedAssets;

/// Where a page finds an authored image, when not in the site's `_images/`.
pub trait ImageLocator {
    /// The address of the image at `resolved` — its path from the source
    /// root, as [`rinx_ast::AssetUri::resolve`] returns it — or `None` to
    /// link it into `_images/` as the build does.
    fn image_href(&self, resolved: &Path) -> Option<String>;
}

/// The files a page is rendered with, beside its document and its index.
#[derive(Clone, Copy)]
pub struct PageAssets<'a> {
    /// The `data:` URIs for the document's `:loading: embed` images.
    pub embedded: &'a EmbeddedAssets,
    /// Where an authored image lies, for a page with no `_images/` beside
    /// it; `None` in the build.
    pub images: Option<&'a dyn ImageLocator>,
}

impl<'a> PageAssets<'a> {
    /// The build's assets: `embedded`, every other image in `_images/`.
    #[must_use]
    pub fn embedding(embedded: &'a EmbeddedAssets) -> Self {
        Self {
            embedded,
            images: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_locates_no_image() {
        // Given
        let embedded = EmbeddedAssets::new();

        // When
        let assets = PageAssets::embedding(&embedded);

        // Then
        assert!(assets.images.is_none());
        assert!(assets.embedded.is_empty());
    }
}
