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

use lsp_server::{Message, RequestId, Response};
use lsp_types::{CompletionList, CompletionResponse, GotoDefinitionResponse, Hover, Range, Uri};
use rinx_ast::Diagnostic;
use rinx_entity::EntitySchema;
use rinx_renderer::config::SiteConfig;
use rinx_renderer::{ReferenceResolver, ReferenceTarget};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::handlers::publish_diagnostics;
use super::scan::ScanEvent;
use crate::completion::{completion_items, role_at};
use crate::definition::reference_definition;
use crate::diagnostics::{ParsedSource, diagnose_source, parse_source};
use crate::documents::DocumentStore;
use crate::hover::reference_hover;
use crate::includes::IncludeGraph;
use crate::position::{PositionEncoding, char_index_of, to_lsp_position, to_lsp_range};
use crate::progress::{IndexState, IndexStatus, ScanProgress};
use crate::reference_at::reference_at;
use crate::render::render_diagnostics;
use crate::uri::{file_path, file_uri};
use crate::workspace::{IndexedDocument, WorkspaceFolder};

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
    /// The workspace folders, each a project of its own.
    pub(super) folders: Vec<WorkspaceFolder>,
    /// Whether the client accepts `$/progress` reports.
    progress_supported: bool,
    /// Whether the client takes a definition as a link from the reference.
    definition_links: bool,
    /// The scan's progress while one runs.
    scan: Option<RunningScan>,
    /// The id the next request the server sends will carry.
    next_request_id: i32,
}

/// A document the graph diagnoses: its latest parse, and what rendering it
/// found, if it has been rendered since.
#[derive(Debug)]
struct TrackedDocument {
    parsed: ParsedSource,
    /// The workspace folder the document is in, whose index it renders
    /// against — `None` for one in no folder, which is never rendered.
    folder: Option<usize>,
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

/// A workspace scan under way.
#[derive(Debug)]
struct RunningScan {
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
            progress_supported: false,
            definition_links: false,
            scan: None,
            next_request_id: 1,
        }
    }

    /// This server, indexing a workspace folder at each of `roots`, absolute
    /// paths; `progress_supported` says whether the client takes
    /// `$/progress` reports.
    #[must_use]
    pub fn with_workspace(mut self, roots: Vec<PathBuf>, progress_supported: bool) -> Self {
        self.folders = roots.into_iter().map(WorkspaceFolder::new).collect();
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

    /// The root of every workspace folder, in the order scan events name
    /// them by.
    #[must_use]
    pub fn folder_roots(&self) -> Vec<PathBuf> {
        self.folders
            .iter()
            .map(|folder| folder.root().to_path_buf())
            .collect()
    }

    /// Starts the workspace scan, as the messages announcing it: a status
    /// saying the server is indexing, and a request to create a progress
    /// token. Nothing, with no workspace folder to scan.
    pub(super) fn start_scan(&mut self) -> Vec<Message> {
        if self.folders.is_empty() {
            return Vec::new();
        }
        let request_id = RequestId::from(self.next_request_id);
        self.next_request_id += 1;
        let (progress, mut messages) = ScanProgress::start(self.progress_supported, request_id);
        self.scan = Some(RunningScan {
            progress,
            pending: (0..self.folders.len()).collect(),
            elapsed: Duration::ZERO,
        });
        messages.push(self.status(IndexState::Indexing, None).notification());
        messages
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
                documents,
                elapsed,
            } => self.finish_folder(folder, documents, elapsed),
        }
    }

    /// Records a finished folder's documents, and once every folder is done
    /// ends the scan: the final progress report, the ready status, and the
    /// diagnostics of the open files a newly known document includes.
    fn finish_folder(
        &mut self,
        folder: usize,
        documents: Vec<(String, IndexedDocument)>,
        elapsed: Duration,
    ) -> Vec<Message> {
        if let Some(scanned) = self.folders.get_mut(folder) {
            scanned.record_scanned(documents);
        }
        let Some(scan) = self.scan.as_mut() else {
            return Vec::new();
        };
        scan.pending.remove(&folder);
        scan.elapsed = scan.elapsed.max(elapsed);
        if !scan.pending.is_empty() {
            return Vec::new();
        }
        let elapsed = scan.elapsed;
        let Some(mut scan) = self.scan.take() else {
            return Vec::new();
        };
        // "Indexed" means the project index exists: folded once here, it is
        // cached until a document changes.
        for folder in &mut self.folders {
            folder.project_index();
        }
        let mut messages = scan.progress.finish(self.document_count(), elapsed);
        messages.push(self.status(IndexState::Ready, Some(elapsed)).notification());
        messages.extend(self.diagnose_includers_of_open_documents());
        messages
    }

    /// The completion of the reference role at `position` in the open
    /// document `uri`, from its workspace folder's index — or `None` when the
    /// cursor is in no role completion answers for, or the document is not
    /// open or lies in no workspace folder.
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
        let (folder, doc_path) = self.locate(&file_path(uri)?)?;
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
        let index = self.folders[folder].project_index();
        Some(CompletionResponse::List(CompletionList {
            is_incomplete,
            items: completion_items(index, &doc_path, &context, range),
        }))
    }

    /// The hover for the reference at `position` in the open document `uri`:
    /// where the built page would link it, from its workspace folder's index
    /// — or `None` when the cursor is on no reference, the reference would be
    /// drawn broken, or the document is not open or lies in no workspace
    /// folder.
    pub(super) fn hover(&mut self, uri: &Uri, position: lsp_types::Position) -> Option<Hover> {
        let (range, target, folder) = self.reference_target_at(uri, position)?;
        let folder = &self.folders[folder];
        Some(reference_hover(&target, range, |doc_path| {
            file_uri(&folder.path_of(doc_path))
        }))
    }

    /// The definition of the reference at `position` in the open document
    /// `uri`: the file of the document the built page would link it to — or
    /// `None` wherever [`Self::hover`] has none, and for a reference leading
    /// to no document of the folder.
    pub(super) fn definition(
        &mut self,
        uri: &Uri,
        position: lsp_types::Position,
    ) -> Option<GotoDefinitionResponse> {
        let (range, target, folder) = self.reference_target_at(uri, position)?;
        let folder = &self.folders[folder];
        reference_definition(
            &target,
            range,
            |doc_path| file_uri(&folder.path_of(doc_path)),
            self.definition_links,
        )
    }

    /// The reference at `position` in the open document `uri` — its range,
    /// where it leads and the workspace folder whose index resolved it — or
    /// `None` when the cursor is on no reference, the reference would be
    /// drawn broken, or the document is not open or lies in no workspace
    /// folder.
    ///
    /// Read from the document's latest parse, which every change brings up
    /// to date before the next message is handled, so neither a hover nor a
    /// go-to-definition waits for the render tier.
    fn reference_target_at(
        &mut self,
        uri: &Uri,
        position: lsp_types::Position,
    ) -> Option<(Range, ReferenceTarget, usize)> {
        let text = &self.documents.get(uri)?.text;
        let line = text.lines().nth(position.line as usize).unwrap_or_default();
        let column = char_index_of(line, position.character, self.encoding) + 1;
        let at = rinx_ast::Position::new(position.line + 1, u32::try_from(column).ok()?);
        let tracked = self.tracked.get(uri)?;
        let folder = tracked.folder?;
        let document = &tracked.parsed.document;
        let reference = reference_at(document, at)?;
        let range = to_lsp_range(reference.span()?, text, self.encoding);
        // Until the server reads a project's configuration (roadmap #10,
        // #18), it resolves as a site with no `rinx.toml` and no entity
        // schema would, as the render tier does.
        let config = SiteConfig::default();
        let index = self.folders[folder].project_index();
        let target = ReferenceResolver::new(document, index, &config, EntitySchema::empty_ref())
            .resolve(reference)?;
        Some((range, target, folder))
    }

    /// The status of the workspace index, `state` and `elapsed` given.
    fn status(&self, state: IndexState, elapsed: Option<Duration>) -> IndexStatus {
        IndexStatus {
            state,
            documents: self.document_count(),
            elapsed_ms: elapsed
                .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)),
        }
    }

    /// The documents indexed, over every workspace folder.
    fn document_count(&self) -> usize {
        self.folders
            .iter()
            .map(WorkspaceFolder::document_count)
            .sum()
    }

    /// Re-diagnoses the document at `changed`, which was just opened, edited
    /// or closed, and every document that reads its file, as the
    /// notifications to send.
    ///
    /// `changed` itself is published last and always — even with nothing to
    /// report — so a client waiting for it knows every other publish this
    /// change caused came first.
    pub(super) fn refresh(&mut self, changed: &Uri) -> Vec<Message> {
        let mut includers: BTreeSet<Uri> = BTreeSet::new();
        if let Some(path) = file_path(changed) {
            includers.extend(self.graph.includers_of(&path));
            includers.extend(self.workspace_includers_of(&path));
        }
        includers.remove(changed);
        let mut affected = self.diagnose(changed);
        for includer in &includers {
            affected.extend(self.diagnose(includer));
        }
        affected.remove(changed);
        let mut messages: Vec<Message> = affected
            .into_iter()
            .filter_map(|uri| self.publish(uri, false))
            .collect();
        messages.extend(self.publish(changed.clone(), true));
        messages
    }

    /// Diagnoses every closed document that includes an open file and is not
    /// diagnosed yet — what a finished scan makes known — as the
    /// notifications to send.
    fn diagnose_includers_of_open_documents(&mut self) -> Vec<Message> {
        let open: Vec<PathBuf> = self
            .folders
            .iter()
            .flat_map(|folder| self.open_paths_under(folder.root()))
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
    fn open_paths_under(&self, root: &Path) -> Vec<PathBuf> {
        self.documents
            .uris()
            .filter_map(file_path)
            .filter(|path| path.starts_with(root))
            .collect()
    }

    /// The URI of every workspace document whose latest parse read the file
    /// at `path`.
    fn workspace_includers_of(&self, path: &Path) -> Vec<Uri> {
        self.folders
            .iter()
            .flat_map(|folder| {
                folder
                    .includers_of(path)
                    .into_iter()
                    .map(|doc_path| folder.path_of(&doc_path))
            })
            .filter_map(|includer| self.uri_of_path(&includer))
            .collect()
    }

    /// The URI the file at `path` goes by: the client's own when it is open,
    /// so a publish reaches the buffer the client knows.
    fn uri_of_path(&self, path: &Path) -> Option<Uri> {
        self.documents
            .uri_of(path)
            .cloned()
            .or_else(|| file_uri(path))
    }

    /// The workspace folder holding the document at `path`, by index, and the
    /// document's name in it. The innermost folder wins when folders nest.
    fn locate(&self, path: &Path) -> Option<(usize, String)> {
        self.folders
            .iter()
            .enumerate()
            .filter_map(|(index, folder)| Some((index, folder.doc_path_of(path)?, folder.root())))
            .max_by_key(|(_, _, root)| root.components().count())
            .map(|(index, doc_path, _)| (index, doc_path))
    }

    /// Parses the document at `uri` — its buffer when open, else the file
    /// when it is a workspace document — records it in its folder's index,
    /// and records its diagnosis while it is open or includes an open file.
    /// Returns the URIs whose published diagnostics may have changed.
    ///
    /// What the last render found is dropped with the parse it was found in:
    /// its positions count in the old text. It returns when the document is
    /// rendered again.
    fn diagnose(&mut self, uri: &Uri) -> BTreeSet<Uri> {
        let path = file_path(uri);
        let located = path.as_deref().and_then(|path| self.locate(path));
        let open = self
            .documents
            .get(uri)
            .map(|document| document.text.clone());
        let text = match (&open, &located, &path) {
            (Some(text), _, _) => text.clone(),
            (None, Some(_), Some(path)) => match std::fs::read_to_string(path) {
                Ok(text) => text,
                Err(_) => return self.untrack(uri),
            },
            _ => return self.untrack(uri),
        };
        let doc_path = located.as_ref().map(|(_, doc_path)| doc_path.as_str());
        let parsed = parse_source(uri, doc_path, &text, &self.documents);
        let folder = located.as_ref().map(|(folder, _)| *folder);
        if let Some((folder, doc_path)) = located {
            let indexed = IndexedDocument::of(&parsed.document, parsed.reads.paths.clone());
            self.folders[folder].record(doc_path, indexed);
        }
        let reads_open = parsed
            .reads
            .paths
            .iter()
            .any(|read| self.documents.uri_of(read).is_some());
        if open.is_none() && !reads_open {
            return self.untrack(uri);
        }
        self.parses += 1;
        self.tracked.insert(
            uri.clone(),
            TrackedDocument {
                parsed,
                folder,
                parse: self.parses,
                rendered: None,
            },
        );
        self.record_diagnosis(uri)
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
        let mut diagnosis = diagnose_source(
            uri,
            &tracked.parsed,
            rendered,
            &self.documents,
            self.encoding,
        );
        if self.documents.get(uri).is_none() {
            diagnosis.by_uri.remove(uri);
        }
        self.graph.record(uri.clone(), diagnosis)
    }

    /// Whether a document the server shows has not been rendered against its
    /// latest parse and its folder's current index.
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
        let Some((uri, folder, key)) = self.pending_render() else {
            return Vec::new();
        };
        let index = self.folders[folder].project_index();
        let Some(tracked) = self.tracked.get_mut(&uri) else {
            return Vec::new();
        };
        let diagnostics = render_diagnostics(&tracked.parsed.document, index);
        tracked.rendered = Some(Rendered { key, diagnostics });
        self.record_diagnosis(&uri)
            .into_iter()
            .filter_map(|uri| self.publish(uri, false))
            .collect()
    }

    /// The document due to be rendered next, with its folder and what the
    /// render would read: the one most recently parsed, which is the one the
    /// author is most likely looking at.
    fn pending_render(&self) -> Option<(Uri, usize, RenderKey)> {
        self.tracked
            .iter()
            .filter_map(|(uri, tracked)| {
                let folder = tracked.folder?;
                let key = RenderKey {
                    parse: tracked.parse,
                    generation: self.folders[folder].generation(),
                };
                let due = tracked
                    .rendered
                    .as_ref()
                    .is_none_or(|rendered| rendered.key != key);
                due.then(|| (uri.clone(), folder, key))
            })
            .max_by_key(|(_, _, key)| key.parse)
    }

    /// The notification publishing what `uri` shows now — or nothing, when it
    /// shows nothing and showed nothing before, unless `always`.
    fn publish(&mut self, uri: Uri, always: bool) -> Option<Message> {
        let diagnostics = self.graph.diagnostics_for(&uri);
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
