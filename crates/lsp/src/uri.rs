//! Between the protocol's `file:` URIs and the filesystem paths the parser's
//! loader works in.
//!
//! The protocol names a document by URI, while an `.. include::` names a file
//! by path, so a diagnostic found inside an included fragment has to cross
//! back: its file is known by the path the loader resolved, and it is published
//! under a URI. Both directions live here so they stay inverses.
//!
//! Read off the URI `lsp-types` already parsed, rather than through the `url`
//! crate, whose international-domain support brings in a score of crates to
//! answer a question about `file:` URIs that never have a domain.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use lsp_types::Uri;

/// The filesystem path behind `uri`, when it names a file on this machine.
#[must_use]
pub fn file_path(uri: &Uri) -> Option<PathBuf> {
    let is_file = uri
        .scheme()
        .is_some_and(|scheme| scheme.as_str().eq_ignore_ascii_case("file"));
    let is_local = uri
        .authority()
        .is_none_or(|authority| matches!(authority.as_str(), "" | "localhost"));
    if !is_file || !is_local {
        return None;
    }
    let path = uri.path().as_estr().decode().into_string().ok()?;
    Some(PathBuf::from(path.as_ref()))
}

/// The `file:` URI naming the absolute `path`, or `None` for a relative path
/// or one that is not UTF-8.
///
/// Every byte but an unreserved character or `/` is percent-encoded. A client
/// may encode a different set of characters for the same file, so a URI built
/// here is only for a file the client has *not* opened: an open one is
/// published under the URI the client gave it.
#[must_use]
pub fn file_uri(path: &Path) -> Option<Uri> {
    let path = path.to_str()?;
    if !path.starts_with('/') {
        return None;
    }
    let mut uri = String::from("file://");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            let _ = write!(uri, "%{byte:02X}");
        }
    }
    uri.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(text: &str) -> Uri {
        text.parse().expect("valid uri")
    }

    #[test]
    fn test_file_path_reads_a_file_uri() {
        // Given / When
        let path = file_path(&uri("file:///docs/a%20b.rst"));

        // Then
        assert_eq!(path, Some(PathBuf::from("/docs/a b.rst")));
    }

    #[test]
    fn test_file_path_accepts_localhost() {
        // Given / When
        let path = file_path(&uri("file://localhost/docs/index.rst"));

        // Then
        assert_eq!(path, Some(PathBuf::from("/docs/index.rst")));
    }

    #[test]
    fn test_file_path_is_none_for_a_remote_host() {
        // Given / When / Then
        assert_eq!(file_path(&uri("file://server/docs/index.rst")), None);
    }

    #[test]
    fn test_file_path_is_none_for_another_scheme() {
        // Given / When / Then
        assert_eq!(file_path(&uri("untitled:Untitled-1")), None);
    }

    #[test]
    fn test_file_uri_leaves_a_plain_path_readable() {
        // Given / When
        let built = file_uri(Path::new("/docs/shared/part_1.rst"));

        // Then
        assert_eq!(built, Some(uri("file:///docs/shared/part_1.rst")));
    }

    #[test]
    fn test_file_uri_percent_encodes_spaces_percent_signs_and_non_ascii() {
        // Given / When
        let built = file_uri(Path::new("/docs/a b%c/é.rst"));

        // Then
        assert_eq!(built, Some(uri("file:///docs/a%20b%25c/%C3%A9.rst")));
    }

    #[test]
    fn test_file_uri_refuses_a_relative_path() {
        // Given / When / Then
        assert_eq!(file_uri(Path::new("docs/index.rst")), None);
    }

    #[test]
    fn test_file_uri_and_file_path_are_inverses() {
        // Given
        let path = PathBuf::from("/a dir/with #hash?/and%25/ü.rst");

        // When
        let round_trip = file_uri(&path).as_ref().and_then(file_path);

        // Then
        assert_eq!(round_trip, Some(path));
    }
}
