//! How a document open in the editor reads the files its directives name.
//!
//! The parser performs no I/O, so `.. include::`, `.. literalinclude::` and a
//! `.. csv-table::`'s `:file:` ask a [`ParseFileLoader`] for their text. This
//! one resolves relative to the file the directive was written in — the same
//! docutils rule the build's loader follows — and then prefers the buffer the
//! editor holds for that file over what is saved: an open buffer is the truth,
//! so a fragment's unsaved edit is what its includers are diagnosed against.
//!
//! It also remembers every file it was asked for, read or not. That is how the
//! server learns which documents include which files — to re-diagnose an
//! includer when a fragment changes — and it keeps each file's text, against
//! which a diagnostic found inside it is placed.
//!
//! The server knows no source root yet, so a `/`-rooted path is refused rather
//! than resolved against something it is not; workspace awareness brings the
//! root.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rinx_parser::{LoadedFile, ParseFileLoader};

use crate::diagnostics::protocol_line_endings;
use crate::documents::DocumentStore;

/// What one parse asked its loader for.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FileReads {
    /// Every file a directive named, resolved — including one that could not
    /// be read, since opening or creating it later changes the parse.
    pub paths: BTreeSet<PathBuf>,
    /// The text of every file that was read, as the parser saw it.
    pub texts: BTreeMap<PathBuf, String>,
}

/// Reads paths relative to the document at an absolute filesystem path,
/// open buffers first and the disk second.
pub struct WorkspaceFiles<'a> {
    document: PathBuf,
    open: &'a DocumentStore,
    reads: RefCell<FileReads>,
}

impl<'a> WorkspaceFiles<'a> {
    /// A loader for the document at `document`, an absolute path, that sees
    /// the buffers in `open`.
    #[must_use]
    pub fn for_document(document: &Path, open: &'a DocumentStore) -> Self {
        Self {
            document: document.to_path_buf(),
            open,
            reads: RefCell::default(),
        }
    }

    /// Everything this loader was asked for.
    #[must_use]
    pub fn into_reads(self) -> FileReads {
        self.reads.into_inner()
    }

    /// The text of the file at `path`: its open buffer, or else its contents
    /// on disk.
    fn read(&self, path: &Path) -> std::io::Result<String> {
        match self.open.get_by_path(path) {
            Some(buffer) => Ok(buffer.text.clone()),
            None => std::fs::read_to_string(path),
        }
    }
}

impl ParseFileLoader for WorkspaceFiles<'_> {
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
        let read = self.read(&resolved);
        let mut reads = self.reads.borrow_mut();
        reads.paths.insert(resolved.clone());
        let text = read.map_err(|error| format!("cannot read '{id}': {error}"))?;
        // Lines are counted as the editor counts them, so a position inside
        // the file lands on the line the editor shows.
        let text = protocol_line_endings(&text).into_owned();
        reads.texts.insert(resolved, text.clone());
        Ok(LoadedFile { id, text })
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
        let open = DocumentStore::default();
        let loader = WorkspaceFiles::for_document(&dir.join("index.rst"), &open);

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
        let open = DocumentStore::default();
        let loader = WorkspaceFiles::for_document(&dir.join("index.rst"), &open);
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
        let open = DocumentStore::default();
        let loader = WorkspaceFiles::for_document(&dir.join("index.rst"), &open);

        // When
        let Err(error) = loader.load("absent.csv", None) else {
            panic!("a missing file must not load");
        };

        // Then
        assert!(error.contains("absent.csv"), "{error}");
    }

    #[test]
    fn test_load_prefers_an_open_buffer_over_the_disk() {
        // Given a fragment saved with one text and open with another
        let dir = temp_directory("buffer", &[("part.rst", "saved\n")]);
        let mut open = DocumentStore::default();
        let part = dir.join("part.rst");
        let uri = crate::uri::file_uri(&part).expect("an absolute path");
        open.open(uri, 2, "unsaved\r\n".to_string());
        let loader = WorkspaceFiles::for_document(&dir.join("index.rst"), &open);

        // When
        let file = loader.load("part.rst", None).expect("read the buffer");

        // Then — with the editor's line endings normalized as the document's are
        assert_eq!(file.text, "unsaved\n");
    }

    #[test]
    fn test_load_records_what_it_read_and_what_it_could_not() {
        // Given
        let dir = temp_directory("reads", &[("data.csv", "a\n")]);
        let open = DocumentStore::default();
        let loader = WorkspaceFiles::for_document(&dir.join("index.rst"), &open);

        // When
        let _ = loader.load("data.csv", None);
        let _ = loader.load("absent.rst", None);
        let reads = loader.into_reads();

        // Then
        assert_eq!(
            reads.paths,
            BTreeSet::from([dir.join("absent.rst"), dir.join("data.csv")])
        );
        assert_eq!(
            reads.texts,
            BTreeMap::from([(dir.join("data.csv"), "a\n".to_string())])
        );
    }

    #[test]
    fn test_load_refuses_a_source_root_path() {
        // Given
        let dir = temp_directory("rooted", &[]);
        let open = DocumentStore::default();
        let loader = WorkspaceFiles::for_document(&dir.join("index.rst"), &open);

        // When
        let Err(error) = loader.load("/shared/data.csv", None) else {
            panic!("a rooted path must not load");
        };

        // Then
        assert!(error.contains("source root"), "{error}");
    }
}
