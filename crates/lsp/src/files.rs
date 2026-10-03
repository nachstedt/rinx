//! How a document open in the editor reads the files its directives name.
//!
//! The parser performs no I/O, so `.. include::`, `.. literalinclude::` and a
//! `.. csv-table::`'s `:file:` ask a [`ParseFileLoader`] for their text. This
//! one reads the disk, resolving relative to the file the directive was written
//! in — the same docutils rule the build's loader follows. Without it every such
//! directive would be underlined as unreadable in the editor while building
//! fine.
//!
//! The server knows no source root yet, so a `/`-rooted path is refused rather
//! than resolved against something it is not; workspace awareness brings the
//! root. A buffer open in the editor is not consulted either: what is read is
//! what is saved.

use rinx_parser::{LoadedFile, ParseFileLoader};
use std::path::{Path, PathBuf};

/// Reads paths relative to the document at an absolute filesystem path.
pub struct DiskFiles {
    document: PathBuf,
}

impl DiskFiles {
    /// A loader for the document at `document`, an absolute path.
    #[must_use]
    pub fn for_document(document: &Path) -> Self {
        Self {
            document: document.to_path_buf(),
        }
    }
}

impl ParseFileLoader for DiskFiles {
    fn load(&self, path: &str, relative_to: Option<&str>) -> Result<LoadedFile, String> {
        if path.starts_with('/') {
            return Err(format!(
                "cannot read '{path}': the editor does not know this project's source root yet"
            ));
        }
        // A directive inside an included fragment resolves against that
        // fragment, whose id is the absolute path this loader returned for it.
        let anchor = relative_to.map_or_else(|| self.document.to_string_lossy(), Into::into);
        let resolved = rinx_ast::resolve_from_document(path, &anchor);
        let id = resolved.to_string_lossy().into_owned();
        std::fs::read_to_string(&resolved)
            .map(|text| LoadedFile {
                id: id.clone(),
                text,
            })
            .map_err(|error| format!("cannot read '{id}': {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory holding `files`.
    fn temp_directory(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rinx_lsp_files_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        for (relative, contents) in files {
            let target = dir.join(relative);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).expect("create temp subdir");
            }
            std::fs::write(target, contents).expect("write temp file");
        }
        dir
    }

    #[test]
    fn test_load_reads_a_file_beside_the_document() {
        // Given
        let dir = temp_directory("beside", &[("data.csv", "a, b\n")]);
        let loader = DiskFiles::for_document(&dir.join("index.rst"));

        // When
        let file = loader.load("data.csv", None).expect("read the file");

        // Then
        assert_eq!(file.text, "a, b\n");
        assert_eq!(file.id, dir.join("data.csv").to_string_lossy());
    }

    #[test]
    fn test_load_resolves_against_the_including_fragment() {
        // Given
        let dir = temp_directory("fragment", &[("shared/rows.csv", "x\n")]);
        let loader = DiskFiles::for_document(&dir.join("index.rst"));
        let fragment = dir.join("shared/part.rst");

        // When
        let file = loader
            .load("rows.csv", Some(&fragment.to_string_lossy()))
            .expect("read the file");

        // Then
        assert_eq!(file.text, "x\n");
    }

    #[test]
    fn test_load_reports_a_missing_file() {
        // Given
        let dir = temp_directory("missing", &[]);
        let loader = DiskFiles::for_document(&dir.join("index.rst"));

        // When
        let Err(error) = loader.load("absent.csv", None) else {
            panic!("a missing file must not load");
        };

        // Then
        assert!(error.contains("absent.csv"), "{error}");
    }

    #[test]
    fn test_load_refuses_a_source_root_path() {
        // Given
        let dir = temp_directory("rooted", &[]);
        let loader = DiskFiles::for_document(&dir.join("index.rst"));

        // When
        let Err(error) = loader.load("/shared/data.csv", None) else {
            panic!("a rooted path must not load");
        };

        // Then
        assert!(error.contains("source root"), "{error}");
    }
}
