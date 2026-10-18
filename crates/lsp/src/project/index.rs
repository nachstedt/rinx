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
//! The project also holds its `intersphinx_mapping` inventories as far as
//! they are loaded ([`ProjectInventories`]): the loaded ones join its index,
//! and the others lower what its [`Strictness`] reports. A change to either
//! counts as a change of the index, so every open document renders again.
//!
//! The fold also yields what only the whole project reveals — a toctree
//! naming a missing document, a label two documents define, a relation to an
//! entity no document declares — kept with the index it was found in, by
//! document, for the render tier to publish (issue #265).
//!
//! Which files are documents, what they are named and how the index is built
//! is the project's [`ProjectModel`]'s to say, the entity schema included.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rinx_analyzer::{DocumentAnalysis, build_project_index_from_analyses};
use rinx_ast::{Diagnostic, Document};
use rinx_index::{ExternalInventory, ProjectIndex};
use rinx_renderer::config::SiteConfig;

use super::discover::DiscoveredProject;
use super::inventories::{Fetched, ProjectInventories};
use super::model::{DEFAULT_ROOT_DOC, ProjectModel, ProjectSource, RST_SUFFIX};
use super::strictness::Strictness;
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

/// One fold of a project's documents: the index, and what building it found
/// about each document, by its name — kept together so the two cannot be of
/// different folds.
#[derive(Debug)]
struct Fold {
    index: ProjectIndex,
    diagnostics: BTreeMap<String, Vec<Diagnostic>>,
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
    fold: Option<Fold>,
    /// Counts the changes to `documents`, so a render can tell whether the
    /// index it read is still the index.
    generation: u64,
    /// The documents forgotten since the scan began, which its result must
    /// not record again.
    forgotten: BTreeSet<String>,
    /// Whether the scan's result is recorded, after which nothing need be
    /// remembered as forgotten.
    scanned: bool,
    inventories: ProjectInventories,
    /// The loaded inventories while no fold holds them: moved into each fold
    /// and back out when it is dropped, since copying Python's alone would
    /// cost every fold ~4 ms.
    external: Vec<ExternalInventory>,
    /// The model's strictness, knowing which inventories are not loaded.
    strictness: Strictness,
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
        let inventories = ProjectInventories::new(&model.settings.intersphinx);
        let strictness = model
            .settings
            .strictness
            .clone()
            .with_unloaded_inventories(inventories.unloaded());
        Self {
            inventories,
            external: Vec::new(),
            strictness,
            model,
            findings,
            conf_text,
            folder,
            documents: BTreeMap::new(),
            fold: None,
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

    /// What reading `conf.py` found, and loading the inventories it
    /// declares, in source order — and the text their positions count in.
    #[must_use]
    pub fn conf_findings(&self) -> (Vec<Diagnostic>, &str) {
        let mut findings = self.findings.clone();
        findings.extend(self.inventories.findings());
        findings.sort_by_key(|finding| finding.span.map(|span| span.start));
        (findings, &self.conf_text)
    }

    /// How the project's findings are reported, knowing which of its
    /// inventories are not loaded.
    #[must_use]
    pub fn strictness(&self) -> &Strictness {
        &self.strictness
    }

    /// Loads what of the project's inventories can be loaded now — files
    /// from the disk, downloads as `fetched` says (see
    /// [`ProjectInventories::advance`]) — returning whether any changed,
    /// which changes the index.
    pub fn advance_inventories(&mut self, fetched: &impl Fn(&str) -> Option<Fetched>) -> bool {
        let read = |path: &Path| std::fs::read(path);
        let changed = self
            .inventories
            .advance(&self.model.source_root, &read, fetched);
        if changed {
            self.inventories_changed();
        }
        changed
    }

    /// The addresses whose download the project's inventories wait for.
    #[must_use]
    pub fn awaited_inventories(&self) -> Vec<String> {
        self.inventories
            .awaited()
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    /// How many days a downloaded inventory stays fresh for this project.
    #[must_use]
    pub fn inventory_cache_limit_days(&self) -> i64 {
        self.model.settings.intersphinx.cache_limit_days
    }

    /// Reads every inventory read from the file at `path` again, as it
    /// changed on the disk — returning whether one is.
    pub fn reload_inventory_file(&mut self, path: &Path) -> bool {
        self.inventories
            .reset_reading(&self.model.source_root, path)
    }

    fn inventories_changed(&mut self) {
        self.strictness = self
            .model
            .settings
            .strictness
            .clone()
            .with_unloaded_inventories(self.inventories.unloaded());
        self.invalidate();
        self.external = self.inventories.loaded().cloned().collect();
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
        if let Some(fold) = self.fold.take() {
            self.external = fold.index.external_inventories;
        }
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
        &self.fold().index
    }

    /// What folding the project's documents found about the document
    /// `doc_path` — the build's `index` warnings about it, before any
    /// `.. noqa:` is applied.
    pub fn index_diagnostics(&mut self, doc_path: &str) -> &[Diagnostic] {
        self.fold()
            .diagnostics
            .get(doc_path)
            .map_or(&[], Vec::as_slice)
    }

    /// The fold of the project's documents, folding them first if a document
    /// changed since the last.
    fn fold(&mut self) -> &Fold {
        let root_doc = self.root_doc().to_string();
        let (documents, model, external) = (&self.documents, &self.model, &mut self.external);
        self.fold.get_or_insert_with(|| {
            let analyses: BTreeMap<String, DocumentAnalysis> = documents
                .iter()
                .map(|(doc_path, indexed)| (doc_path.clone(), indexed.analysis.clone()))
                .collect();
            let build = build_project_index_from_analyses(
                &analyses,
                &model.index_settings(&root_doc),
                &model.settings.parse.schema,
            );
            let mut index = build.index;
            // Whole rather than pruned to what the documents name, as the
            // build's index is: the next edit may name anything.
            index.external_inventories = std::mem::take(external);
            let mut diagnostics: BTreeMap<String, Vec<Diagnostic>> = BTreeMap::new();
            for reported in build.diagnostics {
                diagnostics
                    .entry(reported.source_path)
                    .or_default()
                    .extend(reported.diagnostics);
            }
            Fold { index, diagnostics }
        })
    }
}

#[cfg(test)]
mod fold_tests;
#[cfg(test)]
mod inventory_tests;
#[cfg(test)]
mod naming_tests;
#[cfg(test)]
mod test_support;
