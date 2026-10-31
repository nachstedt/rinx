//! A document read back from its `.ast` is the document that was parsed.
//!
//! Every later build phase reads a `Document` from a `.ast`, while the
//! language server reads the one it just parsed. Were any of the document left
//! out of the file, the two would hold different documents, and code reading
//! the missing part would work in the editor and find nothing in the build.
//! What only the editor reads is the `rinx_ast::SourceMap` beside the
//! document instead, which has no wire form at all.

use std::path::{Path, PathBuf};

use rinx_ast::{Document, Domain};
use rinx_parser::{LoadedFile, ParseCtx, ParseFileLoader};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root exists")
}

/// Every reStructuredText document under `examples/` and `docs/`, sorted.
fn corpus(root: &Path) -> Vec<PathBuf> {
    fn collect(directory: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(directory).expect("a readable directory") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                collect(&path, found);
            } else if path.extension().is_some_and(|extension| extension == "rst") {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    collect(&root.join("examples"), &mut found);
    collect(&root.join("docs"), &mut found);
    found.sort();
    found
}

/// Reads a file a directive names from beside the document, or beside the
/// fragment that named it, as the parse action's loader does — so a document
/// with `.. include::`s round-trips with its included files' table filled.
struct DiskFiles {
    document: String,
}

impl ParseFileLoader for DiskFiles {
    fn load(&self, path: &str, relative_to: Option<&str>) -> Result<LoadedFile, String> {
        let resolved = rinx_ast::resolve_from_document(path, relative_to.unwrap_or(&self.document));
        let id = resolved.to_string_lossy().into_owned();
        std::fs::read_to_string(&resolved)
            .map(|text| LoadedFile { id, text })
            .map_err(|error| error.to_string())
    }
}

/// `document` parsed as the build's parse action parses it, and the
/// document the `.ast` it would write reads back as.
fn parsed_and_read_back(document: &Path) -> (Document, Document) {
    let text = std::fs::read_to_string(document).expect("a readable document");
    let files = DiskFiles {
        document: document.to_string_lossy().into_owned(),
    };
    let ctx = ParseCtx::new(Domain::Py, &files);
    let parsed = rinx_parser::parse(&files.document, &text, &ctx).document;
    let ast = serde_json::to_string(&parsed).expect("the document serializes");
    let read_back = serde_json::from_str(&ast).expect("the `.ast` deserializes");
    (parsed, read_back)
}

#[test]
fn test_every_corpus_document_reads_back_from_its_ast_as_it_was_parsed() {
    // Given
    let root = repository_root();
    let documents = corpus(&root);
    assert!(documents.len() > 50, "found only {documents:?}");

    // When
    let differing: Vec<String> = documents
        .iter()
        .filter(|document| {
            let (parsed, read_back) = parsed_and_read_back(document);
            parsed != read_back
        })
        .map(|document| document.display().to_string())
        .collect();

    // Then
    assert_eq!(differing, Vec::<String>::new());
}

#[test]
fn test_an_included_fragment_reads_back_with_its_file_table() {
    // Given the example document that includes fragments
    let including = repository_root().join("examples/includes.rst");

    // When
    let (parsed, read_back) = parsed_and_read_back(&including);

    // Then — the comparison above covers the include path too
    assert!(!parsed.source_files.is_empty(), "{}", including.display());
    assert_eq!(parsed, read_back);
}
