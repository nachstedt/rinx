//! Fixtures shared by the server's test modules: the notifications a client
//! sends and readers for the replies.

pub(super) use super::handlers::*;
pub(super) use super::run::run;
pub(super) use super::scan::ScanEvent;
pub(super) use super::state::*;
pub(super) use crate::position::PositionEncoding;
pub(super) use crate::project::scan_folder;
pub(super) use lsp_server::RequestId;
pub(super) use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
pub(super) use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    PublishDiagnostics,
};
pub(super) use lsp_types::request::{Initialize, Request as _, Shutdown};
pub(super) use lsp_types::{
    ClientCapabilities, InitializeParams, PublishDiagnosticsParams, TextDocumentSyncCapability,
    TextDocumentSyncKind, Uri,
};
pub(super) use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    NumberOrString, TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    VersionedTextDocumentIdentifier,
};
pub(super) use std::path::PathBuf;
pub(super) use std::time::Duration;

/// The `index`th project the server keeps, in the order they were found.
pub(super) fn project(state: &mut ServerState, index: usize) -> &mut crate::project::Project {
    state
        .projects
        .values_mut()
        .nth(index)
        .expect("the server keeps that many projects")
}

pub(super) fn uri() -> Uri {
    "file:///docs/index.rst".parse().expect("valid uri")
}

pub(super) fn did_open(text: &str) -> Notification {
    Notification::new(
        DidOpenTextDocument::METHOD.to_string(),
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                uri(),
                "restructuredtext".to_string(),
                1,
                text.to_string(),
            ),
        },
    )
}

pub(super) fn did_change(text: &str) -> Notification {
    Notification::new(
        DidChangeTextDocument::METHOD.to_string(),
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri(), 2),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.to_string(),
            }],
        },
    )
}

pub(super) fn did_close() -> Notification {
    Notification::new(
        DidCloseTextDocument::METHOD.to_string(),
        DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier::new(uri()),
        },
    )
}

/// The parameters of `message`, which must be a `publishDiagnostics`.
pub(super) fn published(message: &Message) -> PublishDiagnosticsParams {
    let Message::Notification(notification) = message else {
        panic!("expected a notification, got {message:?}");
    };
    notification
        .clone()
        .extract(PublishDiagnostics::METHOD)
        .expect("publishDiagnostics parameters")
}

pub(super) fn codes(params: &PublishDiagnosticsParams) -> Vec<String> {
    params
        .diagnostics
        .iter()
        .filter_map(|diagnostic| match &diagnostic.code {
            Some(NumberOrString::String(code)) => Some(code.clone()),
            _ => None,
        })
        .collect()
}

/// Sends `initialize` and `initialized` from `client`, returning the
/// server's answer to the first.
pub(super) fn initialize(client: &Connection) -> Response {
    initialize_with(client, InitializeParams::default())
}

/// [`initialize`], announcing `params`.
pub(super) fn initialize_with(client: &Connection, params: InitializeParams) -> Response {
    client
        .sender
        .send(Request::new(RequestId::from(1), Initialize::METHOD.to_string(), params).into())
        .unwrap();
    let Message::Response(initialized) = client.receiver.recv().unwrap() else {
        panic!("expected the initialize response");
    };
    client
        .sender
        .send(
            Notification::new(
                lsp_types::notification::Initialized::METHOD.to_string(),
                lsp_types::InitializedParams {},
            )
            .into(),
        )
        .unwrap();
    initialized
}

pub(super) fn exit() -> Notification {
    Notification::new(
        lsp_types::notification::Exit::METHOD.to_string(),
        serde_json::Value::Null,
    )
}

/// Every render the server has pending, run to the end, as the messages it
/// sends.
pub(super) fn render_all(state: &mut ServerState) -> Vec<Message> {
    let mut messages = Vec::new();
    while state.has_pending_renders() {
        messages.extend(state.render_next());
    }
    messages
}

/// A scanned server over `files`, with nothing open.
pub(super) fn scanned(name: &str, files: &[(&str, &str)]) -> (Workspace, ServerState) {
    let workspace = Workspace::new(name, files);
    let (mut state, _) = workspace.server(false);
    state.on_scan_event(workspace.scanned());
    (workspace, state)
}

/// The labels the `index`th project's index defines.
pub(super) fn project_labels(state: &mut ServerState, index: usize) -> Vec<String> {
    project(state, index)
        .project_index()
        .targets
        .keys()
        .map(|name| name.as_str().to_string())
        .collect()
}

/// The `rinx/status` parameters among `messages`, in order.
pub(super) fn statuses(messages: &[Message]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::Notification(notification)
                if notification.method == crate::progress::STATUS_METHOD =>
            {
                Some(notification.params.clone())
            }
            _ => None,
        })
        .collect()
}

/// A workspace folder in a scratch directory, holding `files`.
pub(super) struct Workspace {
    pub(super) root: PathBuf,
}

impl Workspace {
    pub(super) fn new(name: &str, files: &[(&str, &str)]) -> Self {
        let root = std::env::temp_dir().join(format!("rinx_lsp_workspace_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create the folder");
        let workspace = Self { root };
        for (file, text) in files {
            workspace.write(file, text);
        }
        workspace
    }

    pub(super) fn write(&self, file: &str, text: &str) {
        let path = self.root.join(file);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    }

    /// Deletes `file`, a file or a directory, from the disk.
    pub(super) fn remove(&self, file: &str) {
        let path = self.root.join(file);
        if path.is_dir() {
            std::fs::remove_dir_all(path).expect("remove a directory");
        } else {
            std::fs::remove_file(path).expect("remove a file");
        }
    }

    /// Renames `from` to `to` on the disk.
    pub(super) fn rename(&self, from: &str, to: &str) {
        std::fs::rename(self.root.join(from), self.root.join(to)).expect("rename");
    }

    /// The `didChangeWatchedFiles` notification reporting `changes`.
    pub(super) fn watched(&self, changes: &[(&str, lsp_types::FileChangeType)]) -> Notification {
        Notification::new(
            lsp_types::notification::DidChangeWatchedFiles::METHOD.to_string(),
            lsp_types::DidChangeWatchedFilesParams {
                changes: changes
                    .iter()
                    .map(|(file, typ)| lsp_types::FileEvent::new(self.uri(file), *typ))
                    .collect(),
            },
        )
    }

    pub(super) fn uri(&self, file: &str) -> Uri {
        crate::uri::file_uri(&self.root.join(file)).expect("an absolute path")
    }

    /// A server indexing this folder, with the scan started but not run.
    pub(super) fn server(&self, progress: bool) -> (ServerState, Vec<Message>) {
        let mut state = ServerState::new(PositionEncoding::Utf16)
            .with_workspace(vec![self.root.clone()], progress);
        let started = state.start_scan();
        (state, started)
    }

    /// The scan of this folder as it stands on disk now, as the event the
    /// scan thread would send.
    pub(super) fn scanned(&self) -> ScanEvent {
        ScanEvent::Finished {
            folder: 0,
            projects: scan_folder(&self.root, &|_, _| {}),
            elapsed: Duration::from_millis(800),
        }
    }

    pub(super) fn open(&self, file: &str, text: &str) -> Notification {
        Notification::new(
            DidOpenTextDocument::METHOD.to_string(),
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem::new(
                    self.uri(file),
                    "restructuredtext".to_string(),
                    1,
                    text.to_string(),
                ),
            },
        )
    }

    pub(super) fn change(&self, file: &str, version: i32, text: &str) -> Notification {
        Notification::new(
            DidChangeTextDocument::METHOD.to_string(),
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(self.uri(file), version),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: text.to_string(),
                }],
            },
        )
    }

    pub(super) fn close(&self, file: &str) -> Notification {
        Notification::new(
            DidCloseTextDocument::METHOD.to_string(),
            DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier::new(self.uri(file)),
            },
        )
    }

    /// The codes `replies` publish for `file`, in its last publish, or `None`
    /// when nothing was published for it.
    pub(super) fn codes_for(&self, replies: &[Message], file: &str) -> Option<Vec<String>> {
        let uri = self.uri(file);
        replies
            .iter()
            .filter_map(|reply| match reply {
                Message::Notification(notification)
                    if notification.method == PublishDiagnostics::METHOD =>
                {
                    Some(published(reply))
                }
                _ => None,
            })
            .filter(|params| params.uri == uri)
            .last()
            .map(|params| codes(&params))
    }
}
