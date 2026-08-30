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
}
