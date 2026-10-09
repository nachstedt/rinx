//! What the server remembers between messages, and how one change is turned
//! into the diagnostics to publish and the workspace index to fold.
//!
//! A change re-parses the changed document and every document reading its
//! file — open or not — once each. That one parse feeds both what the
//! document shows and its entry in its workspace folder's index, so the editor
//! and the index cannot read a document differently. A closed document is
//! diagnosed only while it includes an open file, and then only for that
//! file: its own squiggles would describe text nobody is looking at.
//!
//! A completion reads the same index: the open buffer's latest analysis is
//! already in it, so a label typed a moment ago completes elsewhere. A hover
//! and a go-to-definition read it too, resolving the reference under the
//! cursor in the document's latest parse through the renderer's own
//! resolution.
//!
//! What only a render finds — a broken reference above all — is a second
//! tier (ADR-038 §9). It is computed for the documents the graph diagnoses,
//! one at a time, when the loop finds the client quiet, and against the index
//! of the document's workspace folder. Which renders are due is derived rather
//! than recorded: a document is due when its parse or its folder's index has
//! changed since it was last rendered, so a label added in one document
//! re-renders every other one shown, and nothing has to remember to say so.
//!
//! Each document belongs to a project — the nearest `conf.py` above it, or
//! its workspace folder — whose model says how it is named, parsed, indexed
//! and rendered (see [`super::projects`]). Until the scan has found a
//! folder's projects, the folder is one project with no configuration.
//!
//! The disk changes too, outside the editor: a file renamed in the Explorer,
//! a branch checked out. A client announcing dynamic registration is asked to
//! watch every file of the workspace, and an event is treated as an edit of
//! each document it touches — a source under a folder, a file a document
//! reads, or every document under a directory created or deleted — while an
//! open file's event is ignored, its buffer being the truth. Everything else
//! the client reports (build output, above all) is dropped. A document whose
//! file is gone leaves its project's index, so the references to it render
//! broken. An event touching a `conf.py` finds the folder's projects again.
//! A client without dynamic registration sees none of this until the
//! document is opened.

use lsp_server::{Message, Request, RequestId, Response};
use lsp_types::request::{RegisterCapability, Request as _};
use lsp_types::{
    CompletionList, CompletionResponse, DidChangeWatchedFilesRegistrationOptions, FileChangeType,
    FileEvent, FileSystemWatcher, GlobPattern, GotoDefinitionResponse, Hover, Range, Registration,
    RegistrationParams, Uri,
};
use rinx_ast::Diagnostic;
use rinx_entity::EntitySchema;
use rinx_renderer::{ReferenceResolver, ReferenceTarget};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rayon::prelude::*;

use super::handlers::publish_diagnostics;
use super::projects::ProjectId;
use super::scan::ScanEvent;
use crate::completion::{completion_items, role_at};
use crate::definition::reference_definition;
use crate::diagnostics::{ParsedSource, diagnose_source, parse_source};
use crate::documents::DocumentStore;
use crate::hover::reference_hover;
use crate::includes::IncludeGraph;
use crate::position::{PositionEncoding, char_index_of, to_lsp_position, to_lsp_range};
use crate::progress::{IndexState, ScanProgress};
use crate::project::{
    DiscoveredProject, IndexedDocument, ParseSettings, Project, ScannedProject, Strictness,
    is_visible_under, sources_under,
};
use crate::reference_at::reference_at;
use crate::render::render_diagnostics;
use crate::uri::{file_path, file_uri};

/// The id the file watcher is registered under.
const WATCH_REGISTRATION: &str = "rinx/watch";

/// Everything the server remembers between messages.
#[derive(Debug)]
pub struct ServerState {
    pub(super) encoding: PositionEncoding,
    pub(super) documents: DocumentStore,
    /// The latest diagnosis of every open document, and of every closed one
    /// that includes an open file.
    pub(super) graph: IncludeGraph,
    /// The URIs last published with at least one diagnostic — the ones an
    /// empty publish must reach to clear.
    pub(super) shown: HashSet<Uri>,
    /// The latest parse of every document the graph diagnoses, and what
    /// rendering it found.
    tracked: HashMap<Uri, TrackedDocument>,
    /// How many parses the server has recorded, which orders them.
    parses: u64,
    /// The root of every workspace folder.
    pub(super) folders: Vec<PathBuf>,
    /// The folders whose projects have been found, by the scan or since.
    pub(super) discovered: BTreeSet<usize>,
    /// Every project of every folder, by an id no other project ever has.
    pub(super) projects: BTreeMap<ProjectId, Project>,
    /// The id the next project found gets.
    pub(super) next_project: u64,
    /// Each `conf.py` whose unmodelled extensions the user has been told
    /// about, with the extensions named — so a project found again says
    /// nothing new, and one declaring others says so.
    pub(super) announced: BTreeSet<(PathBuf, Vec<String>)>,
    /// How long the last workspace scan took, once it has finished.
    pub(super) scan_elapsed: Option<Duration>,
    /// Whether the client accepts `$/progress` reports.
    progress_supported: bool,
    /// Whether the client takes a definition as a link from the reference.
    definition_links: bool,
    /// Whether the client watches files for the server when asked to.
    watched_files: bool,
    /// The scan's progress while one runs.
    pub(super) scan: Option<RunningScan>,
    /// The id the next request the server sends will carry.
    next_request_id: i32,
}

/// A document the graph diagnoses: its latest parse, and what rendering it
/// found, if it has been rendered since.
#[derive(Debug)]
struct TrackedDocument {
    parsed: ParsedSource,
    /// The project the document is in, whose index it renders against —
    /// `None` for one in no project, which is never rendered.
    project: Option<ProjectId>,
    /// Which parse this is, counted over every document.
    parse: u64,
    rendered: Option<Rendered>,
}

/// What rendering a document found, and what it rendered.
#[derive(Debug)]
struct Rendered {
    key: RenderKey,
    diagnostics: Vec<Diagnostic>,
}

/// The inputs of a render: one parse of the document, and one version of its
/// folder's index. A render is due when either changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RenderKey {
    parse: u64,
    generation: u64,
}

/// What reading one document found.
#[derive(Debug)]
struct Reading {
    /// The document's file, if it has one.
    path: Option<PathBuf>,
    /// The project holding the document and its name there, if any.
    located: Option<(ProjectId, String)>,
    /// The parse, or `None` with no text to parse: a workspace document whose
    /// file is gone, or a file that is no document at all.
    parsed: Option<ParsedSource>,
    /// Whether the text parsed was the document's open buffer.
    open: bool,
}

/// A workspace scan under way.
#[derive(Debug)]
pub(super) struct RunningScan {
    progress: ScanProgress,
    /// The folders whose scan has not finished yet.
    pending: BTreeSet<usize>,
    /// How long the scan has run, as of the last folder to finish.
    elapsed: Duration,
}

impl ServerState {
    /// A server that has agreed on `encoding` and has no document open and
    /// no workspace folder.
    #[must_use]
    pub fn new(encoding: PositionEncoding) -> Self {
        Self {
            encoding,
            documents: DocumentStore::default(),
            graph: IncludeGraph::default(),
            shown: HashSet::new(),
            tracked: HashMap::new(),
            parses: 0,
            folders: Vec::new(),
            discovered: BTreeSet::new(),
            projects: BTreeMap::new(),
            next_project: 0,
            announced: BTreeSet::new(),
            scan_elapsed: None,
            progress_supported: false,
            definition_links: false,
            watched_files: false,
            scan: None,
            next_request_id: 1,
        }
    }

    /// This server, indexing a workspace folder at each of `roots`, absolute
    /// paths; `progress_supported` says whether the client takes
    /// `$/progress` reports. Each folder is one project with no
    /// configuration until the scan finds its projects.
    #[must_use]
    pub fn with_workspace(mut self, roots: Vec<PathBuf>, progress_supported: bool) -> Self {
        for (folder, root) in roots.iter().enumerate() {
            self.add_project(folder, DiscoveredProject::folder(root.clone()));
        }
        self.folders = roots;
        self.progress_supported = progress_supported;
        self
    }

    /// This server, answering a go-to-definition with links from the whole
    /// reference when `definition_links`, as a client announcing
    /// `textDocument.definition.linkSupport` takes them.
    #[must_use]
    pub fn with_definition_links(mut self, definition_links: bool) -> Self {
        self.definition_links = definition_links;
        self
    }

    /// This server, asking the client to watch the workspace's files when
    /// `watched_files`, as a client announcing
    /// `workspace.didChangeWatchedFiles.dynamicRegistration` does on request.
    #[must_use]
    pub fn with_watched_files(mut self, watched_files: bool) -> Self {
        self.watched_files = watched_files;
        self
    }

    /// The root of every workspace folder, in the order scan events name
    /// them by.
    #[must_use]
    pub fn folder_roots(&self) -> Vec<PathBuf> {
        self.folders.clone()
    }

    /// Starts the workspace scan, as the messages announcing it: a status
    /// saying the server is indexing, and a request to create a progress
    /// token. Nothing, with no workspace folder to scan.
    pub(super) fn start_scan(&mut self) -> Vec<Message> {
        if self.folders.is_empty() {
            return Vec::new();
        }
        let request_id = self.next_request_id();
        let (progress, mut messages) = ScanProgress::start(self.progress_supported, request_id);
        self.scan = Some(RunningScan {
            progress,
            pending: (0..self.folders.len()).collect(),
            elapsed: Duration::ZERO,
        });
        messages.push(self.status(IndexState::Indexing, None).notification());
        messages
    }

    /// The request asking the client to watch every file of the workspace —
    /// or nothing, for a client that cannot be asked or a server with no
    /// workspace folder.
    ///
    /// Every file rather than the sources alone: an `.. include::`d fragment
    /// may have any extension, and a directory renamed or deleted is reported
    /// as one event naming the directory, which a `**/*.rst` watcher would
    /// never see. [`Self::on_watched_files`] drops what touches no document.
    pub(super) fn watch_files(&mut self) -> Vec<Message> {
        if !self.watched_files || self.folders.is_empty() {
            return Vec::new();
        }
        let options = DidChangeWatchedFilesRegistrationOptions {
            watchers: vec![FileSystemWatcher {
                glob_pattern: GlobPattern::String("**/*".to_string()),
                kind: None,
            }],
        };
        let params = RegistrationParams {
            registrations: vec![Registration {
                id: WATCH_REGISTRATION.to_string(),
                method: "workspace/didChangeWatchedFiles".to_string(),
                register_options: serde_json::to_value(options).ok(),
            }],
        };
        let id = self.next_request_id();
        vec![Request::new(id, RegisterCapability::METHOD.to_string(), params).into()]
    }

    /// The id for the next request the server sends.
    fn next_request_id(&mut self) -> RequestId {
        let id = RequestId::from(self.next_request_id);
        self.next_request_id += 1;
        id
    }

    /// The messages a reply from the client brings about.
    pub(super) fn on_response(&mut self, response: &Response) -> Vec<Message> {
        self.scan
            .as_mut()
            .map(|scan| scan.progress.on_response(response))
            .unwrap_or_default()
    }

    /// The messages one event of the workspace scan brings about.
    pub(super) fn on_scan_event(&mut self, event: ScanEvent) -> Vec<Message> {
        match event {
            ScanEvent::Projects { folder, projects } => {
                if self.discovered.contains(&folder) {
                    return Vec::new();
                }
                self.install_projects(folder, projects)
            }
            ScanEvent::Progress {
                folder,
                done,
                total,
            } => self
                .scan
                .as_mut()
                .map(|scan| scan.progress.on_progress(folder, done, total))
                .unwrap_or_default(),
            ScanEvent::Finished {
                folder,
                projects,
                elapsed,
            } => self.finish_folder(folder, projects, elapsed),
        }
    }

    /// Records a finished folder's documents — into the projects the scan
    /// found, unless the folder's projects were found again since, in which
    /// case a project the folder no longer has the same way is left alone —
    /// and once every folder is done ends the scan: the final progress
    /// report, the ready status, and the diagnostics of the open files a
    /// newly known document includes.
    fn finish_folder(
        &mut self,
        folder: usize,
        scanned: Vec<ScannedProject>,
        elapsed: Duration,
    ) -> Vec<Message> {
        let mut messages = Vec::new();
        if !self.discovered.contains(&folder) {
            let found = scanned
                .iter()
                .map(|project| project.discovered.clone())
                .collect();
            messages.extend(self.install_projects(folder, found));
        }
        for project in scanned {
            if let Some(recorded) = self.project_with_model(folder, &project.discovered) {
                recorded.record_scanned(project.documents);
            }
        }
        let Some(scan) = self.scan.as_mut() else {
            return messages;
        };
        scan.pending.remove(&folder);
        scan.elapsed = scan.elapsed.max(elapsed);
        if !scan.pending.is_empty() {
            return messages;
        }
        let elapsed = scan.elapsed;
        let Some(mut scan) = self.scan.take() else {
            return messages;
        };
        self.scan_elapsed = Some(elapsed);
        // "Indexed" means the project index exists: folded once here, it is
        // cached until a document changes.
        for project in self.projects.values_mut() {
            project.project_index();
        }
        messages.extend(scan.progress.finish(self.document_count(), elapsed));
        messages.push(self.status(IndexState::Ready, Some(elapsed)).notification());
        messages.extend(self.diagnose_includers_of_open_documents());
        messages
    }

    /// The completion of the reference role at `position` in the open
    /// document `uri`, from its project's index — or `None` when the cursor
    /// is in no role completion answers for, or the document is not open or
    /// is in no project.
    ///
    /// The list is incomplete while the workspace scan runs, so the client
    /// asks again rather than filtering a list missing most documents.
    pub(super) fn complete(
        &mut self,
        uri: &Uri,
        position: lsp_types::Position,
    ) -> Option<CompletionResponse> {
        let text = &self.documents.get(uri)?.text;
        let line = text.lines().nth(position.line as usize).unwrap_or_default();
        let context = role_at(line, char_index_of(line, position.character, self.encoding))?;
        let (project, doc_path) = self.locate(&file_path(uri)?)?;
        let at = |index: usize| {
            let column = u32::try_from(index + 1).unwrap_or(u32::MAX);
            to_lsp_position(
                rinx_ast::Position::new(position.line + 1, column),
                line,
                self.encoding,
            )
        };
        let range = lsp_types::Range::new(at(context.replace.start), at(context.replace.end));
        let is_incomplete = self.scan.is_some();
        let index = self.projects.get_mut(&project)?.project_index();
        Some(CompletionResponse::List(CompletionList {
            is_incomplete,
            items: completion_items(index, &doc_path, &context, range),
        }))
    }

    /// The hover for the reference at `position` in the open document `uri`:
    /// where the built page would link it, from its project's index — or
    /// `None` when the cursor is on no reference, the reference would be
    /// drawn broken, or the document is not open or is in no project.
    pub(super) fn hover(&mut self, uri: &Uri, position: lsp_types::Position) -> Option<Hover> {
        let (range, target, project) = self.reference_target_at(uri, position)?;
        let project = self.projects.get(&project)?;
        Some(reference_hover(&target, range, |doc_path| {
            file_uri(&project.path_of(doc_path))
        }))
    }

    /// The definition of the reference at `position` in the open document
    /// `uri`: the file of the document the built page would link it to — or
    /// `None` wherever [`Self::hover`] has none, and for a reference leading
    /// to no document of the project.
    pub(super) fn definition(
        &mut self,
        uri: &Uri,
        position: lsp_types::Position,
    ) -> Option<GotoDefinitionResponse> {
        let (range, target, project) = self.reference_target_at(uri, position)?;
        let project = self.projects.get(&project)?;
        reference_definition(
            &target,
            range,
            |doc_path| file_uri(&project.path_of(doc_path)),
            self.definition_links,
        )
    }

    /// The reference at `position` in the open document `uri` — its range,
    /// where it leads and the project whose index resolved it — or `None`
    /// when the cursor is on no reference, the reference would be drawn
    /// broken, or the document is not open or is in no project.
    ///
    /// Read from the document's latest parse, which every change brings up
    /// to date before the next message is handled, so neither a hover nor a
    /// go-to-definition waits for the render tier.
    fn reference_target_at(
        &mut self,
        uri: &Uri,
        position: lsp_types::Position,
    ) -> Option<(Range, ReferenceTarget, ProjectId)> {
        let text = &self.documents.get(uri)?.text;
        let line = text.lines().nth(position.line as usize).unwrap_or_default();
        let column = char_index_of(line, position.character, self.encoding) + 1;
        let at = rinx_ast::Position::new(position.line + 1, u32::try_from(column).ok()?);
        let tracked = self.tracked.get(uri)?;
        let id = tracked.project?;
        let document = &tracked.parsed.document;
        let reference = reference_at(document, at)?;
        let range = to_lsp_range(reference.span()?, text, self.encoding);
        // With the project's configuration, as the render tier resolves; no
        // entity schema applies until roadmap #15 and #18.
        let project = self.projects.get_mut(&id)?;
        let config = project.site_config();
        let index = project.project_index();
        let target = ReferenceResolver::new(document, index, &config, EntitySchema::empty_ref())
            .resolve(reference)?;
        Some((range, target, id))
    }

    /// Re-diagnoses the document at `changed`, which was just opened, edited
    /// or closed, and every document that reads its file, as the
    /// notifications to send.
    ///
    /// `changed` itself is published last and always — even with nothing to
    /// report — so a client waiting for it knows every other publish this
    /// change caused came first.
    pub(super) fn refresh(&mut self, changed: &Uri) -> Vec<Message> {
        let mut affected = self.rediagnose(&BTreeSet::from([changed.clone()]));
        affected.remove(changed);
        let mut messages: Vec<Message> = affected
            .into_iter()
            .filter_map(|uri| self.publish(uri, false))
            .collect();
        messages.extend(self.publish(changed.clone(), true));
        messages
    }

    /// Re-diagnoses the documents every file-system event in `changes`
    /// touches, as the notifications to send. Each is re-read once, however
    /// many events name it, and only what changed is published.
    ///
    /// An event touching a `conf.py` — the file itself, or a directory
    /// created or deleted with one inside — finds its folder's projects
    /// again first, since which files are documents may have changed.
    pub(super) fn on_watched_files(&mut self, changes: &[FileEvent]) -> Vec<Message> {
        let reconfigured: BTreeSet<usize> = changes
            .iter()
            .filter_map(|event| self.folder_reconfigured_by(event))
            .collect();
        let mut messages: Vec<Message> = reconfigured
            .into_iter()
            .flat_map(|folder| self.reload_folder(folder))
            .collect();
        let touched: BTreeSet<Uri> = changes
            .iter()
            .flat_map(|event| self.touched_by(event))
            .collect();
        if touched.is_empty() {
            return messages;
        }
        let affected = self.rediagnose(&touched);
        messages.extend(
            affected
                .into_iter()
                .filter_map(|uri| self.publish(uri, false)),
        );
        messages
    }

    /// The URIs a file-system `event` makes worth re-reading: the file itself
    /// when it is a document or some document reads it (or a file under it),
    /// and every document under a directory created or deleted. Nothing for
    /// an open file, whose buffer the disk does not change.
    fn touched_by(&self, event: &FileEvent) -> Vec<Uri> {
        let Some(path) = file_path(&event.uri) else {
            return Vec::new();
        };
        if self.documents.uri_of(&path).is_some() {
            return Vec::new();
        }
        let mut touched = Vec::new();
        let is_document = self.locate(&path).is_some();
        let is_read = !self.graph.includers_of(&path).is_empty()
            || !self.workspace_includers_of(&path).is_empty();
        if is_document || is_read {
            touched.extend(self.uri_of_path(&path));
        }
        let under: Vec<PathBuf> = if event.typ == FileChangeType::CREATED && is_directory(&path) {
            let visible = self
                .folders
                .iter()
                .any(|root| is_visible_under(root, &path));
            self.project_holding(&path)
                .filter(|_| visible)
                .map(|project| sources_under(project.model(), &path))
                .unwrap_or_default()
                .into_iter()
                .map(|(_, source)| source)
                .collect()
        } else if event.typ == FileChangeType::DELETED {
            self.projects
                .values()
                .flat_map(|project| {
                    project
                        .documents_under(&path)
                        .into_iter()
                        .map(|doc_path| project.path_of(&doc_path))
                })
                .collect()
        } else {
            Vec::new()
        };
        touched.extend(
            under
                .iter()
                .filter(|document| self.documents.uri_of(document).is_none())
                .filter_map(|document| self.uri_of_path(document)),
        );
        touched
    }

    /// Re-diagnoses every document in `changed` and every document that
    /// reads one of their files, each once, returning the URIs whose
    /// published diagnostics may have changed.
    fn rediagnose(&mut self, changed: &BTreeSet<Uri>) -> BTreeSet<Uri> {
        let mut includers: BTreeSet<Uri> = BTreeSet::new();
        for uri in changed {
            if let Some(path) = file_path(uri) {
                includers.extend(self.graph.includers_of(&path));
                includers.extend(self.workspace_includers_of(&path));
            }
        }
        let documents: Vec<&Uri> = changed
            .iter()
            .chain(includers.difference(changed))
            .collect();
        // Reading is the expensive part and changes nothing, so a batch — a
        // branch checked out — is read in parallel; recording stays in order.
        let readings: Vec<Reading> = documents
            .par_iter()
            .map(|uri| self.read_document(uri))
            .collect();
        let mut affected = BTreeSet::new();
        for (uri, reading) in documents.into_iter().zip(readings) {
            affected.extend(self.record_reading(uri, reading));
        }
        affected
    }

    /// Diagnoses every closed document that includes an open file and is not
    /// diagnosed yet — what a finished scan makes known — as the
    /// notifications to send.
    pub(super) fn diagnose_includers_of_open_documents(&mut self) -> Vec<Message> {
        let open: Vec<PathBuf> = self
            .folders
            .iter()
            .flat_map(|root| self.open_paths_under(root))
            .collect();
        let mut includers: BTreeSet<Uri> = BTreeSet::new();
        for path in &open {
            includers.extend(self.workspace_includers_of(path));
        }
        let mut affected = BTreeSet::new();
        for includer in includers {
            if self.documents.get(&includer).is_none() {
                affected.extend(self.diagnose(&includer));
            }
        }
        affected
            .into_iter()
            .filter_map(|uri| self.publish(uri, false))
            .collect()
    }

    /// The location of every open document under `root`.
    pub(super) fn open_paths_under(&self, root: &Path) -> Vec<PathBuf> {
        self.documents
            .uris()
            .filter_map(file_path)
            .filter(|path| path.starts_with(root))
            .collect()
    }

    /// The URI of every workspace document whose latest parse read the file
    /// at `path`.
    fn workspace_includers_of(&self, path: &Path) -> Vec<Uri> {
        self.projects
            .values()
            .flat_map(|project| {
                project
                    .includers_of(path)
                    .into_iter()
                    .map(|doc_path| project.path_of(&doc_path))
            })
            .filter_map(|includer| self.uri_of_path(&includer))
            .collect()
    }

    /// The URI the file at `path` goes by: the client's own when it is open,
    /// so a publish reaches the buffer the client knows.
    pub(super) fn uri_of_path(&self, path: &Path) -> Option<Uri> {
        self.documents
            .uri_of(path)
            .cloned()
            .or_else(|| file_uri(path))
    }

    /// Parses the document at `uri` and records what it found: see
    /// [`Self::read_document`] and [`Self::record_reading`]. Returns the URIs
    /// whose published diagnostics may have changed.
    pub(super) fn diagnose(&mut self, uri: &Uri) -> BTreeSet<Uri> {
        let reading = self.read_document(uri);
        self.record_reading(uri, reading)
    }

    /// Parses the document at `uri` — its buffer when open, else the file
    /// when it is a workspace document. Reads the server's state and the disk
    /// but changes nothing, so a batch of documents is read in parallel.
    fn read_document(&self, uri: &Uri) -> Reading {
        let path = file_path(uri);
        let located = path.as_deref().and_then(|path| self.locate(path));
        let buffer = self.documents.get(uri);
        let text = match (buffer, &located, &path) {
            (Some(document), _, _) => Some(document.text.clone()),
            (None, Some(_), Some(path)) => std::fs::read_to_string(path).ok(),
            _ => None,
        };
        let doc_path = located.as_ref().map(|(_, doc_path)| doc_path.as_str());
        let default_settings = ParseSettings::default();
        let settings = located
            .as_ref()
            .and_then(|(project, _)| self.projects.get(project))
            .map_or(&default_settings, |project| &project.model().settings.parse);
        let parsed = text.map(|text| parse_source(uri, doc_path, &text, settings, &self.documents));
        Reading {
            path,
            located,
            parsed,
            open: buffer.is_some(),
        }
    }

    /// Records what reading the document at `uri` found: its analysis in its
    /// project's index, and its diagnosis while it is open or includes an
    /// open file. A closed workspace document whose file is gone leaves the
    /// index.
    /// Returns the URIs whose published diagnostics may have changed.
    ///
    /// What the last render found is dropped with the parse it was found in:
    /// its positions count in the old text. It returns when the document is
    /// rendered again.
    fn record_reading(&mut self, uri: &Uri, reading: Reading) -> BTreeSet<Uri> {
        let Reading {
            path,
            located,
            parsed,
            open,
        } = reading;
        let Some(parsed) = parsed else {
            if let Some((project, doc_path)) = located
                && let Some(project) = self.projects.get_mut(&project)
            {
                project.forget(&doc_path);
            }
            return self.untrack(uri);
        };
        let project = located.as_ref().map(|(project, _)| *project);
        if let (Some((id, doc_path)), Some(path)) = (located, path)
            && let Some(project) = self.projects.get_mut(&id)
        {
            let indexed = IndexedDocument::of(path, &parsed.document, parsed.reads.paths.clone());
            project.record(doc_path, indexed);
        }
        let reads_open = parsed
            .reads
            .paths
            .iter()
            .any(|read| self.documents.uri_of(read).is_some());
        if !open && !reads_open {
            return self.untrack(uri);
        }
        self.parses += 1;
        self.tracked.insert(
            uri.clone(),
            TrackedDocument {
                parsed,
                project,
                parse: self.parses,
                rendered: None,
            },
        );
        self.record_diagnosis(uri)
    }

    /// The documents the graph diagnoses whose file is under `root`.
    pub(super) fn tracked_under(&self, root: &Path) -> Vec<Uri> {
        self.tracked
            .keys()
            .filter(|uri| file_path(uri).is_some_and(|path| path.starts_with(root)))
            .cloned()
            .collect()
    }

    /// Re-diagnoses every document in `uris` and what reads them, as the
    /// notifications to send.
    pub(super) fn rediagnose_and_publish(&mut self, uris: &BTreeSet<Uri>) -> Vec<Message> {
        if uris.is_empty() {
            return Vec::new();
        }
        self.rediagnose(uris)
            .into_iter()
            .filter_map(|uri| self.publish(uri, false))
            .collect()
    }

    /// Stops diagnosing the document at `uri`, returning the URIs whose
    /// published diagnostics may have changed.
    fn untrack(&mut self, uri: &Uri) -> BTreeSet<Uri> {
        self.tracked.remove(uri);
        self.graph.forget(uri)
    }

    /// Records in the graph what the tracked document at `uri` found — its
    /// latest parse, and its render while that is of the same parse — and
    /// returns the URIs whose published diagnostics may have changed.
    ///
    /// A closed document shows nothing of its own: it is diagnosed only for
    /// the open files it includes.
    fn record_diagnosis(&mut self, uri: &Uri) -> BTreeSet<Uri> {
        let Some(tracked) = self.tracked.get(uri) else {
            return self.graph.forget(uri);
        };
        let rendered = tracked
            .rendered
            .as_ref()
            .map_or(&[][..], |rendered| rendered.diagnostics.as_slice());
        // A document in no project declares no extensions, so nothing about
        // it is lowered.
        let default_strictness = Strictness::default();
        let strictness = tracked
            .project
            .and_then(|project| self.projects.get(&project))
            .map_or(&default_strictness, |project| {
                &project.model().settings.strictness
            });
        let mut diagnosis = diagnose_source(
            uri,
            &tracked.parsed,
            rendered,
            strictness,
            &self.documents,
            self.encoding,
        );
        if self.documents.get(uri).is_none() {
            diagnosis.by_uri.remove(uri);
        }
        self.graph.record(uri.clone(), diagnosis)
    }

    /// Whether a document the server shows has not been rendered against its
    /// latest parse and its project's current index.
    pub(super) fn has_pending_renders(&self) -> bool {
        self.pending_render().is_some()
    }

    /// Renders the document most recently parsed among those due, and
    /// publishes what changed — nothing when none is due.
    ///
    /// One document per call, so the loop can answer the client between two
    /// renders: a render reads the current parse and index whenever it runs,
    /// so no render's result can be stale when it is recorded.
    pub(super) fn render_next(&mut self) -> Vec<Message> {
        let Some((uri, project, key)) = self.pending_render() else {
            return Vec::new();
        };
        let Some(project) = self.projects.get_mut(&project) else {
            return Vec::new();
        };
        let config = project.site_config();
        let index = project.project_index();
        let Some(tracked) = self.tracked.get_mut(&uri) else {
            return Vec::new();
        };
        let diagnostics = render_diagnostics(&tracked.parsed.document, index, &config);
        tracked.rendered = Some(Rendered { key, diagnostics });
        self.record_diagnosis(&uri)
            .into_iter()
            .filter_map(|uri| self.publish(uri, false))
            .collect()
    }

    /// The document due to be rendered next, with its project and what the
    /// render would read: the one most recently parsed, which is the one the
    /// author is most likely looking at.
    fn pending_render(&self) -> Option<(Uri, ProjectId, RenderKey)> {
        self.tracked
            .iter()
            .filter_map(|(uri, tracked)| {
                let id = tracked.project?;
                let key = RenderKey {
                    parse: tracked.parse,
                    generation: self.projects.get(&id)?.generation(),
                };
                let due = tracked
                    .rendered
                    .as_ref()
                    .is_none_or(|rendered| rendered.key != key);
                due.then(|| (uri.clone(), id, key))
            })
            .max_by_key(|(_, _, key)| key.parse)
    }

    /// The notification publishing what `uri` shows now — or nothing, when it
    /// shows nothing and showed nothing before, unless `always`. A `conf.py`
    /// shows what reading it found.
    pub(super) fn publish(&mut self, uri: Uri, always: bool) -> Option<Message> {
        let mut diagnostics = self.graph.diagnostics_for(&uri);
        diagnostics.extend(self.conf_diagnostics_for(&uri));
        if diagnostics.is_empty() {
            if !self.shown.remove(&uri) && !always {
                return None;
            }
        } else {
            self.shown.insert(uri.clone());
        }
        let version = self.documents.get(&uri).map(|document| document.version);
        Some(publish_diagnostics(uri, diagnostics, version))
    }
}

/// Whether `path` is a directory itself, not a link to one — a symlinked
/// directory is not followed, as the scan does not follow one.
fn is_directory(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
}
