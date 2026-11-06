//! The Bazel package a migration writes: where it lies in its workspace, and
//! what a glob over it would match.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use super::module_bazel::MODULE_FILE;

/// The files that make a directory a Bazel package of its own.
const BUILD_FILES: [&str; 2] = ["BUILD", "BUILD.bazel"];

/// What `glob` sees of a package's directory.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PackageListing {
    /// Every `.rst` file `glob(["**/*.rst"])` matches, relative and
    /// `/`-separated.
    pub rst_files: BTreeSet<String>,
    /// Every directory below that is a package of its own, which no glob of
    /// this one enters.
    pub subpackages: BTreeSet<String>,
    /// Every symbolic link to a directory, which a glob would follow but the
    /// project's documents are never found through.
    pub linked_directories: BTreeSet<String>,
}

impl PackageListing {
    /// Whether `relative` lies in a package of its own.
    #[must_use]
    pub(crate) fn in_subpackage(&self, relative: &str) -> bool {
        self.subpackages
            .iter()
            .any(|package| relative.starts_with(&format!("{package}/")))
    }
}

/// What a glob over the package at `root` would see.
#[must_use]
pub(crate) fn list_package(root: &Path) -> PackageListing {
    let mut listing = PackageListing::default();
    walk(root, "", &mut listing);
    listing
}

fn walk(directory: &Path, relative: &str, listing: &mut PackageListing) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let child = if relative.is_empty() {
            name.clone()
        } else {
            format!("{relative}/{name}")
        };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if file_type.is_symlink() && path.is_dir() {
            listing.linked_directories.insert(child);
        } else if file_type.is_dir() {
            if BUILD_FILES.iter().any(|build| path.join(build).is_file()) {
                listing.subpackages.insert(child);
            } else {
                walk(&path, &child, listing);
            }
        } else if is_rst(Path::new(&name)) {
            listing.rst_files.insert(child);
        }
    }
}

/// Whether `path` is a `.rst` file to `glob` and `srcs` alike — whose
/// matching is case-sensitive, so `.RST` is not.
#[must_use]
pub(crate) fn is_rst(path: &Path) -> bool {
    path.extension() == Some(OsStr::new("rst"))
}

/// The `MODULE.bazel` of the Bazel workspace holding `directory`: the
/// nearest at or above it.
#[must_use]
pub(crate) fn enclosing_module(directory: &Path) -> Option<PathBuf> {
    directory
        .ancestors()
        .map(|ancestor| ancestor.join(MODULE_FILE))
        .find(|module| module.is_file())
}

/// `path` relative to `root`, `/`-separated — `None` when it lies outside.
#[must_use]
pub(crate) fn relative_to(path: &Path, root: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let components: Vec<String> = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    Some(components.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_tree(name: &str, files: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("rinx_lsp_migration_package_{name}"));
        let _ = fs::remove_dir_all(&root);
        for file in files {
            let path = root.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "").unwrap();
        }
        root
    }

    #[test]
    fn test_list_package_lists_the_rst_files_and_stops_at_a_subpackage() {
        // Given
        let root = temp_tree(
            "listing",
            &[
                "index.rst",
                "logo.png",
                ".hidden/a.rst",
                "guide/setup.rst",
                "tools/BUILD.bazel",
                "tools/notes.rst",
            ],
        );

        // When
        let listing = list_package(&root);

        // Then
        assert_eq!(
            listing.rst_files,
            BTreeSet::from([
                ".hidden/a.rst".to_string(),
                "guide/setup.rst".to_string(),
                "index.rst".to_string(),
            ])
        );
        assert_eq!(listing.subpackages, BTreeSet::from(["tools".to_string()]));
        assert!(listing.in_subpackage("tools/notes.rst"));
        assert!(!listing.in_subpackage("toolsmith.rst"));
    }

    #[cfg(unix)]
    #[test]
    fn test_list_package_does_not_follow_a_linked_directory() {
        // Given
        let root = temp_tree("linked", &["index.rst", "elsewhere/a.rst"]);
        std::os::unix::fs::symlink(root.join("elsewhere"), root.join("link")).unwrap();

        // When
        let listing = list_package(&root);

        // Then
        assert_eq!(
            listing.linked_directories,
            BTreeSet::from(["link".to_string()])
        );
        assert!(!listing.rst_files.contains("link/a.rst"));
    }

    #[test]
    fn test_is_rst_matches_the_suffix_as_bazel_does() {
        // When / Then
        assert!(is_rst(Path::new("guide/a.rst")));
        assert!(!is_rst(Path::new("a.RST")));
        assert!(!is_rst(Path::new("a.txt")));
    }

    #[test]
    fn test_enclosing_module_finds_the_nearest_module_above() {
        // Given
        let root = temp_tree("module", &["MODULE.bazel", "Doc/conf.py"]);

        // When / Then
        assert_eq!(
            enclosing_module(&root.join("Doc")),
            Some(root.join("MODULE.bazel"))
        );
    }

    #[test]
    fn test_relative_to_joins_with_slashes_and_refuses_an_outside_path() {
        // When / Then
        assert_eq!(
            relative_to(Path::new("/a/b/c.rst"), Path::new("/a")),
            Some("b/c.rst".to_string())
        );
        assert_eq!(relative_to(Path::new("/x/c.rst"), Path::new("/a")), None);
    }
}
