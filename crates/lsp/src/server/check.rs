//! What the server would show for every document of a folder at once, for
//! measuring a corpus — `rinx lsp --check` — rather than serving an editor.
//!
//! It is the server itself, driven in-process: the folder is scanned, every
//! document a project indexes is opened, and the render tier is run until
//! nothing is pending, so the result holds both tiers of every document. Over
//! a connection the same would need a timeout to guess when rendering is
//! done, since a render that changes nothing publishes nothing.
//!
//! It also times the preview of the documents it is asked to, each right
//! after an edit, which is how the preview's latency is measured.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use lsp_server::{Message, Notification};
use lsp_types::notification::{
    DidChangeTextDocument, DidOpenTextDocument, Notification as _, PublishDiagnostics,
};
use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, Position, PublishDiagnosticsParams,
    Range, TextDocumentContentChangeEvent, TextDocumentItem, Uri, VersionedTextDocumentIdentifier,
};
use serde::Serialize;

use super::bazel_manifests::ManifestSource;
use super::conf_runs::ConfExecution;
use super::handlers::handle_notification;
use super::scan::ScanEvent;
use super::state::ServerState;
use crate::position::PositionEncoding;
use crate::project::{discover_projects, scan_projects};
use crate::uri::file_uri;

/// What the server shows for a folder's documents, all open.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FolderCheck {
    /// How many documents the folder's projects index — each opened.
    pub documents: usize,
    /// What the server last published for each file it published for: the
    /// documents, the files they include, and each project's `conf.py`.
    pub published: BTreeMap<Uri, Vec<lsp_types::Diagnostic>>,
    /// The quick fixes the server offers anywhere in each document that has
    /// any, with their edits — what a script applies to fix a build.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub quick_fixes: BTreeMap<Uri, Vec<lsp_types::CodeAction>>,
    /// How each document [`CheckOptions::previews`] names was previewed.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub previews: BTreeMap<Uri, PreviewTiming>,
}

/// How long previewing a document took right after an edit to it — the
/// latency an author waits for between typing and the page.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PreviewTiming {
    /// From the edit to the page: the parse, the fold and the render.
    pub milliseconds: f64,
    /// Why no page was rendered, if none was.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// What a check grants that an editor would — an interpreter for `conf.py`,
/// and the manifests of the folder's Bazel sites — and which previews it
/// times.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckOptions {
    /// The interpreter each `conf.py` that reading falls short for runs in.
    pub python: Option<PathBuf>,
    /// The `lsp_manifest` files of the folder's sites, their files found
    /// under the folder.
    pub manifests: Vec<PathBuf>,
    /// The documents whose preview is timed, absolute.
    pub previews: Vec<PathBuf>,
}

/// Scans `root`, a workspace folder, opens every document its projects
/// index, renders them all, and returns what the server shows — with what
/// `options` grants, as an editor allowed to would.
#[must_use]
pub fn check_folder(root: &Path, options: &CheckOptions) -> FolderCheck {
    let started = Instant::now();
    let execution = options
        .python
        .clone()
        .map_or(ConfExecution::Off, ConfExecution::Granted);
    let manifests = if options.manifests.is_empty() {
        ManifestSource::Off
    } else {
        ManifestSource::Granted(options.manifests.clone())
    };
    let mut state = ServerState::new(PositionEncoding::Utf16)
        .with_workspace(vec![root.to_path_buf()], false)
        .with_conf_execution(execution)
        .with_bazel_manifests(manifests);
    let mut messages = state.start_scan();
    let projects = scan_projects(discover_projects(root), &|_, _| {});
    messages.extend(state.on_scan_event(ScanEvent::Finished {
        folder: 0,
        projects,
        elapsed: started.elapsed(),
    }));
    messages.extend(state.finish_conf_runs());
    let documents: Vec<PathBuf> = state
        .projects
        .values()
        .flat_map(|project| project.documents().map(|(_, path)| path.to_path_buf()))
        .collect();
    for path in &documents {
        let (Some(uri), Ok(text)) = (file_uri(path), std::fs::read_to_string(path)) else {
            continue;
        };
        messages.extend(handle_notification(&mut state, did_open(uri, text)));
    }
    while state.has_pending_renders() {
        messages.extend(state.render_next());
    }
    let whole = Range::new(Position::new(0, 0), Position::new(u32::MAX, 0));
    let quick_fixes = documents
        .iter()
        .map(PathBuf::as_path)
        .filter_map(file_uri)
        .filter_map(|uri| {
            let fixes = state.code_actions(&uri, whole);
            (!fixes.is_empty()).then_some((uri, fixes))
        })
        .collect();
    let previews = options
        .previews
        .iter()
        .filter_map(|path| Some((file_uri(path)?, std::fs::read_to_string(path).ok()?)))
        .map(|(uri, text)| {
            let timing = time_preview(&mut state, &uri, text);
            (uri, timing)
        })
        .collect();
    FolderCheck {
        documents: documents.len(),
        published: last_published(&messages),
        quick_fixes,
        previews,
    }
}

/// How long previewing the open document `uri` takes after an edit leaving
/// it holding `text`.
fn time_preview(state: &mut ServerState, uri: &Uri, text: String) -> PreviewTiming {
    let started = Instant::now();
    handle_notification(state, did_change(uri.clone(), text));
    let page = state.preview(uri, None);
    PreviewTiming {
        milliseconds: started.elapsed().as_secs_f64() * 1000.0,
        error: page.err().map(|refusal| refusal.to_string()),
    }
}

/// A `didOpen` of the reStructuredText document at `uri` holding `text`.
fn did_open(uri: Uri, text: String) -> Notification {
    Notification::new(
        DidOpenTextDocument::METHOD.to_string(),
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(uri, "restructuredtext".to_string(), 1, text),
        },
    )
}

/// A `didChange` replacing the text of the document at `uri` with `text`.
fn did_change(uri: Uri, text: String) -> Notification {
    Notification::new(
        DidChangeTextDocument::METHOD.to_string(),
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri, 2),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text,
            }],
        },
    )
}

/// The diagnostics `messages` last publish for each URI.
fn last_published(messages: &[Message]) -> BTreeMap<Uri, Vec<lsp_types::Diagnostic>> {
    let mut published = BTreeMap::new();
    for message in messages {
        if let Message::Notification(notification) = message
            && notification.method == PublishDiagnostics::METHOD
            && let Ok(params) =
                serde_json::from_value::<PublishDiagnosticsParams>(notification.params.clone())
        {
            published.insert(params.uri, params.diagnostics);
        }
    }
    published
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::test_support::Workspace;
    use lsp_types::DiagnosticSeverity;

    #[test]
    fn test_check_folder_shows_both_tiers_of_every_document_and_conf_py() {
        // Given — a parse finding, a render finding, and one in `conf.py`
        let workspace = Workspace::new(
            "check_folder",
            &[
                (
                    "conf.py",
                    "import os\nextensions = ['sphinx.ext.autodoc']\nroot_doc = os.getenv('R')\n",
                ),
                // Listing `api` keeps it from being an orphan, a third tier's
                // finding this test is not about.
                (
                    "index.rst",
                    "Home\n====\n\n.. toctree::\n\n   api\n\n.. automodule:: spam\n",
                ),
                ("api.rst", "API\n===\n\nSee :py:func:`spam.eggs`.\n"),
            ],
        );

        // When
        let check = check_folder(&workspace.root, &CheckOptions::default());

        // Then
        let severities = |file: &str| -> Vec<Option<DiagnosticSeverity>> {
            check.published[&workspace.uri(file)]
                .iter()
                .map(|diagnostic| diagnostic.severity)
                .collect()
        };
        assert_eq!(check.documents, 2);
        assert_eq!(
            severities("index.rst"),
            [Some(DiagnosticSeverity::INFORMATION)]
        );
        assert_eq!(severities("api.rst"), [Some(DiagnosticSeverity::HINT)]);
        assert_eq!(severities("conf.py"), [Some(DiagnosticSeverity::WARNING)]);
    }

    #[test]
    fn test_check_folder_lists_each_documents_quick_fixes() {
        // Given — a toctree entry outside `//:index`'s deps, in a site
        // granted by its manifest
        use crate::server::bazel_manifest_tests::{library, manifest};
        let workspace = Workspace::new(
            "check_folder_quick_fixes",
            &[
                ("MODULE.bazel", "module(name = \"docs\")\n"),
                ("rinx.toml", ""),
                ("index.rst", "Home\n====\n\n.. toctree::\n\n   guide\n"),
                ("guide.rst", "Guide\n=====\n"),
                (
                    "BUILD.bazel",
                    "rinx_library(\n    name = \"index\",\n    srcs = [\"index.rst\"],\n)\n",
                ),
                (
                    "site.lsp_manifest.json",
                    &manifest(
                        "//:site",
                        vec![
                            library(
                                "//:index",
                                &["index.rst"],
                                &serde_json::json!({"allowed_docs": ["index"]}),
                            ),
                            library("//:guide", &["guide.rst"], &serde_json::json!({})),
                        ],
                    ),
                ),
            ],
        );
        let options = CheckOptions {
            manifests: vec![workspace.root.join("site.lsp_manifest.json")],
            ..CheckOptions::default()
        };

        // When
        let check = check_folder(&workspace.root, &options);

        // Then — only the document with something to fix is listed
        let titles: Vec<(&Uri, Vec<&str>)> = check
            .quick_fixes
            .iter()
            .map(|(uri, fixes)| (uri, fixes.iter().map(|fix| fix.title.as_str()).collect()))
            .collect();
        assert_eq!(
            titles,
            [(
                &workspace.uri("index.rst"),
                vec!["Add //:guide to the deps of //:index"]
            )]
        );
    }

    #[test]
    fn test_check_folder_times_the_preview_of_each_document_it_names() {
        // Given
        let workspace = Workspace::new(
            "check_folder_previews",
            &[
                ("index.rst", "Home\n====\n"),
                ("other.rst", "Other\n=====\n"),
            ],
        );
        let options = CheckOptions {
            previews: vec![workspace.root.join("index.rst")],
            ..CheckOptions::default()
        };

        // When
        let check = check_folder(&workspace.root, &options);

        // Then — only the document named, previewed without an error
        let timed: Vec<(&Uri, Option<&String>)> = check
            .previews
            .iter()
            .map(|(uri, timing)| (uri, timing.error.as_ref()))
            .collect();
        assert_eq!(timed, [(&workspace.uri("index.rst"), None)]);
        assert!(check.previews[&workspace.uri("index.rst")].milliseconds > 0.0);
    }

    #[test]
    fn test_a_check_with_no_quick_fixes_prints_none() {
        // Given
        let check = FolderCheck::default();

        // When
        let json = serde_json::to_value(&check).expect("serializes");

        // Then
        assert_eq!(json, serde_json::json!({"documents": 0, "published": {}}));
    }

    #[test]
    fn test_last_published_keeps_each_uris_latest_publish() {
        // Given
        let uri: Uri = "file:///docs/index.rst".parse().expect("a URI");
        let publish = |count: usize| -> Message {
            super::super::handlers::publish_diagnostics(
                uri.clone(),
                vec![lsp_types::Diagnostic::default(); count],
                None,
            )
        };

        // When
        let published = last_published(&[publish(2), publish(1)]);

        // Then
        assert_eq!(published[&uri].len(), 1);
    }
}
