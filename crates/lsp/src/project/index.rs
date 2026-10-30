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
//!
//! Each analysis also holds where the document's definitions were written,
//! which no fold reads (`docs/decisions/045-definition-spans.md`): a
//! document whose definitions merely moved replaces its analysis without
//! folding again, so pressing Enter above a heading costs no fold.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use rinx_analyzer::{DocumentAnalysis, build_project_index_from_analyses};
use rinx_ast::{Diagnostic, Document, Position};
use rinx_index::{DefinitionKey, ExternalInventory, NameRange, ProjectIndex};
use rinx_renderer::config::SiteConfig;

use super::discover::DiscoveredProject;
use super::inventories::{Fetched, ProjectInventories};
use super::model::{DEFAULT_ROOT_DOC, ProjectModel, ProjectSource, RST_SUFFIX};
use super::references::{DocumentUses, Resolution, UseSite, UseTarget, resolve_uses};
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
    /// What the document wrote that uses a definition, which no fold reads.
    pub uses: DocumentUses,
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
            uses: DocumentUses::of(document),
        }
    }
}

/// Where a definition was written: a file, and the range in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionLocation {
    /// The file: the defining document's, or the `.. include::`d fragment's.
    pub path: PathBuf,
    /// The definition's first character.
    pub start: Position,
    /// Just past its last character.
    pub end: Position,
    /// Exactly the characters its name was written as, in the same file,
    /// when the parser knew them.
    pub name: Option<NameRange>,
}

/// Where a project's documents wrote their definitions, borrowed from the
/// project beside its index by [`Project::index_and_definitions`].
#[derive(Debug, Clone, Copy)]
pub struct Definitions<'a> {
    documents: &'a BTreeMap<String, IndexedDocument>,
    source_root: &'a Path,
}

impl Definitions<'_> {
    /// Where the document `doc_path` wrote the definition `key`, by its
    /// latest analysis — `None` when the project holds no such document, or
    /// the document no such definition.
    #[must_use]
    pub fn site(&self, doc_path: &str, key: &DefinitionKey) -> Option<DefinitionLocation> {
        let indexed = self.documents.get(doc_path)?;
        let site = indexed.analysis.definition_sites().get(key)?;
        let path = match &site.file {
            None => indexed.path.clone(),
            // The build names a fragment from the source root, the server's
            // loader by its absolute path; joining keeps the latter as it is.
            Some(file) => self.source_root.join(file),
        };
        Some(DefinitionLocation {
            path,
            start: site.start,
            end: site.end,
            name: site.name,
        })
    }

    /// The definition the document `doc_path` wrote at `position` in its own
    /// text, by its latest analysis — a label's line, a heading, a term, an
    /// object's or an entity's marker. An entity, which is a label too, is
    /// preferred to its label, since every reference to it names the entity,
    /// and a section is the last resort, since none names a section. `None`
    /// where it wrote none.
    #[must_use]
    pub fn at(&self, doc_path: &str, position: Position) -> Option<DefinitionKey> {
        self.documents
            .get(doc_path)?
            .analysis
            .definition_sites()
            .iter()
            .filter(|(_, site)| {
                site.file.is_none() && site.start <= position && position < site.end
            })
            .min_by_key(|(key, _)| match key {
                DefinitionKey::Entity(_) => 0,
                DefinitionKey::Section(_) => 2,
                _ => 1,
            })
            .map(|(key, _)| key.clone())
    }

    /// The fragment the document `doc_path` wrote the definition `key` in,
    /// named from the source root when it lies under it — `None` when the
    /// document wrote it itself, or wrote no such definition.
    #[must_use]
    pub fn fragment_of(&self, doc_path: &str, key: &DefinitionKey) -> Option<String> {
        let site = self.site(doc_path, key)?;
        if self.documents.get(doc_path)?.path == site.path {
            return None;
        }
        let shown = site
            .path
            .strip_prefix(self.source_root)
            .unwrap_or(&site.path);
        Some(shown.to_string_lossy().into_owned())
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
    /// Whether reading `conf.py` without running it fell short.
    wants_conf_run: bool,
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
    /// What each document's uses resolved to against `fold`, for the
    /// documents asked about since it or they last changed.
    uses: BTreeMap<String, Vec<(UseTarget, UseSite)>>,
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
            wants_conf_run,
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
            wants_conf_run,
            folder,
            documents: BTreeMap::new(),
            fold: None,
            generation: 0,
            forgotten: BTreeSet::new(),
            scanned: false,
            uses: BTreeMap::new(),
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
            ProjectSource::Folder | ProjectSource::Bazel { .. } => None,
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

    /// Takes what `discovered`, the project found again with the same
    /// model, found reading its `conf.py`: the findings, the text they are
    /// positioned in, and whether running it would make it exact.
    pub fn set_conf_reading(&mut self, discovered: &DiscoveredProject) {
        self.findings.clone_from(&discovered.findings);
        self.conf_text.clone_from(&discovered.conf_text);
        self.wants_conf_run = discovered.wants_conf_run;
    }

    /// The text of `conf.py` as last read, empty for a folder.
    #[must_use]
    pub fn conf_text(&self) -> &str {
        &self.conf_text
    }

    /// Whether reading the project's `conf.py` without running it fell short
    /// for a setting the server reads.
    #[must_use]
    pub fn wants_conf_run(&self) -> bool {
        self.wants_conf_run
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
    ///
    /// An analysis differing only in where its definitions were written
    /// replaces the recorded one without folding again.
    pub fn record(&mut self, doc_path: String, indexed: IndexedDocument) {
        self.forgotten.remove(&doc_path);
        let folds_alike = self.documents.get(&doc_path).is_some_and(|recorded| {
            recorded.path == indexed.path
                && recorded.reads == indexed.reads
                && recorded.analysis.folds_like(&indexed.analysis)
        });
        self.uses.remove(&doc_path);
        self.documents.insert(doc_path, indexed);
        if !folds_alike {
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
        self.uses.clear();
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

    /// Where the project's documents wrote their definitions, by their
    /// latest analyses — which no fold reads, so none is needed.
    #[must_use]
    pub fn definitions(&self) -> Definitions<'_> {
        Definitions {
            documents: &self.documents,
            source_root: &self.model.source_root,
        }
    }

    /// The project index, beside where its definitions were written, for a
    /// caller reading both at once.
    pub fn index_and_definitions(&mut self) -> (&ProjectIndex, Definitions<'_>) {
        let root_doc = self.root_doc().to_string();
        let fold = fold_documents(
            &mut self.fold,
            &self.documents,
            &self.model,
            &mut self.external,
            &root_doc,
        );
        let definitions = Definitions {
            documents: &self.documents,
            source_root: &self.model.source_root,
        };
        (&fold.index, definitions)
    }

    /// Where the project's documents use `target`, by their latest analyses,
    /// resolved against the project's index as their pages would resolve
    /// them: in document order, one document after another.
    ///
    /// What a document's uses resolved to is kept until the index changes or
    /// the document does, so asking again after an edit to prose resolves
    /// that one document's uses alone.
    pub fn uses_of(&mut self, target: &UseTarget) -> Vec<UseSite> {
        self.resolve_uses();
        self.uses
            .values()
            .flatten()
            .filter(|(used, _)| used == target)
            .map(|(_, site)| site.clone())
            .collect()
    }

    /// Resolves the uses of every document whose resolution is not kept, in
    /// parallel.
    fn resolve_uses(&mut self) {
        let root_doc = self.root_doc().to_string();
        let config = self.site_config();
        let fold = fold_documents(
            &mut self.fold,
            &self.documents,
            &self.model,
            &mut self.external,
            &root_doc,
        );
        let resolution = Resolution {
            index: &fold.index,
            config: &config,
            schema: &self.model.settings.parse.schema,
            source_root: &self.model.source_root,
        };
        let kept = &self.uses;
        let resolved: Vec<(String, Vec<(UseTarget, UseSite)>)> = self
            .documents
            .par_iter()
            .filter(|(doc_path, _)| !kept.contains_key(*doc_path))
            .map(|(doc_path, indexed)| {
                let uses = resolve_uses(doc_path, &indexed.path, &indexed.uses, &resolution);
                (doc_path.clone(), uses)
            })
            .collect();
        self.uses.extend(resolved);
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
        fold_documents(
            &mut self.fold,
            &self.documents,
            &self.model,
            &mut self.external,
            &root_doc,
        )
    }
}

/// `fold`, folding `documents` into it first if it holds none — over the
/// project's fields rather than the project, so a caller may keep borrowing
/// the documents beside the fold it returns.
fn fold_documents<'f>(
    fold: &'f mut Option<Fold>,
    documents: &BTreeMap<String, IndexedDocument>,
    model: &ProjectModel,
    external: &mut Vec<ExternalInventory>,
    root_doc: &str,
) -> &'f Fold {
    fold.get_or_insert_with(|| {
        let analyses: BTreeMap<String, DocumentAnalysis> = documents
            .iter()
            .map(|(doc_path, indexed)| (doc_path.clone(), indexed.analysis.clone()))
            .collect();
        let build = build_project_index_from_analyses(
            &analyses,
            &model.index_settings(root_doc),
            &model.settings.parse.schema,
        );
        let mut index = build.index;
        index
            .domain_indices
            .clone_from(&model.settings.domain_indices);
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

#[cfg(test)]
mod definition_site_tests;
#[cfg(test)]
mod fold_tests;
#[cfg(test)]
mod inventory_tests;
#[cfg(test)]
mod naming_tests;
#[cfg(test)]
mod references_tests;
#[cfg(test)]
mod test_support;
