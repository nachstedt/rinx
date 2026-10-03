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

use crate::diagnostics::document_diagnostics;
use crate::documents::DocumentStore;
use crate::position::PositionEncoding;

/// Everything the server remembers between messages.
#[derive(Debug)]
pub struct ServerState {
    encoding: PositionEncoding,
    documents: DocumentStore,
}

impl ServerState {
    /// A server that has agreed on `encoding` and has no document open.
    #[must_use]
    pub fn new(encoding: PositionEncoding) -> Self {
        Self {
            encoding,
            documents: DocumentStore::default(),
        }
    }

    /// Diagnoses the open document at `uri`, as a notification to send.
    fn publish_open_document(&self, uri: Uri) -> Option<Message> {
        let document = self.documents.get(&uri)?;
        let diagnostics = document_diagnostics(&uri, &document.text, self.encoding);
        Some(publish_diagnostics(
            uri,
            diagnostics,
            Some(document.version),
        ))
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
            state
                .publish_open_document(document.uri)
                .into_iter()
                .collect()
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
            state
                .publish_open_document(document.uri)
                .into_iter()
                .collect()
        }
        DidCloseTextDocument::METHOD => {
            let Ok(params) = notification
                .extract::<lsp_types::DidCloseTextDocumentParams>(DidCloseTextDocument::METHOD)
            else {
                return Vec::new();
            };
            let uri = params.text_document.uri;
            state.documents.close(&uri);
            // A closed document's squiggles would otherwise linger in the
            // Problems view, describing text nobody is looking at.
            vec![publish_diagnostics(uri, Vec::new(), None)]
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
}
