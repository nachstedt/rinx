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
    CodeAction, CompletionList, CompletionResponse, DidChangeWatchedFilesRegistrationOptions,
    FileChangeType, FileEvent, FileSystemWatcher, FoldingRange, GlobPattern,
    GotoDefinitionResponse, Hover, Range, Registration, RegistrationParams, SemanticTokens, Uri,
};
use rinx_ast::{EntityId, TargetName};
use rinx_index::DefinitionKey;
use rinx_renderer::{Destination, ReferenceResolver, ReferenceTarget};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rayon::prelude::*;

use super::bazel_manifests::{BazelManifests, ManifestSource};
use super::conf_runs::{ConfExecution, ConfRunEvent, ConfRuns};
use super::handlers::publish_diagnostics;
use super::inventories::InventoryFetches;
use super::projects::ProjectId;
use super::renames::EditShape;
use super::scan::ScanEvent;
use crate::code_action::quick_fixes;
use crate::completion::{
    DirectiveContext, DirectiveVocabulary, ItemStyle, PLACEHOLDER, RoleContext, RoleVocabulary,
    TargetQuestion, TextFormat, WrittenRole, directive_at, directive_items, identity_marker_line,
    is_scoped, on_option_marker, probe_text, relation_target_at, role_at, role_name_at,
    role_name_items, target_items,
};
use crate::definition::{line_link, reference_definition, site_location, target_location};
use crate::diagnostics::{
    ParsedSource, diagnose_source, parse_prefix, parse_source, text_through_line,
};
use crate::document_symbol::{DocumentSymbols, FlatSymbol, OutlineShape, document_symbols};
use crate::documents::DocumentStore;
use crate::entity_at::entity_marked_at;
use crate::entity_hover::{declared_entity_markdown, indexed_entity_markdown};
use crate::folding_range::folding_ranges;
use crate::hover::{location_markdown, markdown_hover, reference_hover, targets_hover};
use crate::includes::IncludeGraph;
use crate::position::{PositionEncoding, char_index_of, to_lsp_position, to_lsp_range};
use crate::preview::{
    PreviewRefusal, PreviewSettings, ResourceRoot, render_preview, render_preview_alone,
};
use crate::preview_link::{PageHref, PreviewLinkKind, PreviewLinkResult, resolve_page_href};
use crate::progress::{IndexState, ScanProgress};
use crate::project::{
    DiscoveredProject, IndexedDocument, ParseSettings, Project, ScannedProject, Strictness,
    UseTarget, is_visible_under, sources_under,
};
use crate::reference_at::{reference_at, toc_entry_at};
use crate::references::{ordered_locations, use_location};
use crate::render::{RenderFindings, RenderSettings, render_diagnostics};
use crate::semantic_tokens::semantic_tokens;
use crate::uri::{file_path, file_uri};
use crate::workspace_symbol::{best_matches, workspace_symbol};

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
    /// How the client takes a workspace edit.
    pub(super) edit_shape: EditShape,
    /// How the client takes a document's outline.
    outline_shape: OutlineShape,
    /// The most folding ranges the client takes for one document, when it
    /// sets a limit.
    folding_limit: Option<usize>,
    /// How the client takes a completion item: its text's format, and
    /// whether it reopens completion when an item says to.
    completion: ItemStyle,
    /// Whether the client watches files for the server when asked to.
    watched_files: bool,
    /// The scan's progress while one runs.
    pub(super) scan: Option<RunningScan>,
    /// The id the next request the server sends will carry.
    next_request_id: i32,
    /// The inventory downloads asked of the client.
    pub(super) fetches: InventoryFetches,
    /// The `conf.py` runs asked for, under way and finished.
    pub(super) conf_runs: ConfRuns,
    /// The Bazel manifests asked for and granted.
    pub(super) bazel: BazelManifests,
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

/// What rendering a document found — with what folding its project found
/// about it, which the render read — and what it rendered.
#[derive(Debug)]
struct Rendered {
    key: RenderKey,
    findings: RenderFindings,
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
            edit_shape: EditShape::ByUri,
            outline_shape: OutlineShape::Flat,
            folding_limit: None,
            completion: ItemStyle {
                format: TextFormat::Plain,
                reopens: false,
            },
            watched_files: false,
            scan: None,
            next_request_id: 1,
            fetches: InventoryFetches::default(),
            conf_runs: ConfRuns::default(),
            bazel: BazelManifests::default(),
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

    /// This server, answering a rename with a workspace edit in
    /// `edit_shape`.
    #[must_use]
    pub fn with_edit_shape(mut self, edit_shape: EditShape) -> Self {
        self.edit_shape = edit_shape;
        self
    }

    /// This server, answering for a document's outline in `outline_shape`.
    #[must_use]
    pub fn with_outline_shape(mut self, outline_shape: OutlineShape) -> Self {
        self.outline_shape = outline_shape;
        self
    }

    /// This server, answering at most `folding_limit` folding ranges for one
    /// document when the client sets a limit.
    #[must_use]
    pub fn with_folding_limit(mut self, folding_limit: Option<usize>) -> Self {
        self.folding_limit = folding_limit;
        self
    }

    /// This server, writing a completion's text in `completion_format`: a
    /// directive's name as a snippet for a client taking one.
    #[must_use]
    pub fn with_completion_format(mut self, completion_format: TextFormat) -> Self {
        self.completion.format = completion_format;
        self
    }

    /// This server, asking the client to reopen completion after an option
    /// whose values complete when `reopens`, as a client passing the
    /// `reopenCompletion` initialization option does.
    #[must_use]
    pub fn with_completion_reopening(mut self, reopens: bool) -> Self {
        self.completion.reopens = reopens;
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

    /// This server, asking the client to download the inventories its
    /// projects declare when `fetch_inventories`, as a client passing the
    /// `fetchInventories` initialization option does on request.
    #[must_use]
    pub fn with_inventory_fetching(mut self, fetch_inventories: bool) -> Self {
        self.fetches = InventoryFetches::new(fetch_inventories);
        self
    }

    /// This server, running each `conf.py` that reading falls short for in
    /// the interpreter `execution` comes by (see [`super::conf_runs`]).
    #[must_use]
    pub fn with_conf_execution(mut self, execution: ConfExecution) -> Self {
        self.conf_runs = ConfRuns::new(execution);
        self
    }

    /// This server, reading the Bazel sites of its folders from the
    /// manifests `source` comes by (see [`super::bazel_manifests`]).
    #[must_use]
    pub fn with_bazel_manifests(mut self, source: ManifestSource) -> Self {
        self.bazel = BazelManifests::new(source);
        self
    }

    /// This server, running `conf.py` through `runs` — for tests needing a
    /// timeout of their own.
    #[cfg(test)]
    pub(super) fn with_conf_runs(mut self, runs: ConfRuns) -> Self {
        self.conf_runs = runs;
        self
    }

    /// Where the results of the `conf.py` runs arrive, for the loop to pass
    /// each to [`Self::on_conf_run`].
    #[must_use]
    pub fn conf_run_events(&self) -> crossbeam_channel::Receiver<ConfRunEvent> {
        self.conf_runs.events()
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
    pub(super) fn next_request_id(&mut self) -> RequestId {
        let id = RequestId::from(self.next_request_id);
        self.next_request_id += 1;
        id
    }

    /// The messages a reply from the client brings about.
    pub(super) fn on_response(&mut self, response: &Response) -> Vec<Message> {
        if let Some(messages) = self.on_inventory_response(response) {
            return messages;
        }
        if let Some(messages) = self.on_conf_interpreter_response(response) {
            return messages;
        }
        if let Some(messages) = self.on_bazel_manifests_response(response) {
            return messages;
        }
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
            // A document a Bazel project now lists is that project's, though
            // the scan found it before the manifests arrived.
            let documents: Vec<_> = project
                .documents
                .into_iter()
                .filter(|(doc_path, indexed)| {
                    self.locate(&indexed.path).is_some_and(|(owner, name)| {
                        name == *doc_path
                            && self.projects.get(&owner).is_some_and(|owner| {
                                owner.folder() == folder
                                    && owner.model() == &project.discovered.model
                            })
                    })
                })
                .collect();
            if let Some(recorded) = self.project_with_model(folder, &project.discovered) {
                recorded.record_scanned(documents);
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

    /// The completion at `position` in the open document `uri`: of the
    /// reference role the cursor is in, from its project's index, or else of
    /// the directive name, option or option value it is typing, from the
    /// parser's specs — or `None` when it is in neither, or the document is
    /// not open or is in no project.
    ///
    /// The list is incomplete while the workspace scan runs, so the client
    /// asks again rather than filtering a list missing most documents.
    ///
    /// `trigger` is the character whose typing asked, if one did. A space
    /// asks only where an option's value starts: anywhere else — a role's
    /// title among them — it would pop a list up at every word.
    pub(super) fn complete(
        &mut self,
        uri: &Uri,
        position: lsp_types::Position,
        trigger: Option<&str>,
    ) -> Option<CompletionResponse> {
        let text = &self.documents.get(uri)?.text;
        let lines: Vec<&str> = text.lines().collect();
        let line_index = position.line as usize;
        let line = lines.get(line_index).copied().unwrap_or_default();
        let (project, doc_path) = self.locate(&file_path(uri)?)?;
        // The document's own library's settings, as the parser reads it: a
        // Bazel site's libraries may declare different schemas and domains.
        let settings = self
            .projects
            .get(&project)?
            .model()
            .parse_settings(Some(&doc_path));
        let schema = Arc::clone(&settings.schema);
        let domain = settings.domain;
        let cursor = char_index_of(line, position.character, self.encoding);
        let encoding = self.encoding;
        let at = |index: usize| {
            let column = u32::try_from(index + 1).unwrap_or(u32::MAX);
            to_lsp_position(
                rinx_ast::Position::new(position.line + 1, column),
                line,
                encoding,
            )
        };
        let scanning = self.scan.is_some();
        if trigger == Some(" ")
            && !matches!(
                directive_at(&lines, line_index, cursor),
                Some(DirectiveContext::Value(_))
            )
        {
            return None;
        }
        let (items, incomplete) = if let Some(context) = role_at(line, cursor) {
            let range = lsp_types::Range::new(at(context.replace.start), at(context.replace.end));
            self.complete_target(uri, project, &doc_path, line_index, &context, range)?
        } else if let Some(context) = directive_at(&lines, line_index, cursor) {
            let style = self.completion;
            let index = self.projects.get_mut(&project)?.project_index();
            let vocabulary = DirectiveVocabulary {
                domain,
                schema: &schema,
                index,
            };
            directive_items(&context, &vocabulary, &at, style)
        } else if on_option_marker(&lines, line_index, cursor) {
            return None;
        } else {
            let context = role_name_at(line, cursor)?;
            let range = lsp_types::Range::new(at(context.replace.start), at(context.replace.end));
            let source_root = &self.projects.get(&project)?.model().source_root;
            let in_force = parse_prefix(
                uri,
                Some(&doc_path),
                Some(source_root),
                // A templated document is probed whole: a cut could split a
                // `{% … %}` block, and then nothing below it would render.
                if settings.jinja.is_some() {
                    text
                } else {
                    text_through_line(text, line_index)
                },
                settings,
                &self.documents,
            )
            .roles;
            let vocabulary = RoleVocabulary {
                domain,
                schema: &schema,
                in_force: &in_force,
            };
            (
                role_name_items(&context, &vocabulary, range, self.completion.format),
                false,
            )
        };
        Some(CompletionResponse::List(CompletionList {
            is_incomplete: scanning || incomplete,
            items,
        }))
    }

    /// The completion of the role target `context` describes, on the line
    /// `line_index` of the open document `uri`, each item written over
    /// `range` — and whether it is incomplete — or `None` when the role names
    /// no target the index resolves.
    ///
    /// The role is what the parser reads it as. A role resolved in the scope
    /// it is written in, and a bare one read as the default role, are learnt
    /// from a probe: the text above the cursor parsed with this target
    /// closed, which holds the `.. default-role::`, the module, class and
    /// program in force there. Each candidate is then resolved as the page
    /// would resolve it, from that document.
    fn complete_target(
        &mut self,
        uri: &Uri,
        project: ProjectId,
        doc_path: &str,
        line_index: usize,
        context: &RoleContext,
        range: lsp_types::Range,
    ) -> Option<(Vec<lsp_types::CompletionItem>, bool)> {
        let text = &self.documents.get(uri)?.text;
        let line = text.lines().nth(line_index).unwrap_or_default();
        let model = self.projects.get(&project)?.model();
        let settings = model.parse_settings(Some(doc_path));
        let schema = Arc::clone(&settings.schema);
        let named = match &context.role {
            WrittenRole::Named(name) => Some(rinx_parser::read_role(
                name,
                PLACEHOLDER,
                &rinx_parser::ParseCtx::with_domain(settings.domain).with_schema(&schema),
            )?),
            WrittenRole::Default => None,
        };
        // A bare backtick is a title reference unless a default role says
        // otherwise: the library's, or a `.. default-role::` the probe would
        // find. Prose holds many, so the probe is spared when neither can.
        if named.is_none()
            && settings.default_role.role_name().is_none()
            && !text.contains("default-role::")
            && !settings
                .prolog
                .as_ref()
                .is_some_and(|prolog| prolog.text.contains("default-role::"))
        {
            return None;
        }
        let probe = match &named {
            Some(reference) if !is_scoped(reference) => None,
            _ => Some(parse_prefix(
                uri,
                Some(doc_path),
                Some(&model.source_root),
                &probe_text(
                    text,
                    line_index,
                    &context.closed_line(line, PLACEHOLDER),
                    settings.jinja.is_some(),
                ),
                settings,
                &self.documents,
            )),
        };
        let probed = match &probe {
            Some(probe) => {
                let start = u32::try_from(context.replace.start + 1).ok()?;
                let at = rinx_ast::Position::new(u32::try_from(line_index + 1).ok()?, start);
                Some((&probe.document, reference_at(&probe.document, at)?))
            }
            None => None,
        };
        let latest = &self.tracked.get(uri)?.parsed.document;
        let (document, reference) = match (&probed, &named) {
            (Some((document, reference)), _) => (*document, *reference),
            (None, Some(reference)) => (latest, reference),
            (None, None) => return None,
        };
        let project = self.projects.get_mut(&project)?;
        let config = project.site_config();
        let (index, definitions) = project.index_and_definitions();
        let resolver = ReferenceResolver::new(document, index, &config, &schema);
        Some(target_items(&TargetQuestion {
            reference,
            resolver: &resolver,
            index,
            definitions,
            schema: &schema,
            doc_path,
            typed: &context.typed,
            range,
        }))
    }

    /// The hover for the reference at `position` in the open document `uri`
    /// — or the id written in a relation option there (see
    /// [`Self::relation_id_at`]): where the built page would link it, from
    /// its project's index. Else the hover of a `.. toctree::` entry
    /// ([`Self::toc_entry_hover`]) or of an entity's own directive
    /// ([`Self::entity_marker_hover`]), or `None` when the cursor is on none
    /// of these, the reference would be drawn broken, or the document is not
    /// open or is in no project.
    pub(super) fn hover(&mut self, uri: &Uri, position: lsp_types::Position) -> Option<Hover> {
        let Some((range, target, project)) = self
            .reference_target_at(uri, position)
            .or_else(|| self.relation_id_at(uri, position))
        else {
            return self
                .toc_entry_hover(uri, position)
                .or_else(|| self.entity_marker_hover(uri, position));
        };
        let located = written_at(
            &target,
            self.projects.get(&project)?,
            &self.documents,
            self.encoding,
        );
        let project = self.projects.get_mut(&project)?;
        let location = location_markdown(&target.destination, |_| located.as_ref().map(line_link));
        let schema = Arc::clone(&project.model().settings.parse.schema);
        let index = project.project_index();
        let entity_text = target
            .entity
            .as_ref()
            .and_then(|id| indexed_entity_markdown(id, index, &schema, &location));
        Some(reference_hover(&target, range, &location, entity_text))
    }

    /// The hover of the `.. toctree::` entry at `position` of the open
    /// document `uri`: the document it links, as a reference's hover shows
    /// one, or each document a `:glob:` entry links, one per line.
    fn toc_entry_hover(&mut self, uri: &Uri, position: lsp_types::Position) -> Option<Hover> {
        let (range, targets, project) = self.toc_entry_targets_at(uri, position)?;
        let project = self.projects.get(&project)?;
        let located: Vec<(&ReferenceTarget, String)> = targets
            .iter()
            .map(|target| {
                let located = written_at(target, project, &self.documents, self.encoding);
                let location =
                    location_markdown(&target.destination, |_| located.as_ref().map(line_link));
                (target, location)
            })
            .collect();
        Some(match located.as_slice() {
            [(target, location)] => reference_hover(target, range, location, None),
            several => targets_hover(several, range),
        })
    }

    /// The entity id written in a relation option at `position` of the open
    /// document `uri` — an entity's own or an `.. entity-update::`'s — as a
    /// reference to that entity: the id's range, the entity as its index
    /// holds it, and the project whose index that is. `None` when the cursor
    /// is on no id the index holds.
    fn relation_id_at(
        &mut self,
        uri: &Uri,
        position: lsp_types::Position,
    ) -> Option<(Range, ReferenceTarget, ProjectId)> {
        let text = &self.documents.get(uri)?.text;
        let lines: Vec<&str> = text.lines().collect();
        let line_index = position.line as usize;
        let line = *lines.get(line_index)?;
        let cursor = char_index_of(line, position.character, self.encoding);
        let Some(DirectiveContext::Value(value)) = directive_at(&lines, line_index, cursor) else {
            return None;
        };
        let tracked = self.tracked.get(uri)?;
        let id = tracked.project?;
        let project = self.projects.get_mut(&id)?;
        let settings = project
            .model()
            .parse_settings(Some(&tracked.parsed.document.path));
        let (schema, domain) = (Arc::clone(&settings.schema), settings.domain);
        let index = project.project_index();
        let vocabulary = DirectiveVocabulary {
            domain,
            schema: &schema,
            index,
        };
        let (characters, entity) = relation_target_at(&value, &vocabulary)?;
        let record = index.entities.get(&entity)?;
        let target = ReferenceTarget {
            title: record.display_text(&entity).to_string(),
            destination: Destination::Document {
                doc_path: record.doc_path.clone(),
                anchor: Some(rinx_index::entity_anchor(entity.as_str())),
            },
            entity: Some(entity.clone()),
            definition: Some(DefinitionKey::Entity(entity)),
        };
        let range = line_range(line, position.line, characters, self.encoding);
        Some((range, target, id))
    }

    /// The hover on the entity a directive marker or its `:id:` line at
    /// `position` of the open document `uri` declares — with its derived
    /// back-links, which its own text cannot show.
    fn entity_marker_hover(&mut self, uri: &Uri, position: lsp_types::Position) -> Option<Hover> {
        let text = &self.documents.get(uri)?.text;
        let lines: Vec<&str> = text.lines().collect();
        let line_index = position.line as usize;
        let line = *lines.get(line_index)?;
        let tracked = self.tracked.get(uri)?;
        let project = self.projects.get_mut(&tracked.project?)?;
        let schema = Arc::clone(
            &project
                .model()
                .parse_settings(Some(&tracked.parsed.document.path))
                .schema,
        );
        let marker = identity_marker_line(&lines, line_index)?;
        let entity = entity_marked_at(&tracked.parsed.document, u32::try_from(marker + 1).ok()?)?;
        let text = declared_entity_markdown(entity, project.project_index(), &schema);
        let indent = line.chars().take_while(|c| c.is_whitespace()).count();
        Some(markdown_hover(
            text,
            line_range(
                line,
                position.line,
                indent..line.chars().count(),
                self.encoding,
            ),
        ))
    }

    /// The page of the open document `uri`, as its project would build it,
    /// with the stylesheet and the images under `resource_root` (see
    /// [`PreviewParams`]) when they lie in a workspace folder, and each
    /// block naming the line it was written on when `source_lines` asks.
    ///
    /// Read from the document's latest parse and its project's current
    /// index, which every change brings up to date before the next message
    /// is handled, so a preview is never stale and never waits for the render
    /// tier. A document in no project — a file outside every workspace
    /// folder — is drawn on its own, with the build's defaults.
    ///
    /// [`PreviewParams`]: crate::preview::PreviewParams
    pub(super) fn preview(
        &mut self,
        uri: &Uri,
        resource_root: Option<&str>,
        source_lines: bool,
    ) -> Result<String, PreviewRefusal> {
        if self.documents.get(uri).is_none() {
            return Err(PreviewRefusal::NotOpen);
        }
        let tracked = self.tracked.get(uri).ok_or(PreviewRefusal::NotOpen)?;
        let read = |path: &Path| std::fs::read_to_string(path);
        let source_map = source_lines.then_some(&tracked.parsed.source_map);
        let Some(project) = tracked.project.and_then(|id| self.projects.get_mut(&id)) else {
            return render_preview_alone(&tracked.parsed.document, source_map, &read)
                .map_err(PreviewRefusal::Failed);
        };
        let config = project.site_config();
        let model = project.model();
        let schema = Arc::clone(&model.settings.parse.schema);
        let presentation = model.settings.presentation.clone();
        let source_root = project.root().to_path_buf();
        let document = &tracked.parsed.document;
        let bundled = model
            .parse_settings(Some(&document.path))
            .declarations
            .as_ref()
            .map(|declared| Arc::clone(&declared.bundled));
        let settings = PreviewSettings {
            config: &config,
            schema: &schema,
            presentation: &presentation,
            bundled: bundled.as_deref(),
            source_root: &source_root,
            resources: resource_root.map(|address| ResourceRoot::new(address, &self.folders)),
            source_lines: source_map,
        };
        render_preview(document, project.project_index(), &settings, &read)
            .map_err(PreviewRefusal::Failed)
    }

    /// Where the link `href` on the preview of the open document `uri`
    /// leads: the source of a document its project holds, or a file a
    /// `:download:` names that exists — `None` for any other link, a page
    /// the build writes on its own (`genindex`) among them. A document in no
    /// project leads only to itself.
    pub(super) fn preview_link(
        &mut self,
        uri: &Uri,
        href: &str,
    ) -> Result<Option<PreviewLinkResult>, PreviewRefusal> {
        if self.documents.get(uri).is_none() {
            return Err(PreviewRefusal::NotOpen);
        }
        let tracked = self.tracked.get(uri).ok_or(PreviewRefusal::NotOpen)?;
        let doc_path = &tracked.parsed.document.path;
        let Some(resolved) = resolve_page_href(doc_path, href) else {
            return Ok(None);
        };
        let Some(project) = tracked.project.and_then(|id| self.projects.get_mut(&id)) else {
            return Ok(match resolved {
                PageHref::Document {
                    doc_path: target,
                    fragment,
                } if &target == doc_path => Some(PreviewLinkResult {
                    kind: PreviewLinkKind::Document,
                    uri: uri.clone(),
                    fragment,
                }),
                _ => None,
            });
        };
        Ok(match resolved {
            PageHref::Document { doc_path, fragment } => {
                if project.project_index().find_document(&doc_path).is_none() {
                    return Ok(None);
                }
                file_uri(&project.path_of(&doc_path)).map(|uri| PreviewLinkResult {
                    kind: PreviewLinkKind::Document,
                    uri,
                    fragment,
                })
            }
            PageHref::Download { source_relative } => {
                let path = project.root().join(source_relative);
                path.is_file()
                    .then(|| file_uri(&path))
                    .flatten()
                    .map(|uri| PreviewLinkResult {
                        kind: PreviewLinkKind::File,
                        uri,
                        fragment: None,
                    })
            }
        })
    }

    /// The quick fixes for what the document `uri` fails the build on within
    /// `range`, from its latest parse — none for a document the server does
    /// not track or that is in no Bazel project.
    ///
    /// A build file is read from the disk: the server is never sent one's
    /// buffer, so an unsaved edit to it is not seen.
    pub(super) fn code_actions(&self, uri: &Uri, range: Range) -> Vec<CodeAction> {
        let Some(tracked) = self.tracked.get(uri) else {
            return Vec::new();
        };
        let Some(project) = tracked.project.and_then(|id| self.projects.get(&id)) else {
            return Vec::new();
        };
        quick_fixes(
            &tracked.parsed,
            project.model(),
            range,
            self.encoding,
            &|path| std::fs::read_to_string(path).ok(),
        )
    }

    /// The definition of the reference, the id in a relation option, or the
    /// `.. toctree::` entry at `position` in the open document `uri`: where
    /// each target the built page would link it to was written (see
    /// [`written_at`]) — every document a `:glob:` entry links — or `None`
    /// wherever [`Self::hover`] shows no target, an entity's own directive
    /// included, and for a reference leading to no document of the project.
    pub(super) fn definition(
        &mut self,
        uri: &Uri,
        position: lsp_types::Position,
    ) -> Option<GotoDefinitionResponse> {
        let (range, targets, project) = match self
            .reference_target_at(uri, position)
            .or_else(|| self.relation_id_at(uri, position))
        {
            Some((range, target, project)) => (range, vec![target], project),
            None => self.toc_entry_targets_at(uri, position)?,
        };
        let project = self.projects.get(&project)?;
        let locations = targets
            .iter()
            .filter_map(|target| written_at(target, project, &self.documents, self.encoding))
            .collect();
        reference_definition(locations, range, self.definition_links)
    }

    /// Every use of what the reference, the id in a relation option, the
    /// `.. toctree::` entry or the definition at `position` in the open
    /// document `uri` leads to — each document a `:glob:` entry links — in
    /// the project's documents, open or not, as their pages would link it:
    /// with the definition's own site when `include_declaration` is set.
    /// `None` wherever [`Self::definition`] leads nowhere and no definition
    /// was written.
    pub(super) fn references(
        &mut self,
        uri: &Uri,
        position: lsp_types::Position,
        include_declaration: bool,
    ) -> Option<Vec<lsp_types::Location>> {
        let (targets, project) = self.use_targets_at(uri, position)?;
        let project = self.projects.get_mut(&project)?;
        let mut texts: HashMap<PathBuf, Option<Cow<'_, str>>> = HashMap::new();
        let mut locations = Vec::new();
        for target in &targets {
            for site in project.uses_of(target) {
                let Some(path) = &site.path else {
                    continue;
                };
                let Some(uri) = file_uri(path) else {
                    continue;
                };
                let text = texts
                    .entry(path.clone())
                    .or_insert_with(|| file_text(&self.documents, path));
                locations.push(use_location(uri, &site, text.as_deref(), self.encoding));
            }
            if include_declaration
                && let Some(key) = &target.definition
                && let Some(site) = project.definitions().site(&target.doc_path, key)
                && let Some(uri) = file_uri(&site.path)
            {
                let text = file_text(&self.documents, &site.path);
                locations.push(site_location(
                    uri,
                    site.start,
                    site.end,
                    text.as_deref(),
                    self.encoding,
                ));
            }
        }
        Some(ordered_locations(locations))
    }

    /// What the cursor at `position` in the open document `uri` names, for
    /// [`Self::references`]: where a reference, a relation's id or a
    /// `.. toctree::` entry there leads, else the definition written there —
    /// with the project it is of.
    pub(super) fn use_targets_at(
        &mut self,
        uri: &Uri,
        position: lsp_types::Position,
    ) -> Option<(Vec<UseTarget>, ProjectId)> {
        if let Some((_, target, project)) = self
            .reference_target_at(uri, position)
            .or_else(|| self.relation_id_at(uri, position))
        {
            return Some((UseTarget::of(&target).into_iter().collect(), project));
        }
        if let Some((_, targets, project)) = self.toc_entry_targets_at(uri, position) {
            return Some((targets.iter().filter_map(UseTarget::of).collect(), project));
        }
        if let Some(named) = self.option_definition_at(uri, position) {
            return Some(named);
        }
        let at = self.position_in(uri, position)?;
        let tracked = self.tracked.get(uri)?;
        let id = tracked.project?;
        let doc_path = &tracked.parsed.document.path;
        let key = self.projects.get(&id)?.definitions().at(doc_path, at)?;
        let target = UseTarget {
            doc_path: doc_path.clone(),
            definition: Some(key),
        };
        Some((vec![target], id))
    }

    /// The definition an option line at `position` of the open document
    /// `uri` names, wherever on the line the cursor is: a directive's
    /// `:name:`, a `.. math::`'s `:label:`, an entity's `:id:` — when the
    /// project's index has the document defining it, whether or not its
    /// analysis recorded a site for it.
    fn option_definition_at(
        &mut self,
        uri: &Uri,
        position: lsp_types::Position,
    ) -> Option<(Vec<UseTarget>, ProjectId)> {
        let text = &self.documents.get(uri)?.text;
        let lines: Vec<&str> = text.lines().collect();
        let line_index = position.line as usize;
        let end = lines.get(line_index)?.chars().count();
        let Some(DirectiveContext::Value(value)) = directive_at(&lines, line_index, end) else {
            return None;
        };
        let tracked = self.tracked.get(uri)?;
        let id = tracked.project?;
        let doc_path = tracked.parsed.document.path.clone();
        let index = self.projects.get_mut(&id)?.project_index();
        let name = TargetName::new(&value.value);
        let defines = |defined_in: Option<&String>| defined_in == Some(&doc_path);
        let key = match value.option.as_str() {
            "name" if defines(index.targets.get(&name)) => DefinitionKey::Label(name),
            "label"
                if value.directive.name == "math"
                    && defines(index.equations.get(&name).map(|at| &at.doc_path)) =>
            {
                DefinitionKey::Equation(name)
            }
            "id" => {
                let entity = EntityId::new(&value.value).ok()?;
                if !defines(index.entities.get(&entity).map(|record| &record.doc_path)) {
                    return None;
                }
                DefinitionKey::Entity(entity)
            }
            _ => return None,
        };
        let target = UseTarget {
            doc_path,
            definition: Some(key),
        };
        Some((vec![target], id))
    }

    /// `position` in the open document `uri` as the parser counts it: a
    /// 1-based line and character column.
    pub(super) fn position_in(
        &self,
        uri: &Uri,
        position: lsp_types::Position,
    ) -> Option<rinx_ast::Position> {
        let text = &self.documents.get(uri)?.text;
        let line = text.lines().nth(position.line as usize).unwrap_or_default();
        let column = char_index_of(line, position.character, self.encoding) + 1;
        Some(rinx_ast::Position::new(
            position.line + 1,
            u32::try_from(column).ok()?,
        ))
    }

    /// The outline of the open document `uri`, from its latest parse — `None`
    /// when it is not open.
    pub(super) fn document_symbols(&self, uri: &Uri) -> Option<DocumentSymbols> {
        let parsed = &self.tracked.get(uri)?.parsed;
        Some(document_symbols(
            &parsed.document,
            &parsed.source_map,
            &parsed.text,
            uri,
            self.outline_shape,
            self.encoding,
        ))
    }

    /// What of the open document `uri` folds, from its latest parse —
    /// `None` when it is not open.
    pub(super) fn folding_ranges(&self, uri: &Uri) -> Option<Vec<FoldingRange>> {
        let parsed = &self.tracked.get(uri)?.parsed;
        Some(folding_ranges(
            &parsed.source_map.syntax,
            &parsed.text,
            self.folding_limit,
        ))
    }

    /// The names the open document `uri` writes, coloured by kind — those in
    /// `range` when one is asked for — from its latest parse; `None` when it
    /// is not open.
    pub(super) fn semantic_tokens(
        &self,
        uri: &Uri,
        range: Option<Range>,
    ) -> Option<SemanticTokens> {
        let parsed = &self.tracked.get(uri)?.parsed;
        Some(semantic_tokens(
            &parsed.source_map.syntax,
            &parsed.text,
            self.encoding,
            range,
        ))
    }

    /// The definitions of every project whose names match `query`, best
    /// first — in documents open or not, by their latest analyses, each
    /// placed in the file it was written in.
    pub(super) fn workspace_symbols(&self, query: &str) -> Vec<FlatSymbol> {
        let candidates: Vec<_> = self.projects.values().flat_map(Project::symbols).collect();
        let mut texts: HashMap<PathBuf, Option<Cow<'_, str>>> = HashMap::new();
        best_matches(query, &candidates)
            .iter()
            .filter_map(|candidate| {
                let path = candidate.path();
                let uri = file_uri(&path)?;
                let text = texts
                    .entry(path)
                    .or_insert_with_key(|path| file_text(&self.documents, path));
                Some(workspace_symbol(
                    candidate,
                    uri,
                    text.as_deref(),
                    self.encoding,
                ))
            })
            .collect()
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
        let at = self.position_in(uri, position)?;
        let text = &self.documents.get(uri)?.text;
        let tracked = self.tracked.get(uri)?;
        let id = tracked.project?;
        let document = &tracked.parsed.document;
        let reference = reference_at(document, at)?;
        let range = to_lsp_range(reference.span()?, text, self.encoding);
        // With the project's configuration and schema, as the render tier
        // resolves.
        let project = self.projects.get_mut(&id)?;
        let config = project.site_config();
        let schema = Arc::clone(&project.model().settings.parse.schema);
        let index = project.project_index();
        let target =
            ReferenceResolver::new(document, index, &config, &schema).resolve(reference)?;
        Some((range, target, id))
    }

    /// The `.. toctree::` entry at `position` in the open document `uri` —
    /// its range, the documents it links and the project whose index
    /// resolved them — or `None` when the cursor is on no entry, the entry
    /// links no document of the project, or the document is not open or is
    /// in no project. Read from the latest parse, as
    /// [`Self::reference_target_at`] is.
    fn toc_entry_targets_at(
        &mut self,
        uri: &Uri,
        position: lsp_types::Position,
    ) -> Option<(Range, Vec<ReferenceTarget>, ProjectId)> {
        let at = self.position_in(uri, position)?;
        let text = &self.documents.get(uri)?.text;
        let tracked = self.tracked.get(uri)?;
        let id = tracked.project?;
        let document = &tracked.parsed.document;
        let entry = toc_entry_at(document, at)?;
        let range = to_lsp_range(entry.span()?, text, self.encoding);
        let project = self.projects.get_mut(&id)?;
        let config = project.site_config();
        let schema = Arc::clone(&project.model().settings.parse.schema);
        let index = project.project_index();
        let targets =
            ReferenceResolver::new(document, index, &config, &schema).toc_entry_targets(entry);
        (!targets.is_empty()).then_some((range, targets, id))
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
        let paths: Vec<PathBuf> = changes
            .iter()
            .filter_map(|event| file_path(&event.uri))
            .collect();
        self.forget_conf_runs_importing(&paths);
        let reconfigured: BTreeSet<usize> = changes
            .iter()
            .filter_map(|event| self.folder_reconfigured_by(event))
            .collect();
        let mut messages: Vec<Message> = reconfigured
            .into_iter()
            .flat_map(|folder| self.reload_folder(folder))
            .collect();
        messages.extend(self.reload_inventory_files(&paths));
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
        let model = located
            .as_ref()
            .and_then(|(project, _)| self.projects.get(project))
            .map(Project::model);
        let settings = model.map_or(&default_settings, |model| model.parse_settings(doc_path));
        let source_root = model.map(|model| model.source_root.as_path());
        let parsed = text
            .map(|text| parse_source(uri, doc_path, source_root, &text, settings, &self.documents));
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
            let indexed = IndexedDocument::of(
                path,
                &parsed.document,
                &parsed.source_map,
                parsed.reads.paths.clone(),
            );
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
        let unrendered = RenderFindings::default();
        let rendered = tracked
            .rendered
            .as_ref()
            .map_or(&unrendered, |rendered| &rendered.findings);
        // A document in no project declares no extensions, so nothing about
        // it is lowered.
        let default_strictness = Strictness::default();
        let strictness = tracked
            .project
            .and_then(|project| self.projects.get(&project))
            .map_or(&default_strictness, Project::strictness);
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
        let model = project.model();
        let schema = Arc::clone(&model.settings.parse.schema);
        let strict_links = model.settings.strict_links;
        let Some(tracked) = self.tracked.get_mut(&uri) else {
            return Vec::new();
        };
        let document = &tracked.parsed.document;
        // The site's bundle is shared by its libraries, so any document's
        // library names it.
        let bundled = model
            .parse_settings(Some(&document.path))
            .declarations
            .as_ref()
            .map(|declared| Arc::clone(&declared.bundled));
        let settings = RenderSettings {
            config: &config,
            schema: &schema,
            strict_links,
            bundled: bundled.as_deref(),
        };
        let mut findings = render_diagnostics(document, project.project_index(), &settings);
        // What only the whole project reveals about the document is found by
        // the same fold the render read, so it is as current as the render:
        // the parse names a document by its name in the project, which is
        // what the fold keys it by. None of it fails the build.
        findings
            .reported
            .extend_from_slice(project.index_diagnostics(&document.path));
        tracked.rendered = Some(Rendered { key, findings });
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

/// Where `target`, resolved against `project`'s index, was written: the
/// range of its definition, read from the defining document's latest
/// analysis, else the start of its document's file. A definition's columns
/// are counted on its file's open buffer, else on the disk, the text that
/// analysis read — `None` as for [`target_location`].
fn written_at(
    target: &ReferenceTarget,
    project: &Project,
    documents: &DocumentStore,
    encoding: PositionEncoding,
) -> Option<lsp_types::Location> {
    let definitions = project.definitions();
    target_location(
        target,
        |doc_path| file_uri(&project.path_of(doc_path)),
        |doc_path, key| {
            let site = definitions.site(doc_path, key)?;
            let text = file_text(documents, &site.path);
            Some(site_location(
                file_uri(&site.path)?,
                site.start,
                site.end,
                text.as_deref(),
                encoding,
            ))
        },
    )
}

/// The text of the file at `path` as an analysis read it: its open buffer,
/// else the disk — `None` when neither holds it.
pub(super) fn file_text<'d>(documents: &'d DocumentStore, path: &Path) -> Option<Cow<'d, str>> {
    match documents.get_by_path(path) {
        Some(open) => Some(Cow::Borrowed(open.text.as_str())),
        None => std::fs::read_to_string(path).ok().map(Cow::Owned),
    }
}

/// The protocol range of the characters `characters` on the 0-based line
/// `line_number`, whose text is `line`.
fn line_range(
    line: &str,
    line_number: u32,
    characters: std::ops::Range<usize>,
    encoding: PositionEncoding,
) -> Range {
    let at = |index: usize| {
        let column = u32::try_from(index + 1).unwrap_or(u32::MAX);
        to_lsp_position(
            rinx_ast::Position::new(line_number + 1, column),
            line,
            encoding,
        )
    };
    Range::new(at(characters.start), at(characters.end))
}
