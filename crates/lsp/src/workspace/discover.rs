//! Finding the documents of a workspace folder.
//!
//! Every `*.rst` file under the folder is a document, with two fixed
//! exceptions, and nothing else decides:
//!
//! - **A symlinked directory is not followed.** A Bazel workspace holds
//!   `bazel-bin`, `bazel-out` and friends — symlinks into the build's output,
//!   which mirror the whole source tree — so following them would index every
//!   document twice. Following links also risks a cycle. A symlinked *file* is
//!   a document like any other.
//! - **A hidden directory is skipped**: `.git`, `.venv`, `.tox` and the like
//!   hold tooling, never documentation.
//!
//! `.gitignore` is deliberately not consulted: which files a project
//! documents is a decision of its own configuration, and generated sources —
//! exactly what an ignore file lists — are often part of it. A project's own
//! exclusions arrive with `conf.py` (roadmap #10) and the Bazel manifest
//! (#18).

use std::fs;
use std::path::{Path, PathBuf};

/// The extension a document has.
const SOURCE_EXTENSION: &str = "rst";

/// Every document under `root`, sorted. A directory that cannot be read is
/// skipped rather than failing the whole scan: an unreadable corner of a
/// workspace should cost its own documents, not everyone else's.
#[must_use]
pub fn discover_sources(root: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            // `DirEntry::file_type` does not follow symlinks, which is the
            // whole point: a symlinked directory reports as a symlink.
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if !is_hidden(&path) {
                    pending.push(path);
                }
            } else if is_source(&path) && (file_type.is_file() || path.is_file()) {
                sources.push(path);
            }
        }
    }
    sources.sort();
    sources
}

/// Whether `path` names a document by its extension.
pub(super) fn is_source(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == SOURCE_EXTENSION)
}

/// Whether `path` is a document [`discover_sources`] would find under
/// `root`: a source with no hidden directory between `root` and it. Whether a
/// directory on the way is a symlink is not asked — a file-system watcher does
/// not follow one either, so no event arrives from behind it.
#[must_use]
pub fn is_discoverable(root: &Path, path: &Path) -> bool {
    is_source(path) && is_visible_under(root, path)
}

/// Whether `path` lies under `root` with no hidden component in between,
/// itself included.
#[must_use]
pub fn is_visible_under(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root).is_ok_and(|relative| {
        !relative
            .components()
            .any(|component| component.as_os_str().to_string_lossy().starts_with('.'))
    })
}

/// Whether the last component of `path` starts with a dot.
fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory under the system's temporary one, holding `files`
    /// (relative paths, each written with a line of text).
    fn temp_tree(name: &str, files: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("rinx_lsp_discover_{name}"));
        let _ = fs::remove_dir_all(&root);
        for file in files {
            let path = root.join(file);
            fs::create_dir_all(path.parent().expect("a file has a parent")).expect("mkdir");
            fs::write(&path, "Text\n").expect("write");
        }
        root
    }

    fn relative(root: &Path, sources: &[PathBuf]) -> Vec<String> {
        sources
            .iter()
            .map(|path| {
                path.strip_prefix(root)
                    .expect("under the root")
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    #[test]
    fn test_discover_sources_finds_every_rst_file_in_order() {
        // Given
        let root = temp_tree(
            "nested",
            &[
                "index.rst",
                "guide/setup.rst",
                "guide/notes.txt",
                "a/b/c.rst",
            ],
        );

        // When
        let sources = discover_sources(&root);

        // Then
        assert_eq!(
            relative(&root, &sources),
            ["a/b/c.rst", "guide/setup.rst", "index.rst"]
        );
    }

    #[test]
    fn test_discover_sources_skips_hidden_directories() {
        // Given
        let root = temp_tree(
            "hidden",
            &["index.rst", ".venv/lib/readme.rst", ".git/x.rst"],
        );

        // When
        let sources = discover_sources(&root);

        // Then
        assert_eq!(relative(&root, &sources), ["index.rst"]);
    }

    #[cfg(unix)]
    #[test]
    fn test_discover_sources_does_not_follow_a_symlinked_directory() {
        // Given — a Bazel-style convenience link mirroring the tree.
        let root = temp_tree("symlink", &["index.rst"]);
        std::os::unix::fs::symlink(&root, root.join("bazel-out")).expect("symlink");

        // When
        let sources = discover_sources(&root);

        // Then — the document once, not again through the link.
        assert_eq!(relative(&root, &sources), ["index.rst"]);
    }

    #[cfg(unix)]
    #[test]
    fn test_discover_sources_keeps_a_symlinked_file() {
        // Given — a generated document linked into the tree.
        let root = temp_tree("linked_file", &["generated/api.rst"]);
        std::os::unix::fs::symlink(root.join("generated/api.rst"), root.join("api.rst"))
            .expect("symlink");

        // When
        let sources = discover_sources(&root);

        // Then
        assert_eq!(relative(&root, &sources), ["api.rst", "generated/api.rst"]);
    }

    #[test]
    fn test_is_discoverable_agrees_with_the_scan_on_what_a_document_is() {
        // Given
        let root = Path::new("/work/docs");

        // When / Then
        assert!(is_discoverable(
            root,
            Path::new("/work/docs/guide/setup.rst")
        ));
        assert!(!is_discoverable(
            root,
            Path::new("/work/docs/.venv/lib/readme.rst")
        ));
        assert!(!is_discoverable(root, Path::new("/work/docs/data.csv")));
        assert!(!is_discoverable(root, Path::new("/elsewhere/setup.rst")));
    }

    #[test]
    fn test_is_visible_under_refuses_a_hidden_path_and_one_outside() {
        // Given
        let root = Path::new("/work/.config/docs");

        // When / Then — the root's own hidden ancestors do not count
        assert!(is_visible_under(
            root,
            Path::new("/work/.config/docs/guide")
        ));
        assert!(!is_visible_under(
            root,
            Path::new("/work/.config/docs/.git")
        ));
        assert!(!is_visible_under(root, Path::new("/work/other")));
    }

    #[test]
    fn test_discover_sources_finds_nothing_in_a_missing_folder() {
        // Given
        let root = std::env::temp_dir().join("rinx_lsp_discover_missing_folder_does_not_exist");

        // When / Then
        assert!(discover_sources(&root).is_empty());
    }
}
