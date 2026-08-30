//! The filesystem side of `.. csv-table::`'s `:file:` option.
//!
//! `rusty_sphinx_parser` performs no I/O of its own, so it asks for the file's
//! contents through a [`parser::CsvFileLoader`]. This is the implementation
//! the CLI supplies: paths resolve against the directory of the document being
//! parsed, exactly as docutils resolves them against the source file.
//!
//! Under Bazel that directory is inside the action's sandbox, so the file is
//! only there if it was declared — `rusty_sphinx_library`'s `csv_data`
//! attribute is what puts it in the parse action's inputs.
//!
//! An unreadable file is recorded as well as diagnosed, because the parser
//! itself is deliberately error-resilient: it degrades the directive to
//! `Unknown` and carries on, which is right for the live preview but would
//! otherwise let a build silently ship a page with the whole table missing.
//! [`DocumentRelativeCsvFiles::failures`] is what lets the `parse` subcommand
//! turn that into a failed build, the same way a missing diagram image does.

use rusty_sphinx_parser::{self as parser, CsvFileLoader};
use std::cell::RefCell;
use std::path::{Path, PathBuf};

/// Reads `:file:` paths relative to the parsed document's own directory,
/// remembering the ones it could not read.
pub(super) struct DocumentRelativeCsvFiles {
    base_dir: PathBuf,
    /// `RefCell` because [`CsvFileLoader::load`] takes `&self` — the parser
    /// only ever borrows the loader immutably, and a parse is single-threaded.
    failures: RefCell<Vec<String>>,
}

impl DocumentRelativeCsvFiles {
    /// Builds a loader for the document at `doc_path`, resolving against the
    /// directory containing it (the current directory when it has none).
    pub(super) fn for_document(doc_path: &str) -> Self {
        let base_dir = Path::new(doc_path)
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        Self {
            base_dir,
            failures: RefCell::new(Vec::new()),
        }
    }

    /// The messages for every `:file:` this loader could not read, in the
    /// order they were requested. Empty when every read succeeded.
    pub(super) fn failures(&self) -> Vec<String> {
        self.failures.borrow().clone()
    }
}

impl CsvFileLoader for DocumentRelativeCsvFiles {
    fn load(&self, path: &str) -> Result<String, String> {
        let resolved = self.base_dir.join(path);
        std::fs::read_to_string(&resolved).map_err(|error| {
            let message = format!(
                "cannot read '{}': {error} — if this is a Bazel build, add the file to \
                 the library's csv_data attribute so it reaches the parse action",
                resolved.display()
            );
            self.failures.borrow_mut().push(message.clone());
            message
        })
    }
}

/// A [`parser::ParseCtx`] for `doc_path` that can read its `:file:` data.
pub(super) fn parse_ctx(
    default_domain: rusty_sphinx_ast::Domain,
    csv_files: &DocumentRelativeCsvFiles,
) -> parser::ParseCtx<'_> {
    parser::ParseCtx::new(default_domain, csv_files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::Domain;

    #[test]
    fn test_for_document_uses_the_documents_own_directory() {
        // Given / When
        let loader = DocumentRelativeCsvFiles::for_document("docs/guide/index.rst");

        // Then
        assert_eq!(loader.base_dir, PathBuf::from("docs/guide"));
    }

    #[test]
    fn test_for_document_falls_back_to_the_current_directory() {
        // Given / When
        let loader = DocumentRelativeCsvFiles::for_document("index.rst");

        // Then
        assert_eq!(loader.base_dir, PathBuf::from("."));
    }

    #[test]
    fn test_load_reads_a_file_beside_the_document() {
        // Given
        let dir = std::env::temp_dir().join("rusty_sphinx_csv_files_read");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join("fruits.csv"), "Apple, Red\n").expect("write csv");
        let doc_path = dir.join("index.rst");
        let loader =
            DocumentRelativeCsvFiles::for_document(doc_path.to_str().expect("utf-8 temp path"));

        // When
        let data = loader.load("fruits.csv");

        // Then
        assert_eq!(data.as_deref(), Ok("Apple, Red\n"));
    }

    #[test]
    fn test_load_reports_a_missing_file_with_the_bazel_hint() {
        // Given
        let loader = DocumentRelativeCsvFiles::for_document("docs/index.rst");

        // When
        let result = loader.load("nowhere.csv");

        // Then
        let message = result.expect_err("a missing file must be an error");
        assert!(message.contains("docs/nowhere.csv"), "{message}");
        assert!(message.contains("csv_data"), "{message}");
    }

    #[test]
    fn test_failures_is_empty_before_any_read() {
        // Given / When
        let loader = DocumentRelativeCsvFiles::for_document("docs/index.rst");

        // Then
        assert!(loader.failures().is_empty());
    }

    #[test]
    fn test_failures_records_every_unreadable_path() {
        // Given
        let loader = DocumentRelativeCsvFiles::for_document("docs/index.rst");

        // When
        let _ = loader.load("first.csv");
        let _ = loader.load("second.csv");

        // Then
        let failures = loader.failures();
        assert_eq!(failures.len(), 2);
        assert!(failures[0].contains("docs/first.csv"), "{}", failures[0]);
        assert!(failures[1].contains("docs/second.csv"), "{}", failures[1]);
    }

    #[test]
    fn test_failures_stays_empty_after_a_successful_read() {
        // Given
        let dir = std::env::temp_dir().join("rusty_sphinx_csv_files_success");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join("fruits.csv"), "Apple, Red\n").expect("write csv");
        let doc_path = dir.join("index.rst");
        let loader =
            DocumentRelativeCsvFiles::for_document(doc_path.to_str().expect("utf-8 temp path"));

        // When
        let _ = loader.load("fruits.csv");

        // Then
        assert!(loader.failures().is_empty());
    }

    #[test]
    fn test_parse_ctx_carries_both_the_domain_and_the_loader() {
        // Given
        let loader = DocumentRelativeCsvFiles::for_document("docs/index.rst");

        // When
        let ctx = parse_ctx(Domain::C, &loader);

        // Then
        assert_eq!(ctx.default_domain, Domain::C);
        assert!(ctx.csv_files.load("nowhere.csv").is_err());
    }
}
