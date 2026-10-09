//! Parsing and analysing every document of a workspace folder.
//!
//! Each document is parsed exactly as the server parses an open one — the
//! same [`parse_source`], from the disk — so the scan and the editor cannot
//! read a document differently. The documents are independent, so they are
//! parsed in parallel; what the scan returns is plain data, recorded into the
//! [`super::WorkspaceFolder`] by the server's own thread.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;

use super::discover::discover_sources;
use super::folder::IndexedDocument;
use crate::diagnostics::{ParsedSource, parse_source};
use crate::documents::DocumentStore;
use crate::uri::file_uri;

/// Every document under `root`, by its name within the folder, with its
/// analysis.
///
/// `on_progress` is called with the number of documents done and the total
/// after each one, from whichever thread finished it. A document that cannot
/// be read is left out: it is not part of the project until it can be.
pub fn scan_folder(
    root: &Path,
    on_progress: &(impl Fn(usize, usize) + Sync),
) -> Vec<(String, IndexedDocument)> {
    let sources = discover_sources(root);
    let total = sources.len();
    let done = AtomicUsize::new(0);
    // The scan sees the disk only: what the editor holds is recorded on the
    // server's thread, where the buffers are.
    let disk_only = DocumentStore::default();
    sources
        .par_iter()
        .filter_map(|path| {
            let scanned = parse_document(root, path, &disk_only).map(|(doc_path, parsed)| {
                (
                    doc_path,
                    IndexedDocument::of(&parsed.document, parsed.reads.paths),
                )
            });
            on_progress(done.fetch_add(1, Ordering::Relaxed) + 1, total);
            scanned
        })
        .collect()
}

/// Every document under `root` parsed as the scan parses it, for a test to
/// build the index from by other means.
#[cfg(test)]
pub fn parse_folder_documents(root: &Path) -> Vec<rinx_ast::Document> {
    let disk_only = DocumentStore::default();
    discover_sources(root)
        .iter()
        .filter_map(|path| parse_document(root, path, &disk_only))
        .map(|(_, parsed)| parsed.document)
        .collect()
}

/// The name of the document at `path` under `root` and its parse, read from
/// the disk, or `None` when it cannot be read.
fn parse_document(
    root: &Path,
    path: &Path,
    disk_only: &DocumentStore,
) -> Option<(String, ParsedSource)> {
    let text = std::fs::read_to_string(path).ok()?;
    let uri = file_uri(path)?;
    let doc_path = path
        .strip_prefix(root)
        .ok()?
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");
    let parsed = parse_source(&uri, Some(&doc_path), &text, disk_only);
    Some((doc_path, parsed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;

    fn temp_tree(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("rinx_lsp_scan_{name}"));
        let _ = fs::remove_dir_all(&root);
        for (file, text) in files {
            let path = root.join(file);
            fs::create_dir_all(path.parent().expect("a file has a parent")).expect("mkdir");
            fs::write(&path, text).expect("write");
        }
        root
    }

    #[test]
    fn test_scan_folder_names_every_document_within_the_folder() {
        // Given
        let root = temp_tree(
            "names",
            &[
                ("index.rst", "Home\n====\n"),
                ("guide/setup.rst", "Setup\n=====\n"),
            ],
        );

        // When
        let mut names: Vec<String> = scan_folder(&root, &|_, _| {})
            .into_iter()
            .map(|(doc_path, _)| doc_path)
            .collect();
        names.sort();

        // Then
        assert_eq!(names, ["guide/setup.rst", "index.rst"]);
    }

    #[test]
    fn test_scan_folder_records_the_files_a_document_includes() {
        // Given
        let root = temp_tree(
            "reads",
            &[
                ("index.rst", ".. include:: _shared/note.rst\n"),
                ("_shared/note.rst", "A note.\n"),
            ],
        );

        // When
        let scanned = scan_folder(&root, &|_, _| {});

        // Then
        let (_, index) = scanned
            .iter()
            .find(|(doc_path, _)| doc_path == "index.rst")
            .expect("index.rst is scanned");
        assert!(index.reads.contains(&root.join("_shared/note.rst")));
    }

    #[test]
    fn test_scan_folder_reports_progress_up_to_the_total() {
        // Given
        let root = temp_tree(
            "progress",
            &[("a.rst", "A\n"), ("b.rst", "B\n"), ("c.rst", "C\n")],
        );
        let seen = Mutex::new(Vec::new());

        // When
        scan_folder(&root, &|done, total| {
            seen.lock().expect("lock").push((done, total));
        });

        // Then
        let mut seen = seen.into_inner().expect("lock");
        seen.sort_unstable();
        assert_eq!(seen, [(1, 3), (2, 3), (3, 3)]);
    }
}
