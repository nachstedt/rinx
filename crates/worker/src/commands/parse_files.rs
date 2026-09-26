//! The filesystem side of every file a directive reads while parsing.
//!
//! `rinx_parser` performs no I/O of its own, so it asks for a file's
//! contents through a [`parser::ParseFileLoader`]. This is the implementation
//! the CLI supplies: paths resolve against the directory of the file the
//! directive was written in — the document itself, or, for a directive inside
//! an included fragment, that fragment — exactly as docutils resolves them.
//!
//! Under Bazel that directory is inside the action's sandbox, so the file is
//! only there if it was declared — `rinx_library`'s `parse_data`
//! attribute is what puts it in the parse action's inputs.
//!
//! An unreadable file is recorded as well as diagnosed, because the parser
//! itself is deliberately error-resilient: it degrades the directive and
//! carries on, which is right for the live preview but would otherwise let a
//! build silently ship a page with a whole table or section missing.
//! [`DocumentRelativeFiles::failures`] is what lets the `parse` subcommand
//! turn that into a failed build, the same way a missing diagram image does.

use rinx_parser::{self as parser, LoadedFile, ParseFileLoader};
use std::cell::RefCell;

/// Reads paths relative to the file the directive was written in, remembering
/// the ones it could not read.
pub(super) struct DocumentRelativeFiles {
    /// The document being parsed, as a source-root-relative path. Every
    /// resolution is anchored here or at a file reached from here, so ids come
    /// out source-root-relative too — the same key `ImageUri::resolve` and the
    /// asset embedder produce.
    doc_path: String,
    /// `RefCell` because [`ParseFileLoader::load`] takes `&self` — the parser
    /// only ever borrows the loader immutably, and a parse is single-threaded.
    failures: RefCell<Vec<String>>,
}

impl DocumentRelativeFiles {
    /// Builds a loader for the document at `doc_path`.
    pub(super) fn for_document(doc_path: &str) -> Self {
        Self {
            doc_path: doc_path.to_string(),
            failures: RefCell::new(Vec::new()),
        }
    }

    /// The messages for every file this loader could not read, in the order
    /// they were requested. Empty when every read succeeded.
    pub(super) fn failures(&self) -> Vec<String> {
        self.failures.borrow().clone()
    }
}

impl ParseFileLoader for DocumentRelativeFiles {
    fn load(&self, path: &str, relative_to: Option<&str>) -> Result<LoadedFile, String> {
        // A directive inside an included fragment resolves against *that*
        // fragment, so a fragment can name its neighbours without knowing
        // which document pulled it in.
        let anchor = relative_to.unwrap_or(&self.doc_path);
        let resolved = rinx_ast::resolve_from_document(path, anchor);
        let id = resolved.to_string_lossy().into_owned();
        match std::fs::read_to_string(&resolved) {
            Ok(text) => Ok(LoadedFile { id, text }),
            Err(error) => {
                let message = format!(
                    "cannot read '{id}': {error} — if this is a Bazel build, add the file to \
                     the library's parse_data attribute so it reaches the parse action"
                );
                self.failures.borrow_mut().push(message.clone());
                Err(message)
            }
        }
    }
}

/// A [`parser::ParseCtx`] for a document that can read the files it names.
pub(super) fn parse_ctx(
    default_domain: rinx_ast::Domain,
    files: &DocumentRelativeFiles,
) -> parser::ParseCtx<'_> {
    parser::ParseCtx::new(default_domain, files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::Domain;
    use std::path::PathBuf;

    /// A scratch directory holding `files`, plus the source-root-relative
    /// document path to resolve against it.
    fn temp_document(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rinx_parse_files_{name}"));
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
        let dir = temp_document("read", &[("fruits.csv", "Apple, Red\n")]);
        let doc_path = dir.join("index.rst");
        let loader = DocumentRelativeFiles::for_document(doc_path.to_str().expect("utf-8 path"));

        // When
        let fragment = loader.load("fruits.csv", None).expect("read the file");

        // Then
        assert_eq!(fragment.text, "Apple, Red\n");
    }

    #[test]
    fn test_load_returns_the_resolved_path_as_the_id() {
        // Given
        let dir = temp_document("id", &[("shared/params.rst", "text\n")]);
        let doc_path = dir.join("guide/index.rst");
        let loader = DocumentRelativeFiles::for_document(doc_path.to_str().expect("utf-8 path"));

        // When — written relative to the document's own directory
        let fragment = loader
            .load("../shared/params.rst", None)
            .expect("read the file");

        // Then — `..` is resolved, so two spellings of one file share an id
        assert_eq!(
            fragment.id,
            dir.join("shared/params.rst").to_string_lossy().into_owned()
        );
    }

    #[test]
    fn test_load_resolves_a_nested_path_against_the_including_file() {
        // Given a fragment that names a neighbour of its own
        let dir = temp_document(
            "nested",
            &[
                ("shared/params.rst", "outer\n"),
                ("shared/detail.rst", "inner\n"),
            ],
        );
        let doc_path = dir.join("guide/index.rst");
        let loader = DocumentRelativeFiles::for_document(doc_path.to_str().expect("utf-8 path"));
        let outer = loader
            .load("../shared/params.rst", None)
            .expect("read the outer fragment");

        // When the fragment's own `.. include:: detail.rst` is resolved
        let inner = loader
            .load("detail.rst", Some(&outer.id))
            .expect("read the inner fragment");

        // Then — relative to the fragment, not to the document that pulled it
        assert_eq!(inner.text, "inner\n");
    }

    #[test]
    fn test_load_treats_a_leading_slash_as_the_source_root() {
        // Given — docutils' rule: `/` is the source root, not the filesystem's
        let dir = temp_document("rooted", &[("shared/params.rst", "text\n")]);
        let doc_path = dir.join("guide/index.rst");
        let loader = DocumentRelativeFiles::for_document(doc_path.to_str().expect("utf-8 path"));

        // When — the "source root" here is the temp dir's own root prefix
        let written = format!("/{}", dir.join("shared/params.rst").display());
        let fragment = loader.load(&written, None).expect("read the file");

        // Then
        assert_eq!(fragment.text, "text\n");
    }

    #[test]
    fn test_load_reports_a_missing_file_with_the_bazel_hint() {
        // Given
        let loader = DocumentRelativeFiles::for_document("docs/index.rst");

        // When
        let result = loader.load("nowhere.csv", None);

        // Then
        let message = result.err().expect("a missing file must be an error");
        assert!(message.contains("docs/nowhere.csv"), "{message}");
        assert!(message.contains("parse_data"), "{message}");
    }

    #[test]
    fn test_failures_is_empty_before_any_read() {
        // Given / When
        let loader = DocumentRelativeFiles::for_document("docs/index.rst");

        // Then
        assert!(loader.failures().is_empty());
    }

    #[test]
    fn test_failures_records_every_unreadable_path() {
        // Given
        let loader = DocumentRelativeFiles::for_document("docs/index.rst");

        // When
        let _ = loader.load("first.csv", None);
        let _ = loader.load("second.csv", None);

        // Then
        let failures = loader.failures();
        assert_eq!(failures.len(), 2);
        assert!(failures[0].contains("docs/first.csv"), "{}", failures[0]);
        assert!(failures[1].contains("docs/second.csv"), "{}", failures[1]);
    }

    #[test]
    fn test_failures_stays_empty_after_a_successful_read() {
        // Given
        let dir = temp_document("success", &[("fruits.csv", "Apple, Red\n")]);
        let doc_path = dir.join("index.rst");
        let loader = DocumentRelativeFiles::for_document(doc_path.to_str().expect("utf-8 path"));

        // When
        let _ = loader.load("fruits.csv", None);

        // Then
        assert!(loader.failures().is_empty());
    }

    #[test]
    fn test_parse_ctx_carries_both_the_domain_and_the_loader() {
        // Given
        let loader = DocumentRelativeFiles::for_document("docs/index.rst");

        // When
        let ctx = parse_ctx(Domain::C, &loader);

        // Then
        assert_eq!(ctx.default_domain, Domain::C);
        assert!(ctx.files.load("nowhere.csv", None).is_err());
    }
}
