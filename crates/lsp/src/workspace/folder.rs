//! One workspace folder's documents and the project index they fold into.
//!
//! The folder holds one [`DocumentAnalysis`] per document rather than a
//! merged index, because an index cannot forget: a label deleted in the
//! editor would stay resolvable. Replacing one document's analysis and
//! folding the map again forgets it for free (ADR-038 §3). The fold is cached
//! until a document changes, so a burst of edits pays for it once, when the
//! index is next asked for.
//!
//! A document leaves the folder when its file is gone: deleted or renamed on
//! the disk, as a file-system watcher reports it. The scan may have read it
//! before that, so the folder remembers what it forgot until the scan's
//! result is recorded, and does not let the scan bring it back.
//!
//! Until `conf.py` (roadmap #10) or a Bazel manifest (#18) says otherwise,
//! the folder is the source root: a document is named by its path within the
//! folder, `index` is the root document, and no entity schema applies.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rinx_analyzer::{DocumentAnalysis, IndexSettings, build_project_index_from_analyses};
use rinx_ast::Document;
use rinx_entity::EntitySchema;
use rinx_index::ProjectIndex;

use super::discover::is_source;
use crate::files::reads_at_or_under;

/// The root document of a folder with no configuration, as Sphinx defaults it.
const ROOT_DOC: &str = "index";

/// What the folder knows about one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedDocument {
    pub analysis: DocumentAnalysis,
    /// Every file the document's latest parse read, so an edit to one of
    /// them re-analyses the document.
    pub reads: BTreeSet<PathBuf>,
}

impl IndexedDocument {
    /// What the folder records for `document`, whose parse read `reads`.
    #[must_use]
    pub fn of(document: &Document, reads: BTreeSet<PathBuf>) -> Self {
        Self {
            analysis: DocumentAnalysis::of(document),
            reads,
        }
    }
}

/// A workspace folder the server indexes.
#[derive(Debug)]
pub struct WorkspaceFolder {
    root: PathBuf,
    documents: BTreeMap<String, IndexedDocument>,
    /// The fold of `documents`, until one of them changes.
    index: Option<ProjectIndex>,
    /// Counts the changes to `documents`, so a render can tell whether the
    /// index it read is still the index.
    generation: u64,
    /// The documents forgotten since the scan began, which its result must
    /// not record again.
    forgotten: BTreeSet<String>,
    /// Whether the scan's result is recorded, after which nothing need be
    /// remembered as forgotten.
    scanned: bool,
}

impl WorkspaceFolder {
    /// An empty folder at `root`, an absolute path.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            documents: BTreeMap::new(),
            index: None,
            generation: 0,
            forgotten: BTreeSet::new(),
            scanned: false,
        }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The name of the document at `path` within this folder — its path
    /// relative to the root, `/`-separated as the build names it — or `None`
    /// when `path` is not a document under this folder.
    #[must_use]
    pub fn doc_path_of(&self, path: &Path) -> Option<String> {
        if !is_source(path) {
            return None;
        }
        let relative = path.strip_prefix(&self.root).ok()?;
        let components: Vec<String> = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect();
        Some(components.join("/"))
    }

    /// The location of the document named `doc_path`.
    #[must_use]
    pub fn path_of(&self, doc_path: &str) -> PathBuf {
        doc_path
            .split('/')
            .fold(self.root.clone(), |path, component| path.join(component))
    }

    /// Records `indexed` as the latest analysis of the document `doc_path`.
    pub fn record(&mut self, doc_path: String, indexed: IndexedDocument) {
        self.forgotten.remove(&doc_path);
        if self.documents.get(&doc_path) != Some(&indexed) {
            self.documents.insert(doc_path, indexed);
            self.invalidate();
        }
    }

    /// Records each scanned document the folder does not know yet. A
    /// document already recorded was analysed after the scan read it from
    /// the disk — from its open buffer, or as the includer of an open file —
    /// so it is the more recent of the two. A document forgotten meanwhile is
    /// gone from the disk, so it is not recorded either.
    pub fn record_scanned(&mut self, scanned: impl IntoIterator<Item = (String, IndexedDocument)>) {
        let forgotten = std::mem::take(&mut self.forgotten);
        self.scanned = true;
        for (doc_path, indexed) in scanned {
            if forgotten.contains(&doc_path) {
                continue;
            }
            if let std::collections::btree_map::Entry::Vacant(entry) =
                self.documents.entry(doc_path)
            {
                entry.insert(indexed);
                self.invalidate();
            }
        }
    }

    /// Forgets the document `doc_path`, whose file is gone.
    pub fn forget(&mut self, doc_path: &str) {
        if self.documents.remove(doc_path).is_some() {
            self.invalidate();
        }
        if !self.scanned {
            self.forgotten.insert(doc_path.to_string());
        }
    }

    /// The documents recorded at or under `path` — a document, or a directory
    /// holding documents.
    #[must_use]
    pub fn documents_under(&self, path: &Path) -> Vec<String> {
        self.documents
            .keys()
            .filter(|doc_path| self.path_of(doc_path).starts_with(path))
            .cloned()
            .collect()
    }

    /// Which version of the folder's documents the project index folds: it
    /// changes whenever a document's analysis does, and only then.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Drops the cached fold, a document having changed.
    fn invalidate(&mut self) {
        self.index = None;
        self.generation += 1;
    }

    /// The documents whose latest parse read the file at `path`, or a file
    /// under it when `path` is a directory.
    #[must_use]
    pub fn includers_of(&self, path: &Path) -> Vec<String> {
        self.documents
            .iter()
            .filter(|(_, indexed)| reads_at_or_under(&indexed.reads, path))
            .map(|(doc_path, _)| doc_path.clone())
            .collect()
    }

    /// How many documents the folder has recorded.
    #[must_use]
    pub fn document_count(&self) -> usize {
        self.documents.len()
    }

    /// The project index the folder's documents fold into.
    pub fn project_index(&mut self) -> &ProjectIndex {
        let documents = &self.documents;
        self.index.get_or_insert_with(|| {
            let analyses: BTreeMap<String, DocumentAnalysis> = documents
                .iter()
                .map(|(doc_path, indexed)| (doc_path.clone(), indexed.analysis.clone()))
                .collect();
            build_project_index_from_analyses(
                &analyses,
                &IndexSettings::new(ROOT_DOC),
                &EntitySchema::empty(),
            )
            .index
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::TargetName;

    fn folder() -> WorkspaceFolder {
        WorkspaceFolder::new(PathBuf::from("/work/docs"))
    }

    /// The analysis of a document named `doc_path` defining the label `label`.
    fn labelled(doc_path: &str, label: &str) -> IndexedDocument {
        let document = rinx_parser::parse(doc_path, &format!(".. _{label}:\n\nText.\n"));
        IndexedDocument::of(&document, BTreeSet::new())
    }

    #[test]
    fn test_doc_path_of_names_a_document_by_its_path_in_the_folder() {
        // When / Then
        assert_eq!(
            folder().doc_path_of(Path::new("/work/docs/guide/setup.rst")),
            Some("guide/setup.rst".to_string())
        );
        assert_eq!(
            folder().doc_path_of(Path::new("/elsewhere/setup.rst")),
            None
        );
        assert_eq!(folder().doc_path_of(Path::new("/work/docs/data.csv")), None);
    }

    #[test]
    fn test_path_of_inverts_doc_path_of() {
        // When / Then
        assert_eq!(
            folder().path_of("guide/setup.rst"),
            PathBuf::from("/work/docs/guide/setup.rst")
        );
    }

    #[test]
    fn test_project_index_forgets_a_label_its_document_no_longer_defines() {
        // Given
        let mut folder = folder();
        folder.record("a.rst".to_string(), labelled("a.rst", "setup"));
        assert!(
            folder
                .project_index()
                .targets
                .contains_key(&TargetName::new("setup"))
        );

        // When — the label is renamed in the editor.
        folder.record("a.rst".to_string(), labelled("a.rst", "install"));

        // Then
        let index = folder.project_index();
        assert!(!index.targets.contains_key(&TargetName::new("setup")));
        assert!(index.targets.contains_key(&TargetName::new("install")));
    }

    #[test]
    fn test_record_scanned_keeps_a_more_recent_analysis() {
        // Given — `a.rst` analysed from its open buffer during the scan.
        let mut folder = folder();
        folder.record("a.rst".to_string(), labelled("a.rst", "edited"));

        // When
        folder.record_scanned([
            ("a.rst".to_string(), labelled("a.rst", "saved")),
            ("b.rst".to_string(), labelled("b.rst", "other")),
        ]);

        // Then
        let index = folder.project_index();
        assert!(index.targets.contains_key(&TargetName::new("edited")));
        assert!(!index.targets.contains_key(&TargetName::new("saved")));
        assert!(index.targets.contains_key(&TargetName::new("other")));
        assert_eq!(folder.document_count(), 2);
    }

    #[test]
    fn test_generation_changes_only_when_a_document_changes() {
        // Given
        let mut folder = folder();
        let start = folder.generation();

        // When — a document is recorded, then recorded again unchanged
        folder.record("a.rst".to_string(), labelled("a.rst", "setup"));
        let after_change = folder.generation();
        folder.record("a.rst".to_string(), labelled("a.rst", "setup"));

        // Then
        assert_ne!(after_change, start);
        assert_eq!(folder.generation(), after_change);
    }

    #[test]
    fn test_generation_changes_when_a_scan_brings_a_new_document() {
        // Given — `a.rst` already recorded from its open buffer
        let mut folder = folder();
        folder.record("a.rst".to_string(), labelled("a.rst", "edited"));
        let before = folder.generation();

        // When — the scan brings only `a.rst`, which is kept, then `b.rst`
        folder.record_scanned([("a.rst".to_string(), labelled("a.rst", "saved"))]);
        let after_known = folder.generation();
        folder.record_scanned([("b.rst".to_string(), labelled("b.rst", "other"))]);

        // Then
        assert_eq!(after_known, before);
        assert_ne!(folder.generation(), before);
    }

    #[test]
    fn test_forget_drops_a_document_from_the_index() {
        // Given
        let mut folder = folder();
        folder.record("a.rst".to_string(), labelled("a.rst", "setup"));
        let before = folder.generation();

        // When
        folder.forget("a.rst");

        // Then
        assert!(folder.project_index().targets.is_empty());
        assert_eq!(folder.document_count(), 0);
        assert_ne!(folder.generation(), before);
    }

    #[test]
    fn test_forget_of_an_unknown_document_changes_nothing() {
        // Given
        let mut folder = folder();
        let before = folder.generation();

        // When
        folder.forget("a.rst");

        // Then
        assert_eq!(folder.generation(), before);
    }

    #[test]
    fn test_record_scanned_does_not_bring_back_a_forgotten_document() {
        // Given — `a.rst` deleted while the scan, which had read it, ran
        let mut folder = folder();
        folder.forget("a.rst");

        // When
        folder.record_scanned([
            ("a.rst".to_string(), labelled("a.rst", "stale")),
            ("b.rst".to_string(), labelled("b.rst", "other")),
        ]);

        // Then
        let index = folder.project_index();
        assert!(!index.targets.contains_key(&TargetName::new("stale")));
        assert!(index.targets.contains_key(&TargetName::new("other")));
    }

    #[test]
    fn test_record_after_forget_lets_the_scan_keep_its_rule() {
        // Given — `a.rst` deleted, then created again, during the scan
        let mut folder = folder();
        folder.forget("a.rst");
        folder.record("a.rst".to_string(), labelled("a.rst", "recreated"));

        // When
        folder.record_scanned([("a.rst".to_string(), labelled("a.rst", "stale"))]);

        // Then — the newer analysis stays
        let index = folder.project_index();
        assert!(index.targets.contains_key(&TargetName::new("recreated")));
        assert!(!index.targets.contains_key(&TargetName::new("stale")));
    }

    #[test]
    fn test_documents_under_lists_a_directory_and_a_document() {
        // Given
        let mut folder = folder();
        folder.record("guide/a.rst".to_string(), labelled("guide/a.rst", "a"));
        folder.record("guide/b.rst".to_string(), labelled("guide/b.rst", "b"));
        folder.record("guides.rst".to_string(), labelled("guides.rst", "c"));

        // When / Then — `guides.rst` is not under `guide`
        assert_eq!(
            folder.documents_under(Path::new("/work/docs/guide")),
            ["guide/a.rst", "guide/b.rst"]
        );
        assert_eq!(
            folder.documents_under(Path::new("/work/docs/guides.rst")),
            ["guides.rst"]
        );
    }

    #[test]
    fn test_includers_of_lists_the_documents_that_read_a_file() {
        // Given
        let mut folder = folder();
        let fragment = PathBuf::from("/work/docs/_shared/note.rst");
        let mut includer = labelled("a.rst", "a");
        includer.reads.insert(fragment.clone());
        folder.record("a.rst".to_string(), includer);
        folder.record("b.rst".to_string(), labelled("b.rst", "b"));

        // When / Then — by the file, and by the directory holding it
        assert_eq!(folder.includers_of(&fragment), ["a.rst"]);
        assert_eq!(
            folder.includers_of(Path::new("/work/docs/_shared")),
            ["a.rst"]
        );
    }
}
