//! Parsing and analysing every document of a workspace folder's projects.
//!
//! Each document is parsed exactly as the server parses an open one — the
//! same [`parse_source`], from the disk, with its project's settings — so the
//! scan and the editor cannot read a document differently. The documents are
//! independent, so they are parsed in parallel, every project's at once;
//! what the scan returns is plain data, recorded into each [`super::Project`]
//! by the server's own thread.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;

use super::discover::DiscoveredProject;
#[cfg(test)]
use super::discover::discover_projects;
use super::index::IndexedDocument;
use super::model::ParseSettings;
use crate::diagnostics::{ParsedSource, parse_source};
use crate::documents::DocumentStore;
use crate::uri::file_uri;

/// One project and its scanned documents.
#[derive(Debug, Clone, PartialEq)]
pub struct ScannedProject {
    pub discovered: DiscoveredProject,
    /// Every document by its name, with its analysis.
    pub documents: Vec<(String, IndexedDocument)>,
}

/// Every project under the workspace folder at `root`, with its documents
/// scanned: [`discover_projects`], then [`scan_projects`] — as the scan
/// thread does, for a test to stand in for it.
#[cfg(test)]
pub fn scan_folder(
    root: &Path,
    on_progress: &(impl Fn(usize, usize) + Sync),
) -> Vec<ScannedProject> {
    scan_projects(discover_projects(root), on_progress)
}

/// Parses and analyses the documents of `projects`.
///
/// `on_progress` is called with the number of documents done and the total
/// after each one, from whichever thread finished it. A document that cannot
/// be read is left out: it is not part of the project until it can be.
pub fn scan_projects(
    projects: Vec<DiscoveredProject>,
    on_progress: &(impl Fn(usize, usize) + Sync),
) -> Vec<ScannedProject> {
    let jobs: Vec<(usize, &str, &Path)> = projects
        .iter()
        .enumerate()
        .flat_map(|(project, discovered)| {
            discovered
                .sources
                .iter()
                .map(move |(doc_path, path)| (project, doc_path.as_str(), path.as_path()))
        })
        .collect();
    let total = jobs.len();
    let done = AtomicUsize::new(0);
    // The scan sees the disk only: what the editor holds is recorded on the
    // server's thread, where the buffers are.
    let disk_only = DocumentStore::default();
    let parsed: Vec<(usize, String, IndexedDocument)> = jobs
        .par_iter()
        .filter_map(|&(project, doc_path, path)| {
            let settings = &projects[project].model.settings.parse;
            let scanned = parse_document(doc_path, path, settings, &disk_only).map(|parsed| {
                (
                    project,
                    doc_path.to_string(),
                    IndexedDocument::of(path.to_path_buf(), &parsed.document, parsed.reads.paths),
                )
            });
            on_progress(done.fetch_add(1, Ordering::Relaxed) + 1, total);
            scanned
        })
        .collect();
    let mut scanned: Vec<ScannedProject> = projects
        .into_iter()
        .map(|discovered| ScannedProject {
            discovered,
            documents: Vec::new(),
        })
        .collect();
    for (project, doc_path, indexed) in parsed {
        scanned[project].documents.push((doc_path, indexed));
    }
    scanned
}

/// Every document of `project` parsed as the scan parses it, for a test to
/// build the index from by other means.
#[cfg(test)]
pub fn parse_project_documents(project: &DiscoveredProject) -> Vec<rinx_ast::Document> {
    let disk_only = DocumentStore::default();
    project
        .sources
        .iter()
        .filter_map(|(doc_path, path)| {
            parse_document(doc_path, path, &project.model.settings.parse, &disk_only)
        })
        .map(|parsed| parsed.document)
        .collect()
}

/// The parse of the document `doc_path`, the file at `path`, read from the
/// disk — or `None` when it cannot be read.
fn parse_document(
    doc_path: &str,
    path: &Path,
    settings: &ParseSettings,
    disk_only: &DocumentStore,
) -> Option<ParsedSource> {
    let text = std::fs::read_to_string(path).ok()?;
    let uri = file_uri(path)?;
    Some(parse_source(
        &uri,
        Some(doc_path),
        &text,
        settings,
        disk_only,
    ))
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

    fn names(project: &ScannedProject) -> Vec<String> {
        let mut names: Vec<String> = project
            .documents
            .iter()
            .map(|(doc_path, _)| doc_path.clone())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn test_scan_folder_names_every_document_within_its_project() {
        // Given
        let root = temp_tree(
            "names",
            &[
                ("index.rst", "Home\n====\n"),
                ("guide/setup.rst", "Setup\n=====\n"),
                ("docs/conf.py", ""),
                ("docs/index.rst", "Docs\n====\n"),
            ],
        );

        // When
        let scanned = scan_folder(&root, &|_, _| {});

        // Then
        assert_eq!(scanned.len(), 2);
        assert_eq!(names(&scanned[0]), ["guide/setup.rst", "index.rst"]);
        assert_eq!(names(&scanned[1]), ["index.rst"]);
        assert_eq!(scanned[1].documents[0].1.path, root.join("docs/index.rst"));
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
        let (_, index) = scanned[0]
            .documents
            .iter()
            .find(|(doc_path, _)| doc_path == "index.rst")
            .expect("index.rst is scanned");
        assert!(index.reads.contains(&root.join("_shared/note.rst")));
    }

    #[test]
    fn test_scan_folder_parses_with_the_projects_settings() {
        // Given — `py` would read `.. function::` as a Python function
        let root = temp_tree(
            "settings",
            &[
                ("conf.py", "primary_domain = 'c'\n"),
                ("index.rst", ".. function:: int f(void)\n"),
            ],
        );

        // When
        let scanned = scan_folder(&root, &|_, _| {});

        // Then
        let documents = parse_project_documents(&scanned[0].discovered);
        let index = rinx_analyzer::build_project_index(
            &documents,
            "index",
            &rinx_entity::EntitySchema::empty(),
        );
        let types = &index.domain_objects[&rinx_ast::TargetName::new("f")];
        assert!(
            types
                .keys()
                .all(|kind| kind.domain() == rinx_ast::Domain::C)
        );
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
