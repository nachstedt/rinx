//! The pure handlers: from the server's state and one incoming message to the
//! messages to send back.

use lsp_server::{ErrorCode, Message, Notification, Request, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{Completion, Request as _};
use lsp_types::{
    ClientCapabilities, CompletionOptions, CompletionParams, InitializeResult,
    PublishDiagnosticsParams, ServerCapabilities, ServerInfo, TextDocumentSyncCapability,
    TextDocumentSyncKind, Uri,
};

use super::state::ServerState;
use crate::position::PositionEncoding;

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
            // A role's target opens with a backtick, a titled one's with `<`,
            // and a `:doc:` name turns absolute with `/`.
            completion_provider: Some(CompletionOptions {
                trigger_characters: Some(["`", "<", "/"].map(String::from).to_vec()),
                ..CompletionOptions::default()
            }),
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
/// itself: a completion, or the refusal of a method rinx does not support or
/// of parameters that do not parse.
pub fn handle_request(state: &mut ServerState, request: Request) -> Response {
    let Request { id, method, params } = request;
    match method.as_str() {
        Completion::METHOD => match serde_json::from_value::<CompletionParams>(params) {
            Ok(params) => {
                let position = params.text_document_position;
                Response::new_ok(
                    id,
                    state.complete(&position.text_document.uri, position.position),
                )
            }
            Err(error) => Response::new_err(
                id,
                ErrorCode::InvalidParams as i32,
                format!("invalid '{method}' parameters: {error}"),
            ),
        },
        _ => Response::new_err(
            id,
            ErrorCode::MethodNotFound as i32,
            format!("rinx does not support '{method}'"),
        ),
    }
}

/// A `textDocument/publishDiagnostics` notification.
pub(super) fn publish_diagnostics(
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
