//! Where an image's bytes come from, and where clicking it goes.
//!
//! An image URI is split into "somewhere else on the web" and "a file in this
//! project" the moment it is parsed, because three later phases each ask a
//! different question of it and all three would otherwise re-sniff the string
//! for a `://`: the Bazel validator asks which files must have been declared,
//! the asset embedder asks which files to read, and the renderer asks what to
//! put in `src`.
//!
//! What is *not* done at parse time is resolving a document-relative path into
//! a project-relative one. The parser does not know which document it is
//! parsing (see `ParseCtx`, which carries a slice origin, not a path), and the
//! AST is meant to record what the author wrote. [`ImageUri::resolve`] is the
//! single function all three phases resolve with instead, so none of them can
//! disagree about where `../shared/logo.png` points.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::path_normalization::normalize_path;
use crate::target_name::TargetName;

/// Whether `raw` names a resource outside this project.
///
/// Ported from Sphinx's own `url_re` (`^[a-z][a-z0-9+.-]*://`), widened by the
/// one scheme that carries its payload inline instead of behind a `//`
/// authority: `data:`. Anything else — including a Windows-looking `C:\…` — is
/// a project file, which is the reading that makes an undeclared path fail the
/// build rather than silently becoming a dangling external link.
fn is_external(raw: &str) -> bool {
    let Some((scheme, rest)) = raw.split_once(':') else {
        return false;
    };
    let mut characters = scheme.chars();
    let well_formed = characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '.' | '-')
        });
    well_formed && (rest.starts_with("//") || scheme.eq_ignore_ascii_case("data"))
}

/// The `.. image::`/`.. figure::` argument: what to display.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImageUri {
    /// An absolute URL, passed through to `src` untouched. Never bundled,
    /// never validated against the declared assets, and never embedded — a
    /// build that fetched it would stop being hermetic.
    External(String),
    /// A file in this project, exactly as the author wrote it: relative to the
    /// document, or — with a leading `/` — to the source root.
    Document(String),
}

impl ImageUri {
    /// Reads a directive argument.
    #[must_use]
    pub fn new(raw: &str) -> Self {
        let trimmed = raw.trim();
        if is_external(trimmed) {
            Self::External(trimmed.to_string())
        } else {
            Self::Document(trimmed.to_string())
        }
    }

    /// The URI exactly as written.
    #[must_use]
    pub fn as_written(&self) -> &str {
        match self {
            Self::External(uri) | Self::Document(uri) => uri,
        }
    }

    /// This URI as a path from the source root, given the document that wrote
    /// it. `None` for [`Self::External`], which names no project file.
    ///
    /// A leading `/` means "from the source root" in docutils, not "from the
    /// filesystem root"; anything else resolves against the directory holding
    /// `doc_path`. `..` components are resolved here, so the result is a key
    /// the bundler, the validator and the renderer all agree on.
    #[must_use]
    pub fn resolve(&self, doc_path: &str) -> Option<PathBuf> {
        let Self::Document(written) = self else {
            return None;
        };
        if let Some(from_root) = written.strip_prefix('/') {
            return Some(normalize_path(Path::new(from_root)));
        }
        let directory = Path::new(doc_path)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        Some(normalize_path(&directory.join(written)))
    }
}

/// What an image's `:target:` points at.
///
/// docutils decides between the two with `parse_target`: a value ending in a
/// single `_` is an indirect reference to a named target, anything else is a
/// URI. The trailing marker is markup rather than part of the name, so it is
/// stripped here and the intent recorded as the variant instead.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImageTarget {
    /// A plain URI, used as the link's `href` verbatim.
    Uri(String),
    /// A named target elsewhere in the project, resolved against the index at
    /// render time. Normalized through [`TargetName`] — docutils'
    /// `fully_normalize_name` — so it matches a target however it was cased.
    Reference(TargetName),
}

impl ImageTarget {
    /// Reads a `:target:` value.
    #[must_use]
    pub fn new(raw: &str) -> Self {
        let trimmed = raw.trim();
        // A trailing `__` is an *anonymous* reference in reST. docutils'
        // `parse_target` does not accept one here, so it stays a URI rather
        // than silently consuming an anonymous target this directive never
        // declared.
        if trimmed.ends_with('_') && !trimmed.ends_with("__") && trimmed.len() > 1 {
            let name = trimmed.trim_end_matches('_');
            return Self::Reference(TargetName::new(name));
        }
        Self::Uri(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_reads_an_http_url_as_external() {
        // Given
        let raw = "https://example.com/logo.png";

        // When
        let uri = ImageUri::new(raw);

        // Then
        assert_eq!(uri, ImageUri::External(raw.to_string()));
    }

    #[test]
    fn test_new_reads_a_data_uri_as_external() {
        // Given — no `//` authority, but still not a project file
        let raw = "data:image/png;base64,AAAA";

        // When
        let uri = ImageUri::new(raw);

        // Then
        assert_eq!(uri, ImageUri::External(raw.to_string()));
    }

    #[test]
    fn test_new_reads_a_relative_path_as_a_document_file() {
        // Given
        let raw = "images/logo.png";

        // When
        let uri = ImageUri::new(raw);

        // Then
        assert_eq!(uri, ImageUri::Document(raw.to_string()));
    }

    #[test]
    fn test_new_reads_a_windows_style_path_as_a_document_file() {
        // Given — a drive letter is not a URL scheme
        let raw = r"C:\images\logo.png";

        // When
        let uri = ImageUri::new(raw);

        // Then
        assert!(matches!(uri, ImageUri::Document(_)));
    }

    #[test]
    fn test_new_trims_surrounding_whitespace() {
        // Given
        let raw = "  logo.png  ";

        // When
        let uri = ImageUri::new(raw);

        // Then
        assert_eq!(uri.as_written(), "logo.png");
    }

    #[test]
    fn test_resolve_joins_a_relative_path_to_the_documents_directory() {
        // Given
        let uri = ImageUri::new("images/logo.png");

        // When
        let resolved = uri.resolve("guide/intro.rst");

        // Then
        assert_eq!(resolved, Some(PathBuf::from("guide/images/logo.png")));
    }

    #[test]
    fn test_resolve_handles_a_document_at_the_source_root() {
        // Given
        let uri = ImageUri::new("logo.png");

        // When
        let resolved = uri.resolve("index.rst");

        // Then
        assert_eq!(resolved, Some(PathBuf::from("logo.png")));
    }

    #[test]
    fn test_resolve_reads_a_leading_slash_as_the_source_root() {
        // Given
        let uri = ImageUri::new("/shared/logo.png");

        // When
        let resolved = uri.resolve("deep/nested/page.rst");

        // Then
        assert_eq!(resolved, Some(PathBuf::from("shared/logo.png")));
    }

    #[test]
    fn test_resolve_collapses_parent_components() {
        // Given
        let uri = ImageUri::new("../shared/logo.png");

        // When
        let resolved = uri.resolve("guide/intro.rst");

        // Then
        assert_eq!(resolved, Some(PathBuf::from("shared/logo.png")));
    }

    #[test]
    fn test_resolve_returns_nothing_for_an_external_uri() {
        // Given
        let uri = ImageUri::new("https://example.com/logo.png");

        // When
        let resolved = uri.resolve("index.rst");

        // Then
        assert_eq!(resolved, None);
    }

    #[test]
    fn test_uri_serialization_round_trips() {
        // Given
        let uri = ImageUri::new("images/logo.png");

        // When
        let json = serde_json::to_string(&uri).expect("should serialize");
        let restored: ImageUri = serde_json::from_str(&json).expect("should deserialize");

        // Then
        assert_eq!(restored, uri);
    }

    #[test]
    fn test_target_new_reads_a_trailing_underscore_as_a_reference() {
        // Given
        let raw = "My Target_";

        // When
        let target = ImageTarget::new(raw);

        // Then
        assert_eq!(target, ImageTarget::Reference(TargetName::new("My Target")));
    }

    #[test]
    fn test_target_new_reads_a_url_as_a_uri() {
        // Given
        let raw = "https://example.com/";

        // When
        let target = ImageTarget::new(raw);

        // Then
        assert_eq!(target, ImageTarget::Uri("https://example.com/".to_string()));
    }

    #[test]
    fn test_target_new_reads_a_double_underscore_as_a_uri() {
        // Given — an anonymous reference, which docutils does not accept here
        let raw = "something__";

        // When
        let target = ImageTarget::new(raw);

        // Then
        assert_eq!(target, ImageTarget::Uri("something__".to_string()));
    }

    #[test]
    fn test_target_new_reads_a_bare_underscore_as_a_uri() {
        // Given — nothing is left once the marker is stripped
        let raw = "_";

        // When
        let target = ImageTarget::new(raw);

        // Then
        assert_eq!(target, ImageTarget::Uri("_".to_string()));
    }

    #[test]
    fn test_target_serialization_round_trips() {
        // Given
        let target = ImageTarget::new("some name_");

        // When
        let json = serde_json::to_string(&target).expect("should serialize");
        let restored: ImageTarget = serde_json::from_str(&json).expect("should deserialize");

        // Then
        assert_eq!(restored, target);
    }
}
