//! Which build file declares a package.

use std::path::{Path, PathBuf};

use super::labels::split_label;

/// The build file declaring the library `label` (`//pkg:name`) under
/// `workspace` — `pkg/BUILD.bazel`, or else `pkg/BUILD` — or `None` for a
/// label of another repository or a package with neither.
#[must_use]
pub fn build_file_of(workspace: &Path, label: &str) -> Option<PathBuf> {
    let (package, _) = split_label(label)?;
    let directory = package
        .split('/')
        .filter(|component| !component.is_empty())
        .fold(workspace.to_path_buf(), |path, component| {
            path.join(component)
        });
    ["BUILD.bazel", "BUILD"]
        .into_iter()
        .map(|name| directory.join(name))
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_file_of_prefers_build_bazel_then_build() {
        // Given
        let workspace =
            std::env::temp_dir().join(format!("rinx_build_file_of_{}", std::process::id()));
        std::fs::create_dir_all(workspace.join("a")).unwrap();
        std::fs::create_dir_all(workspace.join("b")).unwrap();
        std::fs::write(workspace.join("a/BUILD.bazel"), "").unwrap();
        std::fs::write(workspace.join("a/BUILD"), "").unwrap();
        std::fs::write(workspace.join("b/BUILD"), "").unwrap();

        // When
        let found = [
            build_file_of(&workspace, "//a:docs"),
            build_file_of(&workspace, "//b:docs"),
            build_file_of(&workspace, "//c:docs"),
            build_file_of(&workspace, "@ext//a:docs"),
        ];
        let _ = std::fs::remove_dir_all(&workspace);

        // Then
        assert_eq!(
            found,
            [
                Some(workspace.join("a/BUILD.bazel")),
                Some(workspace.join("b/BUILD")),
                None,
                None,
            ]
        );
    }
}
