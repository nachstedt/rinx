//! Fixtures shared by the server's test modules: the notifications a client
//! sends and readers for the replies.

pub(super) use super::handlers::*;
pub(super) use super::run::run;
pub(super) use super::state::*;
pub(super) use crate::position::PositionEncoding;
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
