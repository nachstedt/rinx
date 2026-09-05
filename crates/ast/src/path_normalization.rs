use std::path::{Component, Path, PathBuf};

/// Normalizes a path, resolving `.` and `..` components.
#[must_use]
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            _ => normalized.push(component),
        }
    }
    normalized
}

/// Resolves a path an author wrote inside `doc_path` into a path from the
/// source root.
///
/// docutils' rule, and the one every file-naming construct in this crate
/// follows: a leading `/` means "from the source root", not "from the
/// filesystem root"; anything else resolves against the directory holding
/// `doc_path`. `..` components are resolved, so the result is a key that the
/// bundler, the validator, the renderer, the include loader and the embedder
/// all agree on.
///
/// Shared rather than reimplemented per construct because disagreement here is
/// invisible and expensive: two phases resolving `../shared/logo.png`
/// differently means one of them silently looks at the wrong file.
#[must_use]
pub fn resolve_from_document(written: &str, doc_path: &str) -> PathBuf {
    if let Some(from_root) = written.strip_prefix('/') {
        return normalize_path(Path::new(from_root));
    }
    let directory = Path::new(doc_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    normalize_path(&directory.join(written))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path_basic() {
        // Given
        let path = std::path::Path::new("a/b/c");

        // When
        let normalized = normalize_path(path);

        // Then
        assert_eq!(normalized, std::path::PathBuf::from("a/b/c"));
    }
    #[test]
    fn test_normalize_path_current_dir() {
        // Given
        let path1 = std::path::Path::new("a/./c");
        let path2 = std::path::Path::new("./a/b");

        // When
        let normalized1 = normalize_path(path1);
        let normalized2 = normalize_path(path2);

        // Then
        assert_eq!(normalized1, std::path::PathBuf::from("a/c"));
        assert_eq!(normalized2, std::path::PathBuf::from("a/b"));
    }
    #[test]
    fn test_normalize_path_parent_dir() {
        // Given
        let path1 = std::path::Path::new("a/b/../c");
        let path2 = std::path::Path::new("a/b/../../c");

        // When
        let normalized1 = normalize_path(path1);
        let normalized2 = normalize_path(path2);

        // Then
        assert_eq!(normalized1, std::path::PathBuf::from("a/c"));
        assert_eq!(normalized2, std::path::PathBuf::from("c"));
    }
    #[test]
    fn test_normalize_path_complex() {
        // Given
        let path1 = std::path::Path::new("a/./b/../c/d/./../e");
        let path2 = std::path::Path::new("/a/b/../c");

        // When
        let normalized1 = normalize_path(path1);
        let normalized2 = normalize_path(path2);

        // Then
        assert_eq!(normalized1, std::path::PathBuf::from("a/c/e"));
        assert_eq!(normalized2, std::path::PathBuf::from("/a/c"));
    }
    #[test]
    fn test_normalize_path_above_root() {
        // Given
        let path1 = std::path::Path::new("../a");
        let path2 = std::path::Path::new("a/../../b");
        let path3 = std::path::Path::new("/../a");

        // When
        let normalized1 = normalize_path(path1);
        let normalized2 = normalize_path(path2);
        let normalized3 = normalize_path(path3);

        // Then
        assert_eq!(normalized1, std::path::PathBuf::from("a"));
        assert_eq!(normalized2, std::path::PathBuf::from("b"));
        assert_eq!(normalized3, std::path::PathBuf::from("/a"));
    }
    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path(Path::new("a/b/../c")), PathBuf::from("a/c"));
        assert_eq!(normalize_path(Path::new("a/./b")), PathBuf::from("a/b"));
        assert_eq!(normalize_path(Path::new("a/b/c")), PathBuf::from("a/b/c"));

        // Edge cases with excessive ParentDir
        assert_eq!(normalize_path(Path::new("..")), PathBuf::from(""));
        assert_eq!(normalize_path(Path::new("a/../../b")), PathBuf::from("b"));
        assert_eq!(normalize_path(Path::new("../a/b")), PathBuf::from("a/b"));
    }

    #[test]
    fn test_resolve_from_document_is_relative_to_the_documents_directory() {
        // Given / When
        let resolved = resolve_from_document("logo.png", "guide/intro.rst");

        // Then
        assert_eq!(resolved, PathBuf::from("guide/logo.png"));
    }

    #[test]
    fn test_resolve_from_document_treats_a_leading_slash_as_the_source_root() {
        // Given / When — docutils' rule, not the filesystem's
        let resolved = resolve_from_document("/shared/logo.png", "guide/intro.rst");

        // Then
        assert_eq!(resolved, PathBuf::from("shared/logo.png"));
    }

    #[test]
    fn test_resolve_from_document_walks_out_of_the_documents_directory() {
        // Given / When
        let resolved = resolve_from_document("../shared/logo.png", "guide/intro.rst");

        // Then
        assert_eq!(resolved, PathBuf::from("shared/logo.png"));
    }

    #[test]
    fn test_resolve_from_document_handles_a_document_at_the_source_root() {
        // Given a document with no directory component
        let resolved = resolve_from_document("logo.png", "index.rst");

        // Then
        assert_eq!(resolved, PathBuf::from("logo.png"));
    }
}
