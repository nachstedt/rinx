//! One project's documents and the project index they fold into.
//!
//! The project holds one [`DocumentAnalysis`] per document rather than a
//! merged index, because an index cannot forget: a label deleted in the
//! editor would stay resolvable. Replacing one document's analysis and
//! folding the map again forgets it for free (ADR-038 §3). The fold is cached
//! until a document changes, so a burst of edits pays for it once, when the
//! index is next asked for.
//!
//! A document leaves the project when its file is gone: deleted or renamed on
//! the disk, as a file-system watcher reports it. The scan may have read it
//! before that, so the project remembers what it forgot until the scan's
//! result is recorded, and does not let the scan bring it back.
//!
//! Which files are documents, what they are named and how the index is built
//! is the project's [`ProjectModel`]'s to say; no entity schema applies until
//! a Bazel manifest (roadmap #18) or the sphinx-needs settings (#15) name one.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rinx_analyzer::{DocumentAnalysis, build_project_index_from_analyses};
use rinx_ast::{Diagnostic, Document};
use rinx_entity::EntitySchema;
use rinx_index::ProjectIndex;
use rinx_renderer::config::SiteConfig;

use super::discover::DiscoveredProject;
use super::model::{DEFAULT_ROOT_DOC, ProjectModel, ProjectSource, RST_SUFFIX};
use crate::files::reads_at_or_under;

/// The root document Sphinx falls back to when `index` is missing
/// (`sphinx.config.check_master_doc`, its pre-2.0 default).
const OLD_ROOT_DOC: &str = "contents";

/// What the project knows about one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedDocument {
    /// The document's file.
    pub path: PathBuf,
    pub analysis: DocumentAnalysis,
    /// Every file the document's latest parse read, so an edit to one of
    /// them re-analyses the document.
    pub reads: BTreeSet<PathBuf>,
}

impl IndexedDocument {
    /// What the project records for `document`, the file at `path`, whose
    /// parse read `reads`.
    #[must_use]
    pub fn of(path: PathBuf, document: &Document, reads: BTreeSet<PathBuf>) -> Self {
        Self {
            path,
            analysis: DocumentAnalysis::of(document),
            reads,
        }
    }
}

/// A project the server indexes.
#[derive(Debug)]
pub struct Project {
    model: ProjectModel,
    /// What reading `conf.py` found, positioned in [`Self::conf_text`].
    findings: Vec<Diagnostic>,
    conf_text: String,
    /// The workspace folder the project was found in, by index.
    folder: usize,
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

impl Project {
    /// The project `discovered` in workspace folder `folder`, with none of
    /// its documents recorded yet.
    #[must_use]
    pub fn new(folder: usize, discovered: DiscoveredProject) -> Self {
        let DiscoveredProject {
            model,
            findings,
            conf_text,
            sources: _,
        } = discovered;
        Self {
            model,
            findings,
            conf_text,
            folder,
            documents: BTreeMap::new(),
            index: None,
            generation: 0,
            forgotten: BTreeSet::new(),
            scanned: false,
        }
    }

    #[must_use]
    pub fn model(&self) -> &ProjectModel {
        &self.model
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.model.source_root
    }

    /// The workspace folder the project was found in.
    #[must_use]
    pub fn folder(&self) -> usize {
        self.folder
    }

    /// The `conf.py` configuring the project, if it is a Sphinx project.
    #[must_use]
    pub fn conf_path(&self) -> Option<&Path> {
        match &self.model.source {
            ProjectSource::SphinxConf { conf } => Some(conf),
            ProjectSource::Folder => None,
        }
    }

    /// What reading `conf.py` found, and the text its positions count in.
    #[must_use]
    pub fn conf_findings(&self) -> (&[Diagnostic], &str) {
        (&self.findings, &self.conf_text)
    }

    /// Takes `findings`, and the `conf.py` text they are positioned in, as
    /// what reading the project's `conf.py` now finds.
    pub fn set_conf_findings(&mut self, findings: Vec<Diagnostic>, conf_text: String) {
        self.findings = findings;
        self.conf_text = conf_text;
    }

    /// The name of the document at `path` in this project, or `None` when
    /// `path` is no document of it — by its model's rules, or because
    /// another file is already the document of that name: of two files one
    /// document under two suffixes, the first by name is, as in Sphinx.
    #[must_use]
    pub fn doc_path_of(&self, path: &Path) -> Option<String> {
        let doc_path = self.model.doc_path_of(path)?;
        match self.documents.get(&doc_path) {
            Some(recorded) if recorded.path.as_path() < path => None,
            _ => Some(doc_path),
        }
    }

    /// The file of the document named `doc_path`.
    #[must_use]
    pub fn path_of(&self, doc_path: &str) -> PathBuf {
        self.documents.get(doc_path).map_or_else(
            || self.model.source_path(doc_path),
            |recorded| recorded.path.clone(),
        )
    }

    /// Records `indexed` as the latest analysis of the document `doc_path`.
    pub fn record(&mut self, doc_path: String, indexed: IndexedDocument) {
        self.forgotten.remove(&doc_path);
        if self.documents.get(&doc_path) != Some(&indexed) {
            self.documents.insert(doc_path, indexed);
            self.invalidate();
        }
    }

    /// Records each scanned document the project does not know yet. A
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

    /// Every document recorded, by its name, with its file.
    pub fn documents(&self) -> impl Iterator<Item = (&str, &Path)> {
        self.documents
            .iter()
            .map(|(doc_path, recorded)| (doc_path.as_str(), recorded.path.as_path()))
    }

    /// The documents recorded at or under `path` — a document, or a directory
    /// holding documents.
    #[must_use]
    pub fn documents_under(&self, path: &Path) -> Vec<String> {
        self.documents
            .iter()
            .filter(|(_, recorded)| recorded.path.starts_with(path))
            .map(|(doc_path, _)| doc_path.clone())
            .collect()
    }

    /// Which version of the project's documents the project index folds: it
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

    /// How many documents the project has recorded.
    #[must_use]
    pub fn document_count(&self) -> usize {
        self.documents.len()
    }

    /// The root document the project is indexed and rendered with: the
    /// configured one — except that a Sphinx project left at `index` with no
    /// such document but a `contents` one uses that, as Sphinx does.
    #[must_use]
    pub fn root_doc(&self) -> &str {
        let configured = self.model.settings.root_doc.as_str();
        let has = |name: &str| self.documents.contains_key(&format!("{name}{RST_SUFFIX}"));
        if matches!(self.model.source, ProjectSource::SphinxConf { .. })
            && configured == DEFAULT_ROOT_DOC
            && !has(DEFAULT_ROOT_DOC)
            && has(OLD_ROOT_DOC)
        {
            OLD_ROOT_DOC
        } else {
            configured
        }
    }

    /// The configuration the project's documents are rendered with.
    #[must_use]
    pub fn site_config(&self) -> SiteConfig {
        self.model.site_config(self.root_doc())
    }

    /// The project index the project's documents fold into.
    pub fn project_index(&mut self) -> &ProjectIndex {
        let root_doc = self.root_doc().to_string();
        let (documents, model) = (&self.documents, &self.model);
        self.index.get_or_insert_with(|| {
            let analyses: BTreeMap<String, DocumentAnalysis> = documents
                .iter()
                .map(|(doc_path, indexed)| (doc_path.clone(), indexed.analysis.clone()))
                .collect();
            build_project_index_from_analyses(
                &analyses,
                &model.index_settings(&root_doc),
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

    fn folder() -> Project {
        Project::new(0, DiscoveredProject::folder(PathBuf::from("/work/docs")))
    }

    /// The analysis of a document named `doc_path` defining the label `label`.
    fn labelled(doc_path: &str, label: &str) -> IndexedDocument {
        let document = rinx_parser::parse(doc_path, &format!(".. _{label}:\n\nText.\n"));
        IndexedDocument::of(
            Path::new("/work/docs").join(doc_path),
            &document,
            BTreeSet::new(),
        )
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

    /// A Sphinx project at `/work/docs` whose `conf.py` says `conf`.
    fn sphinx(conf: &str) -> Project {
        let reading = crate::project::sphinx_conf::read_sphinx_conf(conf);
        Project::new(
            1,
            DiscoveredProject {
                model: ProjectModel {
                    source_root: PathBuf::from("/work/docs"),
                    source: ProjectSource::SphinxConf {
                        conf: PathBuf::from("/work/docs/conf.py"),
                    },
                    settings: reading.settings,
                },
                findings: reading.findings,
                conf_text: conf.to_string(),
                sources: Vec::new(),
            },
        )
    }

    #[test]
    fn test_doc_path_of_keeps_the_first_file_of_a_document() {
        // Given — `intro` recorded from `intro.rst`
        let mut project = sphinx("source_suffix = ['.rst', '.txt']\n");
        project.record("intro.rst".to_string(), labelled("intro.rst", "a"));

        // When / Then — `intro.txt` comes after it by name
        assert_eq!(project.doc_path_of(Path::new("/work/docs/intro.txt")), None);
        assert_eq!(
            project.doc_path_of(Path::new("/work/docs/intro.rst")),
            Some("intro.rst".to_string())
        );
    }

    #[test]
    fn test_path_of_finds_a_recorded_documents_own_file() {
        // Given
        let mut project = sphinx("source_suffix = '.txt'\n");
        let document = rinx_parser::parse("intro.rst", "Text.\n");
        project.record(
            "intro.rst".to_string(),
            IndexedDocument::of(
                PathBuf::from("/work/docs/intro.txt"),
                &document,
                BTreeSet::new(),
            ),
        );

        // When / Then
        assert_eq!(
            project.path_of("intro.rst"),
            PathBuf::from("/work/docs/intro.txt")
        );
        assert_eq!(
            project.path_of("other.rst"),
            PathBuf::from("/work/docs/other.txt")
        );
    }

    #[test]
    fn test_root_doc_falls_back_to_contents_as_sphinx_does() {
        // Given — no `index`, but a `contents`
        let mut project = sphinx("project = 'P'\n");
        project.record("contents.rst".to_string(), labelled("contents.rst", "a"));

        // When / Then
        assert_eq!(project.root_doc(), "contents");
        assert_eq!(project.site_config().root_doc, "contents");
        assert_eq!(project.project_index().root_documents, ["contents.rst"]);
    }

    #[test]
    fn test_root_doc_keeps_index_when_it_exists_or_is_not_sphinxs() {
        // Given
        let mut project = sphinx("project = 'P'\n");
        project.record("contents.rst".to_string(), labelled("contents.rst", "a"));
        project.record("index.rst".to_string(), labelled("index.rst", "b"));
        let mut plain = folder();
        plain.record("contents.rst".to_string(), labelled("contents.rst", "a"));

        // When / Then
        assert_eq!(project.root_doc(), "index");
        assert_eq!(plain.root_doc(), "index");
        assert_eq!(sphinx("root_doc = 'start'\n").root_doc(), "start");
    }

    #[test]
    fn test_a_projects_conf_and_folder_are_its_own() {
        // Given
        let project = sphinx("root_doc = os.getenv('X')\n");

        // When
        let (findings, text) = project.conf_findings();

        // Then
        assert_eq!(project.folder(), 1);
        assert_eq!(project.root(), Path::new("/work/docs"));
        assert_eq!(project.conf_path(), Some(Path::new("/work/docs/conf.py")));
        assert_eq!(findings.len(), 1);
        assert!(text.starts_with("root_doc"));
        assert_eq!(folder().conf_path(), None);
        assert_eq!(project.model().settings.root_doc, "index");
    }
}
