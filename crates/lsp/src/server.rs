//! The protocol loop, and the handlers it dispatches to.
//!
//! Split the way every subcommand is (`process_*`/`cmd_*`): the handlers are
//! pure functions from the server's state and one incoming message to the
//! messages to send back, and [`run`] is the thin loop that receives and sends.
//! A handler is therefore testable without a connection, and the loop holds no
//! logic worth testing beyond the one end-to-end conversation below.

use anyhow::{Context, Result, bail};
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit, Notification as _,
    PublishDiagnostics,
};
use lsp_types::{
    ClientCapabilities, InitializeParams, InitializeResult, PublishDiagnosticsParams,
    ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};

use std::collections::{BTreeSet, HashSet};

use crate::diagnostics::document_diagnostics;
use crate::documents::DocumentStore;
use crate::includes::IncludeGraph;
use crate::position::PositionEncoding;
use crate::uri::file_path;

/// Everything the server remembers between messages.
#[derive(Debug)]
pub struct ServerState {
    encoding: PositionEncoding,
    documents: DocumentStore,
    /// The latest diagnosis of every open document.
    graph: IncludeGraph,
    /// The URIs last published with at least one diagnostic — the ones an
    /// empty publish must reach to clear.
    shown: HashSet<Uri>,
}

impl ServerState {
    /// A server that has agreed on `encoding` and has no document open.
    #[must_use]
    pub fn new(encoding: PositionEncoding) -> Self {
        Self {
            encoding,
            documents: DocumentStore::default(),
            graph: IncludeGraph::default(),
            shown: HashSet::new(),
        }
    }

    /// Re-diagnoses the document at `changed`, which was just opened, edited
    /// or closed, and every open document that reads its file, as the
    /// notifications to send.
    ///
    /// `changed` itself is published last and always — even with nothing to
    /// report — so a client waiting for it knows every other publish this
    /// change caused came first.
    fn refresh(&mut self, changed: &Uri) -> Vec<Message> {
        let includers = file_path(changed)
            .map(|path| self.graph.includers_of(&path))
            .unwrap_or_default();
        let mut affected = self.diagnose(changed);
        for includer in includers.iter().filter(|includer| *includer != changed) {
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

    /// Parses the document at `uri` if it is open, or forgets it if not, and
    /// returns the URIs whose published diagnostics may have changed.
    fn diagnose(&mut self, uri: &Uri) -> BTreeSet<Uri> {
        let Some(document) = self.documents.get(uri) else {
            return self.graph.forget(uri);
        };
        let diagnosis = document_diagnostics(uri, &document.text, &self.documents, self.encoding);
        self.graph.record(uri.clone(), diagnosis)
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

/// The answer to `initialize` for a client announcing `capabilities`, and the
/// position encoding the two have thereby agreed on.
#[must_use]
pub fn initialize_result(
    capabilities: &ClientCapabilities,
) -> (InitializeResult, PositionEncoding) {
    let encoding = PositionEncoding::negotiate(capabilities);
    let result = InitializeResult {
        capabilities: ServerCapabilities {
            position_encoding: Some(encoding.kind()),
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            ..ServerCapabilities::default()
        },
        server_info: Some(ServerInfo {
            name: "rinx".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
    };
    (result, encoding)
}

/// The messages to send in response to `notification`.
///
/// A notification the server does not handle — or one whose parameters do not
/// parse — is ignored: a notification has no reply in which to report an
/// error, and the protocol requires ignoring what a server does not know.
pub fn handle_notification(state: &mut ServerState, notification: Notification) -> Vec<Message> {
    match notification.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let Ok(params) = notification
                .extract::<lsp_types::DidOpenTextDocumentParams>(DidOpenTextDocument::METHOD)
            else {
                return Vec::new();
            };
            let document = params.text_document;
            state
                .documents
                .open(document.uri.clone(), document.version, document.text);
            state.refresh(&document.uri)
        }
        DidChangeTextDocument::METHOD => {
            let Ok(params) = notification
                .extract::<lsp_types::DidChangeTextDocumentParams>(DidChangeTextDocument::METHOD)
            else {
                return Vec::new();
            };
            // Whole-document synchronisation: the last change is the full text.
            let Some(change) = params.content_changes.into_iter().last() else {
                return Vec::new();
            };
            let document = params.text_document;
            state
                .documents
                .replace(document.uri.clone(), document.version, change.text);
            state.refresh(&document.uri)
        }
        DidCloseTextDocument::METHOD => {
            let Ok(params) = notification
                .extract::<lsp_types::DidCloseTextDocumentParams>(DidCloseTextDocument::METHOD)
            else {
                return Vec::new();
            };
            let uri = params.text_document.uri;
            state.documents.close(&uri);
            // A closed document's own squiggles would otherwise linger in the
            // Problems view, describing text nobody is looking at; what an
            // open includer finds in it stays, and its includers now read it
            // from the disk.
            state.refresh(&uri)
        }
        _ => Vec::new(),
    }
}

/// The response to a request other than `shutdown`, which the loop answers
/// itself. No request is supported yet, so every one is refused by method.
#[must_use]
pub fn handle_request(request: Request) -> Response {
    Response::new_err(
        request.id,
        ErrorCode::MethodNotFound as i32,
        format!("rinx does not support '{}'", request.method),
    )
}

/// A `textDocument/publishDiagnostics` notification.
fn publish_diagnostics(
    uri: Uri,
    diagnostics: Vec<lsp_types::Diagnostic>,
    version: Option<i32>,
) -> Message {
    Notification::new(
        PublishDiagnostics::METHOD.to_string(),
        PublishDiagnosticsParams {
            uri,
            diagnostics,
            version,
        },
    )
    .into()
}

/// Serves `connection` from the `initialize` handshake until `exit`.
///
/// # Errors
///
/// Fails when the handshake or the shutdown sequence breaks protocol, or when
/// the client disconnects while a message is being sent.
pub fn run(connection: &Connection) -> Result<()> {
    let (initialize_id, params) = connection.initialize_start()?;
    let params: InitializeParams =
        serde_json::from_value(params).context("invalid initialize parameters")?;
    let (result, encoding) = initialize_result(&params.capabilities);
    connection.initialize_finish(initialize_id, serde_json::to_value(result)?)?;

    let mut state = ServerState::new(encoding);
    for message in &connection.receiver {
        let replies = match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    return Ok(());
                }
                vec![handle_request(request).into()]
            }
            // An `exit` after `shutdown` never reaches this loop: it is
            // consumed by `handle_shutdown` above.
            Message::Notification(notification) if notification.method == Exit::METHOD => {
                bail!("the client sent 'exit' without 'shutdown'");
            }
            Message::Notification(notification) => handle_notification(&mut state, notification),
            // The server sends no requests yet, so no response is awaited.
            Message::Response(_) => Vec::new(),
        };
        for reply in replies {
            connection.sender.send(reply)?;
        }
    }
    // The protocol asks a server to exit with code 1 unless it was shut down,
    // which the caller makes of an error; this one is a client that crashed.
    bail!("the client disconnected without 'shutdown'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_server::RequestId;
    use lsp_types::request::{Initialize, Request as _, Shutdown};
    use lsp_types::{
        DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        NumberOrString, TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
        VersionedTextDocumentIdentifier,
    };

    fn uri() -> Uri {
        "file:///docs/index.rst".parse().expect("valid uri")
    }

    fn did_open(text: &str) -> Notification {
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

    fn did_change(text: &str) -> Notification {
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

    fn did_close() -> Notification {
        Notification::new(
            DidCloseTextDocument::METHOD.to_string(),
            DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier::new(uri()),
            },
        )
    }

    /// The parameters of `message`, which must be a `publishDiagnostics`.
    fn published(message: &Message) -> PublishDiagnosticsParams {
        let Message::Notification(notification) = message else {
            panic!("expected a notification, got {message:?}");
        };
        notification
            .clone()
            .extract(PublishDiagnostics::METHOD)
            .expect("publishDiagnostics parameters")
    }

    fn codes(params: &PublishDiagnosticsParams) -> Vec<String> {
        params
            .diagnostics
            .iter()
            .filter_map(|diagnostic| match &diagnostic.code {
                Some(NumberOrString::String(code)) => Some(code.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn test_initialize_result_announces_full_sync_and_the_encoding() {
        // Given
        let capabilities = ClientCapabilities::default();

        // When
        let (result, encoding) = initialize_result(&capabilities);

        // Then
        assert_eq!(encoding, PositionEncoding::Utf16);
        assert_eq!(
            result.capabilities.text_document_sync,
            Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL))
        );
        assert_eq!(
            result.capabilities.position_encoding,
            Some(PositionEncoding::Utf16.kind())
        );
        assert_eq!(
            result.server_info.map(|info| info.name),
            Some("rinx".to_string())
        );
    }

    #[test]
    fn test_did_open_publishes_the_document_diagnostics() {
        // Given
        let mut state = ServerState::new(PositionEncoding::Utf16);

        // When
        let replies = handle_notification(&mut state, did_open(".. foo::\n"));

        // Then
        assert_eq!(replies.len(), 1);
        let params = published(&replies[0]);
        assert_eq!(params.uri, uri());
        assert_eq!(params.version, Some(1));
        assert_eq!(codes(&params), vec!["directive.unknown".to_string()]);
    }

    #[test]
    fn test_did_change_publishes_for_the_new_text() {
        // Given
        let mut state = ServerState::new(PositionEncoding::Utf16);
        handle_notification(&mut state, did_open(".. foo::\n"));

        // When
        let replies = handle_notification(&mut state, did_change("Clean prose.\n"));

        // Then
        let params = published(&replies[0]);
        assert_eq!(params.version, Some(2));
        assert_eq!(params.diagnostics, Vec::new());
    }

    #[test]
    fn test_did_change_adding_a_noqa_clears_what_it_names() {
        // Given
        let mut state = ServerState::new(PositionEncoding::Utf16);
        handle_notification(&mut state, did_open(".. foo::\n"));

        // When
        let replies = handle_notification(
            &mut state,
            did_change(".. noqa: directive.unknown\n\n.. foo::\n"),
        );

        // Then
        assert_eq!(published(&replies[0]).diagnostics, Vec::new());
    }

    #[test]
    fn test_did_close_clears_the_diagnostics() {
        // Given
        let mut state = ServerState::new(PositionEncoding::Utf16);
        handle_notification(&mut state, did_open(".. foo::\n"));

        // When
        let replies = handle_notification(&mut state, did_close());

        // Then
        let params = published(&replies[0]);
        assert_eq!(params.diagnostics, Vec::new());
        assert_eq!(params.version, None);
        assert_eq!(state.documents.get(&uri()), None);
    }

    /// A project in a scratch directory: `index.rst` and `guide.rst`
    /// include `part.rst`, which is saved holding `part`.
    struct Project {
        dir: std::path::PathBuf,
    }

    impl Project {
        fn new(name: &str, part: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("rinx_lsp_server_{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("create temp dir");
            std::fs::write(dir.join("part.rst"), part).expect("write the fragment");
            Self { dir }
        }

        fn uri(&self, name: &str) -> Uri {
            crate::uri::file_uri(&self.dir.join(name)).expect("an absolute path")
        }

        fn open(&self, name: &str, version: i32, text: &str) -> Notification {
            Notification::new(
                DidOpenTextDocument::METHOD.to_string(),
                DidOpenTextDocumentParams {
                    text_document: TextDocumentItem::new(
                        self.uri(name),
                        "restructuredtext".to_string(),
                        version,
                        text.to_string(),
                    ),
                },
            )
        }

        fn change(&self, name: &str, version: i32, text: &str) -> Notification {
            Notification::new(
                DidChangeTextDocument::METHOD.to_string(),
                DidChangeTextDocumentParams {
                    text_document: VersionedTextDocumentIdentifier::new(self.uri(name), version),
                    content_changes: vec![TextDocumentContentChangeEvent {
                        range: None,
                        range_length: None,
                        text: text.to_string(),
                    }],
                },
            )
        }

        fn close(&self, name: &str) -> Notification {
            Notification::new(
                DidCloseTextDocument::METHOD.to_string(),
                DidCloseTextDocumentParams {
                    text_document: TextDocumentIdentifier::new(self.uri(name)),
                },
            )
        }

        /// What `replies` publish, as `(file name, version, codes)` in order,
        /// with an include's summary — which has no code — as `summary`.
        fn publishes(&self, replies: &[Message]) -> Vec<(String, Option<i32>, Vec<String>)> {
            replies
                .iter()
                .map(|reply| {
                    let params = published(reply);
                    let path = crate::uri::file_path(&params.uri).expect("a file uri");
                    let name = path
                        .strip_prefix(&self.dir)
                        .expect("a file of the project")
                        .to_string_lossy()
                        .into_owned();
                    let codes = params
                        .diagnostics
                        .iter()
                        .map(|diagnostic| match &diagnostic.code {
                            Some(NumberOrString::String(code)) => code.clone(),
                            _ => SUMMARY.to_string(),
                        })
                        .collect();
                    (name, params.version, codes)
                })
                .collect()
        }
    }

    const INCLUDES_PART: &str = "Title\n=====\n\n.. include:: part.rst\n";
    const BROKEN: &str = "Fine.\n\n.. foo::\n";
    const UNKNOWN: &str = "directive.unknown";
    const SUMMARY: &str = "summary";

    fn entry(
        name: &str,
        version: Option<i32>,
        codes: &[&str],
    ) -> (String, Option<i32>, Vec<String>) {
        (
            name.to_string(),
            version,
            codes.iter().map(ToString::to_string).collect(),
        )
    }

    #[test]
    fn test_opening_an_includer_publishes_the_fragments_mistake_on_the_fragment() {
        // Given
        let project = Project::new("open_includer", BROKEN);
        let mut state = ServerState::new(PositionEncoding::Utf16);

        // When
        let replies = handle_notification(&mut state, project.open("index.rst", 1, INCLUDES_PART));

        // Then — the fragment first, the opened document last
        assert_eq!(
            project.publishes(&replies),
            vec![
                entry("part.rst", None, &[UNKNOWN]),
                entry("index.rst", Some(1), &[SUMMARY]),
            ]
        );
    }

    #[test]
    fn test_editing_an_open_fragment_re_diagnoses_its_includer() {
        // Given a fragment saved clean, and its includer open
        let project = Project::new("edit_fragment", "Fine.\n");
        let mut state = ServerState::new(PositionEncoding::Utf16);
        handle_notification(&mut state, project.open("index.rst", 1, INCLUDES_PART));
        handle_notification(&mut state, project.open("part.rst", 1, "Fine.\n"));

        // When the fragment's buffer breaks, unsaved
        let replies = handle_notification(&mut state, project.change("part.rst", 2, BROKEN));

        // Then — the mistake is found through the includer, on the fragment,
        // and summarized on the include
        assert_eq!(
            project.publishes(&replies),
            vec![
                entry("index.rst", Some(1), &[SUMMARY]),
                entry("part.rst", Some(2), &[UNKNOWN]),
            ]
        );
    }

    #[test]
    fn test_fixing_the_fragment_clears_it() {
        // Given
        let project = Project::new("fix_fragment", BROKEN);
        let mut state = ServerState::new(PositionEncoding::Utf16);
        handle_notification(&mut state, project.open("index.rst", 1, INCLUDES_PART));
        handle_notification(&mut state, project.open("part.rst", 1, BROKEN));

        // When
        let replies = handle_notification(&mut state, project.change("part.rst", 2, "Fine.\n"));

        // Then — the summary on the include goes with it
        assert_eq!(
            project.publishes(&replies),
            vec![
                entry("index.rst", Some(1), &[]),
                entry("part.rst", Some(2), &[])
            ]
        );
    }

    #[test]
    fn test_removing_the_include_clears_the_fragment() {
        // Given
        let project = Project::new("remove_include", BROKEN);
        let mut state = ServerState::new(PositionEncoding::Utf16);
        handle_notification(&mut state, project.open("index.rst", 1, INCLUDES_PART));

        // When
        let replies = handle_notification(&mut state, project.change("index.rst", 2, "Title\n"));

        // Then
        assert_eq!(
            project.publishes(&replies),
            vec![
                entry("part.rst", None, &[]),
                entry("index.rst", Some(2), &[])
            ]
        );
    }

    #[test]
    fn test_closing_the_includer_clears_a_closed_fragment() {
        // Given
        let project = Project::new("close_includer", BROKEN);
        let mut state = ServerState::new(PositionEncoding::Utf16);
        handle_notification(&mut state, project.open("index.rst", 1, INCLUDES_PART));

        // When
        let replies = handle_notification(&mut state, project.close("index.rst"));

        // Then
        assert_eq!(
            project.publishes(&replies),
            vec![entry("part.rst", None, &[]), entry("index.rst", None, &[])]
        );
    }

    #[test]
    fn test_closing_the_includer_of_an_open_fragment_shows_its_own_parse_again() {
        // Given a fragment using a substitution only its includer defines
        let project = Project::new("standalone_again", "|name|\n");
        let includer = ".. |name| replace:: rinx\n\n.. include:: part.rst\n";
        let mut state = ServerState::new(PositionEncoding::Utf16);
        handle_notification(&mut state, project.open("index.rst", 1, includer));
        let opened = handle_notification(&mut state, project.open("part.rst", 1, "|name|\n"));

        // When
        let replies = handle_notification(&mut state, project.close("index.rst"));

        // Then — included, it is clean; alone, the substitution is undefined
        assert_eq!(
            project.publishes(&opened),
            vec![entry("part.rst", Some(1), &[])]
        );
        let [first, last] = project
            .publishes(&replies)
            .try_into()
            .expect("two publishes");
        assert_eq!((first.0.as_str(), first.1), ("part.rst", Some(1)));
        assert_eq!(first.2.len(), 1, "{first:?}");
        assert_eq!(last, entry("index.rst", None, &[]));
    }

    #[test]
    fn test_closing_a_fragment_keeps_what_its_includer_finds_there() {
        // Given a fragment broken only in its unsaved buffer
        let project = Project::new("close_fragment", "Fine.\n");
        let mut state = ServerState::new(PositionEncoding::Utf16);
        handle_notification(&mut state, project.open("index.rst", 1, INCLUDES_PART));
        handle_notification(&mut state, project.open("part.rst", 1, BROKEN));

        // When its buffer is discarded
        let replies = handle_notification(&mut state, project.close("part.rst"));

        // Then — the includer reads the clean saved text again
        assert_eq!(
            project.publishes(&replies),
            vec![
                entry("index.rst", Some(1), &[]),
                entry("part.rst", None, &[])
            ]
        );
    }

    #[test]
    fn test_two_includers_report_a_shared_fragments_mistake_once() {
        // Given
        let project = Project::new("two_includers", BROKEN);
        let mut state = ServerState::new(PositionEncoding::Utf16);
        handle_notification(&mut state, project.open("index.rst", 1, INCLUDES_PART));

        // When
        let replies = handle_notification(&mut state, project.open("guide.rst", 1, INCLUDES_PART));

        // Then
        assert_eq!(
            project.publishes(&replies),
            vec![
                entry("part.rst", None, &[UNKNOWN]),
                entry("guide.rst", Some(1), &[SUMMARY]),
            ]
        );
    }

    #[test]
    fn test_a_document_including_itself_reports_the_cycle_on_itself() {
        // Given
        let project = Project::new("cycle", "");
        let mut state = ServerState::new(PositionEncoding::Utf16);

        // When
        let replies = handle_notification(
            &mut state,
            project.open("index.rst", 1, ".. include:: index.rst\n"),
        );

        // Then — the saved file does not exist, so it is the open buffer that
        // includes itself, through the overlay
        assert_eq!(
            project.publishes(&replies),
            vec![entry("index.rst", Some(1), &["include.cycle"])]
        );
    }

    #[test]
    fn test_an_unknown_notification_is_ignored() {
        // Given
        let mut state = ServerState::new(PositionEncoding::Utf16);
        let notification = Notification::new("$/unknown".to_string(), serde_json::Value::Null);

        // When
        let replies = handle_notification(&mut state, notification);

        // Then
        assert!(replies.is_empty());
    }

    #[test]
    fn test_a_notification_with_malformed_parameters_is_ignored() {
        // Given
        let mut state = ServerState::new(PositionEncoding::Utf16);
        let notification = Notification::new(
            DidOpenTextDocument::METHOD.to_string(),
            serde_json::json!({ "nonsense": true }),
        );

        // When
        let replies = handle_notification(&mut state, notification);

        // Then
        assert!(replies.is_empty());
    }

    #[test]
    fn test_a_change_or_close_with_malformed_parameters_is_ignored() {
        // Given
        let mut state = ServerState::new(PositionEncoding::Utf16);
        handle_notification(&mut state, did_open(".. foo::\n"));

        for method in [DidChangeTextDocument::METHOD, DidCloseTextDocument::METHOD] {
            // When
            let notification =
                Notification::new(method.to_string(), serde_json::json!({ "nonsense": true }));
            let replies = handle_notification(&mut state, notification);

            // Then — and the document stays open as it was
            assert!(replies.is_empty(), "{method}: {replies:?}");
            assert_eq!(
                state.documents.get(&uri()).map(|document| document.version),
                Some(1)
            );
        }
    }

    #[test]
    fn test_a_change_without_content_changes_publishes_nothing() {
        // Given
        let mut state = ServerState::new(PositionEncoding::Utf16);
        handle_notification(&mut state, did_open(".. foo::\n"));
        let empty_change = Notification::new(
            DidChangeTextDocument::METHOD.to_string(),
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(uri(), 2),
                content_changes: Vec::new(),
            },
        );

        // When
        let replies = handle_notification(&mut state, empty_change);

        // Then
        assert!(replies.is_empty(), "{replies:?}");
        assert_eq!(
            state.documents.get(&uri()).map(|document| document.version),
            Some(1)
        );
    }

    #[test]
    fn test_handle_request_refuses_an_unsupported_method() {
        // Given
        let request = Request::new(
            RequestId::from(7),
            "textDocument/hover".to_string(),
            serde_json::Value::Null,
        );

        // When
        let response = handle_request(request);

        // Then
        assert_eq!(response.id, RequestId::from(7));
        assert_eq!(
            response.response_result.err().map(|error| error.code),
            Some(ErrorCode::MethodNotFound as i32)
        );
    }

    /// Sends `initialize` and `initialized` from `client`, returning the
    /// server's answer to the first.
    fn initialize(client: &Connection) -> Response {
        client
            .sender
            .send(
                Request::new(
                    RequestId::from(1),
                    Initialize::METHOD.to_string(),
                    InitializeParams::default(),
                )
                .into(),
            )
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

    fn exit() -> Notification {
        Notification::new(
            lsp_types::notification::Exit::METHOD.to_string(),
            serde_json::Value::Null,
        )
    }

    /// The start of the first diagnostic published for `text`.
    fn first_diagnostic_start(text: &str, encoding: PositionEncoding) -> lsp_types::Position {
        let mut state = ServerState::new(encoding);
        let replies = handle_notification(&mut state, did_open(text));
        published(&replies[0]).diagnostics[0].range.start
    }

    #[test]
    fn test_did_open_counts_an_astral_character_twice_in_utf16() {
        // Given — a crab (two UTF-16 units) and a space before the reference
        let text = "🦀 |nosub|\n";

        // When
        let start = first_diagnostic_start(text, PositionEncoding::Utf16);

        // Then
        assert_eq!(start, lsp_types::Position::new(0, 3));
    }

    #[test]
    fn test_did_open_counts_an_astral_character_once_in_utf32() {
        // Given
        let text = "🦀 |nosub|\n";

        // When
        let start = first_diagnostic_start(text, PositionEncoding::Utf32);

        // Then
        assert_eq!(start, lsp_types::Position::new(0, 2));
    }

    #[test]
    fn test_did_change_of_an_unopened_document_publishes_for_it() {
        // Given — a client that skipped didOpen, against the protocol
        let mut state = ServerState::new(PositionEncoding::Utf16);

        // When
        let replies = handle_notification(&mut state, did_change(".. foo::\n"));

        // Then — leniently treated as opening it, rather than dropped
        let params = published(&replies[0]);
        assert_eq!(params.version, Some(2));
        assert_eq!(codes(&params), vec!["directive.unknown".to_string()]);
    }

    #[test]
    fn test_did_close_of_an_unopened_document_clears_its_diagnostics() {
        // Given
        let mut state = ServerState::new(PositionEncoding::Utf16);

        // When
        let replies = handle_notification(&mut state, did_close());

        // Then
        assert_eq!(published(&replies[0]).diagnostics, Vec::new());
    }

    #[test]
    fn test_run_answers_a_request_before_initialize_as_not_initialized() {
        // Given
        let (server, client) = Connection::memory();
        let handle = std::thread::spawn(move || run(&server));

        // When — the client asks something before initializing
        client
            .sender
            .send(
                Request::new(
                    RequestId::from(9),
                    "textDocument/hover".to_string(),
                    serde_json::Value::Null,
                )
                .into(),
            )
            .unwrap();
        let early = client.receiver.recv().unwrap();
        let initialized = initialize(&client);
        drop(client);

        // Then
        assert!(
            matches!(&early, Message::Response(response)
                if response.response_result.as_ref().err().map(|error| error.code)
                    == Some(ErrorCode::ServerNotInitialized as i32)),
            "{early:?}"
        );
        assert!(initialized.response_result.is_ok(), "{initialized:?}");
        let _ = handle.join().expect("server thread");
    }

    #[test]
    fn test_run_fails_when_the_client_exits_without_shutdown() {
        // Given
        let (server, client) = Connection::memory();
        let handle = std::thread::spawn(move || run(&server));
        initialize(&client);

        // When
        client.sender.send(exit().into()).unwrap();

        // Then — the protocol asks for exit code 1, which an error becomes
        let outcome = handle.join().expect("server thread");
        assert!(outcome.is_err(), "{outcome:?}");
    }

    #[test]
    fn test_run_refuses_a_request_and_ignores_a_response_mid_session() {
        // Given
        let (server, client) = Connection::memory();
        let handle = std::thread::spawn(move || run(&server));
        initialize(&client);

        // When — a request the server does not support, then a response to
        // nothing the server asked, then a document
        client
            .sender
            .send(
                Request::new(
                    RequestId::from(5),
                    "textDocument/hover".to_string(),
                    serde_json::Value::Null,
                )
                .into(),
            )
            .unwrap();
        let refused = client.receiver.recv().unwrap();
        client
            .sender
            .send(Response::new_ok(RequestId::from(99), serde_json::Value::Null).into())
            .unwrap();
        client.sender.send(did_open(".. foo::\n").into()).unwrap();
        let next = client.receiver.recv().unwrap();
        drop(client);

        // Then — the response drew no reply: the next message is the publish
        assert!(
            matches!(&refused, Message::Response(response)
                if response.id == RequestId::from(5)
                    && response.response_result.as_ref().err().map(|error| error.code)
                        == Some(ErrorCode::MethodNotFound as i32)),
            "{refused:?}"
        );
        assert_eq!(
            codes(&published(&next)),
            vec!["directive.unknown".to_string()]
        );
        let _ = handle.join().expect("server thread");
    }

    #[test]
    fn test_run_fails_when_the_client_disconnects_without_shutdown() {
        // Given
        let (server, client) = Connection::memory();
        let handle = std::thread::spawn(move || run(&server));
        initialize(&client);

        // When
        drop(client);

        // Then
        let outcome = handle.join().expect("server thread");
        assert!(outcome.is_err(), "{outcome:?}");
    }

    #[test]
    fn test_run_serves_a_whole_session() {
        // Given — a server on one end of an in-memory connection
        let (server, client) = Connection::memory();
        let handle = std::thread::spawn(move || run(&server));

        // When — the client initializes, opens a broken document, fixes it
        // and shuts down
        client
            .sender
            .send(
                Request::new(
                    RequestId::from(1),
                    Initialize::METHOD.to_string(),
                    InitializeParams::default(),
                )
                .into(),
            )
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
        client.sender.send(did_open(".. foo::\n").into()).unwrap();
        let opened = published(&client.receiver.recv().unwrap());
        client.sender.send(did_change("Fixed.\n").into()).unwrap();
        let changed = published(&client.receiver.recv().unwrap());
        client
            .sender
            .send(
                Request::new(
                    RequestId::from(2),
                    Shutdown::METHOD.to_string(),
                    serde_json::Value::Null,
                )
                .into(),
            )
            .unwrap();
        let shutdown = client.receiver.recv().unwrap();
        client
            .sender
            .send(
                Notification::new(
                    lsp_types::notification::Exit::METHOD.to_string(),
                    serde_json::Value::Null,
                )
                .into(),
            )
            .unwrap();

        // Then
        assert!(initialized.response_result.is_ok(), "{initialized:?}");
        assert_eq!(codes(&opened), vec!["directive.unknown".to_string()]);
        assert_eq!(changed.diagnostics, Vec::new());
        assert!(
            matches!(&shutdown, Message::Response(response) if response.response_result.is_ok()),
            "{shutdown:?}"
        );
        handle.join().expect("server thread").expect("clean exit");
    }

    /// Properties over whole editing sessions rather than single texts.
    mod properties {
        use super::*;
        use proptest::prelude::*;

        fn did_change_to(text: &str, version: i32) -> Notification {
            Notification::new(
                DidChangeTextDocument::METHOD.to_string(),
                DidChangeTextDocumentParams {
                    text_document: VersionedTextDocumentIdentifier::new(uri(), version),
                    content_changes: vec![TextDocumentContentChangeEvent {
                        range: None,
                        range_length: None,
                        text: text.to_string(),
                    }],
                },
            )
        }

        proptest! {
            #[test]
            fn test_every_edit_publishes_once_for_its_version(
                first in "(\\PC|\n){0,40}",
                edits in prop::collection::vec("(\\PC|\n){0,40}", 0..6),
            ) {
                // Given
                let mut state = ServerState::new(PositionEncoding::Utf16);
                let opened = handle_notification(&mut state, did_open(&first));
                prop_assert_eq!(opened.len(), 1);

                for (version, text) in (2..).zip(&edits) {
                    // When
                    let replies = handle_notification(&mut state, did_change_to(text, version));

                    // Then
                    prop_assert_eq!(replies.len(), 1);
                    prop_assert_eq!(published(&replies[0]).version, Some(version));
                }
            }
        }
    }
}
