//! The `data:` URIs a page's `:loading: embed` images are rendered with.
//!
//! The renderer performs no I/O, so it cannot open an image file itself — the
//! same rule that makes `rusty_sphinx_parser` read a `:file:` through an
//! injected loader. Here the answer is a lookup table rather than a loader,
//! because the bytes are read once per document by a separate build step (the
//! worker's `embed_assets` command) whose whole purpose is to be cached: an
//! image edit re-runs that cheap step for every document, but only a document
//! that actually embeds the changed image gets different bytes, so only that
//! page re-renders. See `docs/decisions/007-image-assets.md`.
//!
//! Keys are the image's *resolved* project path — what
//! [`rusty_sphinx_ast::ImageUri::resolve`] returns — never the URI as written,
//! so `logo.png` and `./logo.png` in the same document find one entry.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The embeddable assets one document needs, keyed by resolved project path.
///
/// A `BTreeMap` rather than a `HashMap` so the serialized sidecar has a stable
/// byte-for-byte order: an unordered map would make the cache firewall leak,
/// re-writing the file — and so re-rendering the page — whenever the hash seed
/// changed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EmbeddedAssets(BTreeMap<String, String>);

impl EmbeddedAssets {
    /// An empty table — every image renders as an ordinary link.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records `data_uri` as the embedded form of the asset at `path`.
    pub fn insert(&mut self, path: &Path, data_uri: String) {
        self.0.insert(path_key(path), data_uri);
    }

    /// The `data:` URI for the asset at `path`, if it was embedded.
    #[must_use]
    pub fn get(&self, path: &Path) -> Option<&str> {
        self.0.get(&path_key(path)).map(String::as_str)
    }

    /// Whether nothing at all was embedded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// How many assets were embedded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

/// The map key for `path`.
///
/// Normalized to forward slashes so a sidecar written on one platform is read
/// correctly on another — the path is a document-relative identity, not a
/// location on the building machine's filesystem.
fn path_key(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_new_is_empty() {
        // Given / When
        let assets = EmbeddedAssets::new();

        // Then
        assert!(assets.is_empty());
        assert_eq!(assets.len(), 0);
    }

    #[test]
    fn test_get_returns_an_inserted_asset() {
        // Given
        let mut assets = EmbeddedAssets::new();
        assets.insert(
            Path::new("images/logo.svg"),
            "data:image/svg+xml;base64,AAA".into(),
        );

        // When
        let found = assets.get(Path::new("images/logo.svg"));

        // Then
        assert_eq!(found, Some("data:image/svg+xml;base64,AAA"));
        assert_eq!(assets.len(), 1);
    }

    #[test]
    fn test_get_returns_nothing_for_an_unknown_asset() {
        // Given
        let assets = EmbeddedAssets::new();

        // When / Then
        assert_eq!(assets.get(Path::new("missing.png")), None);
    }

    #[test]
    fn test_lookup_is_independent_of_the_path_separator() {
        // Given — a sidecar written where the separator differs from the reader's
        let mut assets = EmbeddedAssets::new();
        let written: PathBuf = ["images", "logo.svg"].iter().collect();
        assets.insert(&written, "data:image/svg+xml;base64,AAA".into());

        // When
        let found = assets.get(Path::new("images/logo.svg"));

        // Then
        assert_eq!(found, Some("data:image/svg+xml;base64,AAA"));
    }

    #[test]
    fn test_serializes_as_a_plain_map() {
        // Given
        let mut assets = EmbeddedAssets::new();
        assets.insert(
            Path::new("logo.svg"),
            "data:image/svg+xml;base64,AAA".into(),
        );

        // When
        let json = serde_json::to_string(&assets).expect("should serialize");

        // Then — no wrapper object, so the sidecar reads as what it is
        assert_eq!(json, r#"{"logo.svg":"data:image/svg+xml;base64,AAA"}"#);
    }

    #[test]
    fn test_serialization_round_trips() {
        // Given
        let mut assets = EmbeddedAssets::new();
        assets.insert(Path::new("a/logo.svg"), "data:image/png;base64,BBB".into());

        // When
        let json = serde_json::to_string(&assets).expect("should serialize");
        let restored: EmbeddedAssets = serde_json::from_str(&json).expect("should deserialize");

        // Then
        assert_eq!(restored, assets);
    }

    #[test]
    fn test_serialization_order_is_stable() {
        // Given — the cache firewall depends on identical bytes for identical
        // content, so insertion order must not reach the file
        let mut one = EmbeddedAssets::new();
        one.insert(Path::new("b.svg"), "data:b".into());
        one.insert(Path::new("a.svg"), "data:a".into());
        let mut other = EmbeddedAssets::new();
        other.insert(Path::new("a.svg"), "data:a".into());
        other.insert(Path::new("b.svg"), "data:b".into());

        // When
        let one_json = serde_json::to_string(&one).expect("should serialize");
        let other_json = serde_json::to_string(&other).expect("should serialize");

        // Then
        assert_eq!(one_json, other_json);
    }
}
