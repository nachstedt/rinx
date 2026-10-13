//! What the server would show for every document of a folder at once, for
//! measuring a corpus — `rinx lsp --check` — rather than serving an editor.
//!
//! It is the server itself, driven in-process: the folder is scanned, every
//! document a project indexes is opened, and the render tier is run until
//! nothing is pending, so the result holds both tiers of every document. Over
//! a connection the same would need a timeout to guess when rendering is
//! done, since a render that changes nothing publishes nothing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use lsp_server::{Message, Notification};
use lsp_types::notification::{DidOpenTextDocument, Notification as _, PublishDiagnostics};
use lsp_types::{DidOpenTextDocumentParams, PublishDiagnosticsParams, TextDocumentItem, Uri};
use serde::Serialize;

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
}

/// Scans `root`, a workspace folder, opens every document its projects
/// index, renders them all, and returns what the server shows.
#[must_use]
pub fn check_folder(root: &Path) -> FolderCheck {
    let started = Instant::now();
    let mut state =
        ServerState::new(PositionEncoding::Utf16).with_workspace(vec![root.to_path_buf()], false);
    let mut messages = state.start_scan();
    let projects = scan_projects(discover_projects(root), &|_, _| {});
    messages.extend(state.on_scan_event(ScanEvent::Finished {
        folder: 0,
        projects,
        elapsed: started.elapsed(),
    }));
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
    FolderCheck {
        documents: documents.len(),
        published: last_published(&messages),
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
                ("index.rst", "Home\n====\n\n.. automodule:: spam\n"),
                ("api.rst", "API\n===\n\nSee :py:func:`spam.eggs`.\n"),
            ],
        );

        // When
        let check = check_folder(&workspace.root);

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
